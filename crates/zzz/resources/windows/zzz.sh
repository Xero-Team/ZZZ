#!/usr/bin/env sh

if [ "$ZZZ_WSL_DEBUG_INFO" = true ]; then
	set -x
fi

ZZZ_PATH="$(dirname "$(realpath "$0")")"

resolve_zzz_exe() {
    for candidate in \
        "$ZZZ_PATH/zzz.exe" \
        "$ZZZ_PATH/../ZZZ.exe" \
        "$ZZZ_PATH/../zzz.exe"
    do
        if [ -f "$candidate" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    echo "zzz: could not find ZZZ executable" >&2
    echo "Looked for:" >&2
    echo "  $ZZZ_PATH/zzz.exe" >&2
    echo "  $ZZZ_PATH/../ZZZ.exe" >&2
    echo "  $ZZZ_PATH/../zzz.exe" >&2
    return 1
}

ZZZ_EXE="$(resolve_zzz_exe)" || exit $?

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
    "$ZZZ_EXE" --wsl "$WSL_USER@$WSL_DISTRO_NAME" "$@"
    exit $?
else
    "$ZZZ_EXE" "$@"
    exit $?
fi
