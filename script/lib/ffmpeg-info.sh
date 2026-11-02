#!/usr/bin/env bash

# Prints `<series>|<asset>|<release tag>` for the FFmpeg package that
# `script/ffmpeg` installed, so generated license text names the bundled build
# instead of a version that was pinned when the script was written.

function ffmpeg_dependency_info
{
    local marker="" candidate
    local series="" archive="" tag=""

    for candidate in .deps/ffmpeg/*/.zzz-ffmpeg.json; do
        if [[ -f "$candidate" ]]; then
            marker="$candidate"
            break
        fi
    done

    if [[ -n "$marker" ]]; then
        series="$(sed -n 's/.*"ffmpeg_series": "\([^"]*\)".*/\1/p' "$marker" | head -n 1)"
        archive="$(sed -n 's/.*"archive": "\([^"]*\)".*/\1/p' "$marker" | head -n 1)"
        tag="$(sed -n 's/.*"release_tag": "\([^"]*\)".*/\1/p' "$marker" | head -n 1)"
    fi

    printf '%s|%s|%s\n' "${series:-9.0}" "${archive:-unknown build}" "${tag:-latest}"
}
