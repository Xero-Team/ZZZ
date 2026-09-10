#!/usr/bin/env sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
uninstall_script="$script_dir/uninstall.sh"
test_root=$(mktemp -d "${TMPDIR:-/tmp}/zzz-uninstall-test.XXXXXX")
trap 'rm -rf "$test_root"' EXIT HUP INT TERM

fail() {
    echo "test-uninstall: $*" >&2
    exit 1
}

assert_exists() {
    [ -e "$1" ] || [ -L "$1" ] || fail "expected $1 to exist"
}

assert_missing() {
    [ ! -e "$1" ] && [ ! -L "$1" ] || fail "expected $1 to be absent"
}

run_uninstall() {
    test_home="$1"
    test_data_home="$2"
    test_config_home="$3"
    test_channel="$4"
    test_input="$5"

    printf '%s\n' "$test_input" | HOME="$test_home" \
        XDG_DATA_HOME="$test_data_home" \
        XDG_CONFIG_HOME="$test_config_home" \
        ZED_CHANNEL="$test_channel" \
        sh "$uninstall_script" >/dev/null
}

test_multi_channel_and_xdg_isolation() {
    test_home="$test_root/home"
    test_data_home="$test_root/custom data"
    test_config_home="$test_root/custom config"
    zzz_data="$test_data_home/zzz"
    zed_data="$test_data_home/zed"

    mkdir -p "$test_home/.local/zzz.app/bin"
    mkdir -p "$test_home/.local/zzz-nightly.app/bin"
    mkdir -p "$test_home/.local/zed.app/bin"
    mkdir -p "$test_home/.local/share/applications"
    mkdir -p "$test_home/.local/bin"
    mkdir -p "$zzz_data/db/0-stable" "$zzz_data/db/0-dev"
    mkdir -p "$zed_data/db/0-stable"
    mkdir -p "$test_config_home/ZZZ"
    : > "$zzz_data/zed-stable.sock"
    : > "$zzz_data/zed-dev.sock"
    : > "$zed_data/zed-stable.sock"
    : > "$test_home/.local/share/applications/dev.zzz.ZZZ.desktop"
    : > "$test_home/.local/share/applications/dev.zzz.ZZZ-Nightly.desktop"
    : > "$test_home/.local/zed.app/bin/zed"
    ln -s "$test_home/.local/zed.app/bin/zed" "$test_home/.local/bin/zzz"

    run_uninstall "$test_home" "$test_data_home" "$test_config_home" stable ''

    assert_missing "$test_home/.local/zzz.app"
    assert_missing "$test_home/.local/share/applications/dev.zzz.ZZZ.desktop"
    assert_exists "$test_home/.local/zzz-nightly.app"
    assert_exists "$zzz_data/db/0-stable"
    assert_exists "$zzz_data/zed-stable.sock"
    assert_exists "$zzz_data/db/0-dev"
    assert_exists "$test_home/.local/bin/zzz"
    assert_exists "$zed_data/db/0-stable"
    assert_exists "$zed_data/zed-stable.sock"
    assert_exists "$test_config_home/ZZZ"

    run_uninstall "$test_home" "$test_data_home" "$test_config_home" nightly n

    assert_missing "$test_home/.local/zzz-nightly.app"
    assert_missing "$test_home/.local/share/applications/dev.zzz.ZZZ-Nightly.desktop"
    assert_missing "$zzz_data"
    assert_missing "$test_config_home/ZZZ"
    assert_exists "$zed_data/db/0-stable"
    assert_exists "$zed_data/zed-stable.sock"
}

test_dev_scope_isolated_from_stable() {
    test_home="$test_root/dev-home"
    test_data_home="$test_root/dev-data"
    test_config_home="$test_root/dev-config"
    zzz_data="$test_data_home/zzz"

    mkdir -p "$test_home/.local/zzz.app/bin"
    mkdir -p "$test_home/.local/zzz-dev.app/bin"
    mkdir -p "$zzz_data/db/0-stable" "$zzz_data/db/0-dev"
    : > "$zzz_data/zed-stable.sock"
    : > "$zzz_data/zed-dev.sock"

    run_uninstall "$test_home" "$test_data_home" "$test_config_home" dev ''

    assert_missing "$test_home/.local/zzz-dev.app"
    assert_missing "$zzz_data/db/0-dev"
    assert_missing "$zzz_data/zed-dev.sock"
    assert_exists "$test_home/.local/zzz.app"
    assert_exists "$zzz_data/db/0-stable"
    assert_exists "$zzz_data/zed-stable.sock"
}

test_unknown_channel_does_not_delete_data() {
    test_home="$test_root/unknown-home"
    test_data_home="$test_root/unknown-data"
    test_config_home="$test_root/unknown-config"
    zzz_data="$test_data_home/zzz"

    mkdir -p "$test_home/.local/zzz.app"
    mkdir -p "$zzz_data/db/0-stable"

    if HOME="$test_home" XDG_DATA_HOME="$test_data_home" \
        XDG_CONFIG_HOME="$test_config_home" ZED_CHANNEL=invalid \
        sh "$uninstall_script" >/dev/null 2>&1; then
        fail "unknown channel unexpectedly succeeded"
    fi

    assert_exists "$test_home/.local/zzz.app"
    assert_exists "$zzz_data/db/0-stable"
}

test_macos_data_isolation() {
    test_home="$test_root/macos-home"
    test_bin="$test_root/macos-bin"

    mkdir -p "$test_bin"
    mkdir -p "$test_home/Library/Application Support/ZZZ/db/0-stable"
    mkdir -p "$test_home/Library/Application Support/Zed/db/0-stable"
    mkdir -p "$test_home/Library/Logs/ZZZ"
    mkdir -p "$test_home/Library/Logs/Zed"
    printf '%s\n' '#!/usr/bin/env sh' 'echo Darwin' > "$test_bin/uname"
    chmod +x "$test_bin/uname"

    printf '\n' | HOME="$test_home" PATH="$test_bin:$PATH" ZED_CHANNEL=stable \
        sh "$uninstall_script" >/dev/null

    assert_missing "$test_home/Library/Application Support/ZZZ"
    assert_missing "$test_home/Library/Logs/ZZZ"
    assert_exists "$test_home/Library/Application Support/Zed/db/0-stable"
    assert_exists "$test_home/Library/Logs/Zed"
}

test_multi_channel_and_xdg_isolation
test_dev_scope_isolated_from_stable
test_unknown_channel_does_not_delete_data
test_macos_data_isolation
