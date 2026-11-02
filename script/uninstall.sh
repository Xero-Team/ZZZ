#!/usr/bin/env sh
set -eu

# Uninstalls ZZZ that was installed using the install.sh script.

data_home() {
    if [ -n "${FLATPAK_XDG_DATA_HOME:-}" ]; then
        printf '%s\n' "$FLATPAK_XDG_DATA_HOME"
    elif [ -n "${XDG_DATA_HOME:-}" ]; then
        printf '%s\n' "$XDG_DATA_HOME"
    else
        printf '%s\n' "$HOME/.local/share"
    fi
}

config_home() {
    if [ -n "${FLATPAK_XDG_CONFIG_HOME:-}" ]; then
        printf '%s\n' "$FLATPAK_XDG_CONFIG_HOME"
    elif [ -n "${XDG_CONFIG_HOME:-}" ]; then
        printf '%s\n' "$XDG_CONFIG_HOME"
    else
        printf '%s\n' "$HOME/.config"
    fi
}

app_path_for_channel() {
    case "$1" in
        stable) printf '%s\n' "ZZZ.app" ;;
        nightly) printf '%s\n' "ZZZ Nightly.app" ;;
        preview) printf '%s\n' "ZZZ Preview.app" ;;
        dev) printf '%s\n' "ZZZ Dev.app" ;;
    esac
}

linux_app_path_for_channel() {
    case "$1" in
        stable) printf '%s\n' "$HOME/.local/zzz.app" ;;
        nightly) printf '%s\n' "$HOME/.local/zzz-nightly.app" ;;
        preview) printf '%s\n' "$HOME/.local/zzz-preview.app" ;;
        dev) printf '%s\n' "$HOME/.local/zzz-dev.app" ;;
    esac
}

app_is_installed() {
    candidate_channel="$1"
    if [ "$platform" = "macos" ]; then
        [ -d "/Applications/$(app_path_for_channel "$candidate_channel")" ]
    else
        [ -d "$(linux_app_path_for_channel "$candidate_channel")" ]
    fi
}

check_remaining_installations() {
    for candidate_channel in stable nightly preview dev; do
        if app_is_installed "$candidate_channel"; then
            return 1
        fi
    done
    return 0
}

scope_has_remaining_installation() {
    scope="$1"
    case "$scope" in
        stable) candidates="stable nightly preview" ;;
        dev) candidates="dev" ;;
        *) return 1 ;;
    esac

    for candidate_channel in $candidates; do
        if app_is_installed "$candidate_channel"; then
            return 0
        fi
    done
    return 1
}

remove_binary_symlink() {
    app_directory="$1"
    binary_link="$HOME/.local/bin/zzz"

    [ -L "$binary_link" ] || return 0

    link_target="$(readlink "$binary_link")"
    case "$link_target" in
        "$app_directory/bin/zzz"|"$app_directory/bin/cli"|\
        "$app_directory/Contents/MacOS/zzz"|"$app_directory/Contents/MacOS/ZZZ")
            rm -f "$binary_link"
            ;;
    esac
}

prompt_remove_preferences() {
    printf "Do you want to keep your ZZZ preferences? [Y/n] "
    read -r response || response=""
    case "$response" in
        [nN]|[nN][oO])
            rm -rf "$config_dir"
            echo "Preferences removed."
            ;;
        *)
            echo "Preferences kept."
            ;;
    esac
}

main() {
    platform="$(uname -s)"
    channel="${ZZZ_CHANNEL:-stable}"

    case "$channel" in
        stable|nightly|preview|dev) ;;
        *)
            echo "Unknown release channel: $channel" >&2
            exit 1
            ;;
    esac

    case "$platform" in
        Darwin) platform="macos" ;;
        Linux) platform="linux" ;;
        *)
            echo "Unsupported platform $platform" >&2
            exit 1
            ;;
    esac

    if [ "$platform" = "macos" ]; then
        data_dir="$HOME/Library/Application Support/ZZZ"
    else
        data_dir="$(data_home)/zzz"
    fi
    config_dir="$(config_home)/ZZZ"

    "$platform"

    echo "ZZZ has been uninstalled"
}

linux() {
    suffix=""
    if [ "$channel" != "stable" ]; then
        suffix="-$channel"
    fi

    case "$channel" in
        stable)
            appid="dev.zzz.ZZZ"
            db_scope="stable"
            ;;
        nightly)
            appid="dev.zzz.ZZZ-Nightly"
            db_scope="stable"
            ;;
        preview)
            appid="dev.zzz.ZZZ-Preview"
            db_scope="stable"
            ;;
        dev)
            appid="dev.zzz.ZZZ-Dev"
            db_scope="dev"
            ;;
    esac

    app_directory="$HOME/.local/zzz$suffix.app"

    remove_binary_symlink "$app_directory"
    rm -rf "$app_directory"
    rm -f "$HOME/.local/share/applications/${appid}.desktop"

    if ! scope_has_remaining_installation "$db_scope"; then
        rm -rf "$data_dir/db/0-$db_scope"
        rm -f "$data_dir/zzz-$db_scope.sock"
    fi

    if check_remaining_installations; then
        rm -rf "$data_dir"
        rm -rf "$HOME/.zzz_server" "$HOME/.zzz_wsl_server"
        prompt_remove_preferences
    fi
}

macos() {
    app="$(app_path_for_channel "$channel")"
    case "$channel" in
        stable)
            app_id="dev.zzz.ZZZ"
            db_scope="stable"
            ;;
        nightly)
            app_id="dev.zzz.ZZZ-Nightly"
            db_scope="stable"
            ;;
        preview)
            app_id="dev.zzz.ZZZ-Preview"
            db_scope="stable"
            ;;
        dev)
            app_id="dev.zzz.ZZZ-Dev"
            db_scope="dev"
            ;;
    esac

    app_directory="/Applications/$app"

    remove_binary_symlink "$app_directory"
    rm -rf "$app_directory"

    if ! scope_has_remaining_installation "$db_scope"; then
        rm -rf "$data_dir/db/0-$db_scope"
    fi

    rm -rf "$HOME/Library/Application Support/com.apple.sharedfilelist/com.apple.LSSharedFileList.ApplicationRecentDocuments/$app_id.sfl"*
    rm -rf "$HOME/Library/Caches/$app_id"
    rm -rf "$HOME/Library/HTTPStorages/$app_id"
    rm -rf "$HOME/Library/Preferences/$app_id.plist"
    rm -rf "$HOME/Library/Saved Application State/$app_id.savedState"

    if check_remaining_installations; then
        rm -rf "$data_dir"
        rm -rf "$HOME/Library/Logs/ZZZ"
        rm -rf "$HOME/.zzz_server" "$HOME/.zzz_wsl_server"
        prompt_remove_preferences
    fi
}

main "$@"
