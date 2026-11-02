#!/usr/bin/env bash

# Helpers for resolving the newest GitHub release asset, so build scripts use
# current tool releases instead of hand-pinned versions and checksums.
#
# Requires curl, plus jq or python3 for the JSON responses. Set GITHUB_TOKEN to
# raise the GitHub API rate limit.

function github_api_json
{
    local path="$1"
    local -a headers=(
        --header "User-Agent: ZZZ-Setup"
        --header "Accept: application/vnd.github+json"
    )
    local token="${GITHUB_TOKEN:-${GH_TOKEN:-}}"
    if [[ -n "$token" ]]; then
        headers+=(--header "Authorization: Bearer ${token}")
    fi
    curl --silent --show-error --fail --location --retry 3 --retry-all-errors \
        "${headers[@]}" "https://api.github.com${path}"
}

function github_json_tag
{
    # Reads a release JSON document and prints its tag name.
    if command -v jq >/dev/null 2>&1; then
        jq --raw-output '.tag_name // ""'
    elif command -v python3 >/dev/null 2>&1; then
        python3 -c 'import json, sys; print(json.load(sys.stdin).get("tag_name") or "")'
    else
        echo "jq or python3 is required to read GitHub release metadata." >&2
        return 1
    fi
}

function github_json_assets
{
    # Reads a release JSON document and prints one row per asset:
    # <tag>|<asset name>|<sha256>|<download url>
    #
    # `|` separates fields because IFS collapses runs of whitespace, which
    # would drop an empty checksum column.
    if command -v jq >/dev/null 2>&1; then
        jq --raw-output '.tag_name as $tag
            | .assets[]?
            | [$tag, .name, (.digest // "" | sub("^sha256:"; "")), .browser_download_url]
            | join("|")'
    elif command -v python3 >/dev/null 2>&1; then
        python3 -c '
import json, sys

release = json.load(sys.stdin)
tag = release.get("tag_name") or ""
for asset in release.get("assets") or []:
    digest = asset.get("digest") or ""
    if digest.startswith("sha256:"):
        digest = digest[len("sha256:"):]
    print("|".join([tag, asset.get("name") or "", digest, asset.get("browser_download_url") or ""]))
'
    else
        echo "jq or python3 is required to read GitHub release metadata." >&2
        return 1
    fi
}

function github_latest_tag
{
    # <repository>
    github_api_json "/repos/$1/releases/latest" | github_json_tag
}

function github_asset
{
    # <repository> <glob pattern> [tag]
    # Prints: <tag>|<asset name>|<sha256>|<download url>
    local repository="$1"
    local pattern="$2"
    local tag="${3:-}"
    local path="/repos/${repository}/releases/latest"
    local json asset_tag name digest url

    if [[ -n "$tag" ]]; then
        path="/repos/${repository}/releases/tags/${tag}"
    fi
    json="$(github_api_json "$path")" || return 1

    while IFS='|' read -r asset_tag name digest url; do
        # shellcheck disable=SC2053 # intent is glob matching on the asset name
        if [[ "$name" == $pattern ]]; then
            printf '%s|%s|%s|%s\n' "$asset_tag" "$name" "$digest" "$url"
            return 0
        fi
    done < <(printf '%s\n' "$json" | github_json_assets)

    echo "No ${repository} release asset matches '${pattern}'." >&2
    return 1
}

function github_latest_asset
{
    # <repository> <glob pattern>
    github_asset "$1" "$2"
}

function sha256_of
{
    local path="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$path" | cut -d ' ' -f 1
    else
        shasum -a 256 "$path" | cut -d ' ' -f 1
    fi
}

function verify_sha256
{
    # <path> <expected sha256> [label]
    local path="$1"
    local expected="$2"
    local label="${3:-$1}"
    local actual

    if [[ -z "$expected" ]]; then
        echo "No published checksum for ${label}; refusing an unverified download." >&2
        return 1
    fi
    actual="$(sha256_of "$path")"
    if [[ "$actual" != "$expected" ]]; then
        echo "Checksum mismatch for ${label}: expected ${expected}, got ${actual}" >&2
        return 1
    fi
}

function download_verified
{
    # <url> <destination> <sha256> [label]
    local url="$1"
    local destination="$2"
    local expected="$3"
    local label="${4:-$destination}"
    local partial="${destination}.part"

    rm -f "$partial"
    curl --fail --location --retry 3 --retry-all-errors --silent --show-error \
        -o "$partial" "$url"
    if ! verify_sha256 "$partial" "$expected" "$label"; then
        rm -f "$partial"
        return 1
    fi
    mv "$partial" "$destination"
}
