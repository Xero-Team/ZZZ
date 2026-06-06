#!/usr/bin/env sh

if [ "$ZED_WSL_DEBUG_INFO" = true ]; then
	set -x
fi

ZED_PATH="$(dirname "$(realpath "$0")")"

resolve_zed_exe() {
    for candidate in \
        "$ZED_PATH/zzz.exe" \
        "$ZED_PATH/../ZZZ.exe" \
        "$ZED_PATH/../zzz.exe"
    do
        if [ -f "$candidate" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    echo "zzz: could not find ZZZ executable" >&2
    echo "Looked for:" >&2
    echo "  $ZED_PATH/zzz.exe" >&2
    echo "  $ZED_PATH/../ZZZ.exe" >&2
    echo "  $ZED_PATH/../zzz.exe" >&2
    return 1
}

ZED_EXE="$(resolve_zed_exe)" || exit $?

IN_WSL=false
if [ -n "$WSL_DISTRO_NAME" ]; then
	# $WSL_DISTRO_NAME is available since WSL builds 18362, also for WSL2
	IN_WSL=true
fi

if [ $IN_WSL = true ]; then
    WSL_USER="$USER"
    if [ -z "$WSL_USER" ]; then
        WSL_USER="$USERNAME"
    fi
    "$ZED_EXE" --wsl "$WSL_USER@$WSL_DISTRO_NAME" "$@"
    exit $?
else
    "$ZED_EXE" "$@"
    exit $?
fi
