#!/usr/bin/env sh
set -eu

# ZZZ does not ship hosted binaries. This script unpacks a local tarball
# when ZZZ_BUNDLE_PATH is set (used by script/install-linux). It must not
# download a remote release.

print_no_hosted_binaries() {
    cat <<'EOF'
ZZZ has no hosted binaries.

Build from this repository:

    ./script/install-linux

Or run a local debug build:

    cargo run

See the local build guides:

    docs/src/development/macos.md
    docs/src/development/linux.md
    docs/src/development/windows.md
    docs/src/installation.md
EOF
}

main() {
    if [ -z "${ZZZ_BUNDLE_PATH:-}" ]; then
        print_no_hosted_binaries
        exit 1
    fi

    if [ ! -f "$ZZZ_BUNDLE_PATH" ]; then
        echo "Local bundle not found: $ZZZ_BUNDLE_PATH" >&2
        echo "Build one with ./script/install-linux" >&2
        exit 1
    fi

    platform="$(uname -s)"
    arch="$(uname -m)"
    channel="${ZZZ_CHANNEL:-stable}"
    if [ -n "${TMPDIR:-}" ] && [ -d "${TMPDIR}" ]; then
        temp="$(mktemp -d "$TMPDIR/zzz-XXXXXX")"
    else
        temp="$(mktemp -d "/tmp/zzz-XXXXXX")"
    fi
    trap 'rm -rf "$temp"' EXIT

    if [ "$platform" = "Linux" ]; then
        platform="linux"
    else
        echo "Local bundle install is only implemented for Linux." >&2
        print_no_hosted_binaries
        exit 1
    fi

    case "$platform-$arch" in
        linux-arm64* | linux-aarch64)
            arch="aarch64"
            ;;
        linux-x86*)
            arch="x86_64"
            ;;
        *)
            echo "Unsupported platform or architecture: $platform-$arch" >&2
            exit 1
            ;;
    esac

    linux

    if [ "$(command -v zzz)" = "$HOME/.local/bin/zzz" ]; then
        echo "ZZZ has been installed. Run with 'zzz'"
    else
        echo "To run ZZZ from your terminal, you must add ~/.local/bin to your PATH"
        echo "Run:"

        case "$SHELL" in
            *zsh)
                echo "   echo 'export PATH=\$HOME/.local/bin:\$PATH' >> ~/.zshrc"
                echo "   source ~/.zshrc"
                ;;
            *fish)
                echo "   fish_add_path -U $HOME/.local/bin"
                ;;
            *)
                echo "   echo 'export PATH=\$HOME/.local/bin:\$PATH' >> ~/.bashrc"
                echo "   source ~/.bashrc"
                ;;
        esac

        echo "To run ZZZ now, '~/.local/bin/zzz'"
    fi
}

linux() {
    cp "$ZZZ_BUNDLE_PATH" "$temp/zzz-linux-$arch.tar.gz"

    suffix=""
    if [ "$channel" != "stable" ]; then
        suffix="-$channel"
    fi

    appid=""
    case "$channel" in
      stable)
        appid="dev.zzz.ZZZ"
        ;;
      nightly)
        appid="dev.zzz.ZZZ-Nightly"
        ;;
      preview)
        appid="dev.zzz.ZZZ-Preview"
        ;;
      dev)
        appid="dev.zzz.ZZZ-Dev"
        ;;
      *)
        echo "Unknown release channel: ${channel}. Using stable app ID."
        appid="dev.zzz.ZZZ"
        ;;
    esac

    rm -rf "$HOME/.local/zzz$suffix.app"
    mkdir -p "$HOME/.local/zzz$suffix.app"
    tar -xzf "$temp/zzz-linux-$arch.tar.gz" -C "$HOME/.local/"

    zzz_editor="$HOME/.local/zzz$suffix.app/libexec/zzz-editor"
    if [ -f "$zzz_editor" ] && command -v ldd >/dev/null 2>&1; then
        missing="$(ldd "$zzz_editor" 2>/dev/null | sed -n 's/^[[:space:]]*\(.*\) => not found$/\1/p')"
        if [ -n "$missing" ]; then
            echo "Warning: your system is missing libraries that ZZZ needs:"
            echo "$missing" | sed 's/^/    /'
            echo "Install them with your package manager, or ZZZ will fail to start."
        fi
    fi

    mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"

    if [ -f "$HOME/.local/zzz$suffix.app/bin/zzz" ]; then
        ln -sf "$HOME/.local/zzz$suffix.app/bin/zzz" "$HOME/.local/bin/zzz"
    else
        ln -sf "$HOME/.local/zzz$suffix.app/bin/cli" "$HOME/.local/bin/zzz"
    fi

    desktop_file_path="$HOME/.local/share/applications/${appid}.desktop"
    src_dir="$HOME/.local/zzz$suffix.app/share/applications"
    if [ -f "$src_dir/${appid}.desktop" ]; then
        cp "$src_dir/${appid}.desktop" "${desktop_file_path}"
    else
        cp "$src_dir/zzz$suffix.desktop" "${desktop_file_path}"
    fi
    sed -i "s|Icon=zzz|Icon=$HOME/.local/zzz$suffix.app/share/icons/hicolor/512x512/apps/zzz.png|g" "${desktop_file_path}"
    sed -i "s|Exec=zzz|Exec=$HOME/.local/zzz$suffix.app/bin/zzz|g" "${desktop_file_path}"
}

main "$@"
