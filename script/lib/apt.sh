#!/bin/sh
#
# Retries for apt installs that cross a flaky mirror or HTTP proxy.
#
# A failing `apt-get install` keeps every package it already fetched in
# `/var/cache/apt/archives`, so a retry in the same container layer or shell
# session only re-downloads the package that failed. That matters here because a
# proxy in front of the Ubuntu archive periodically answers a single request with
# `502 Bad Gateway`, which aborts a 300 MB install near the end.
#
# `Acquire::Retries` is not enough on its own: apt retries transport errors such
# as connection resets, but a proxy's 502 is delivered as a normal HTTP response
# and fails the file outright.

APT_ATTEMPTS="${APT_ATTEMPTS:-5}"

apt_retry() {
  attempt=1
  while :; do
    if "$@"; then
      return 0
    fi
    if [ "$attempt" -ge "$APT_ATTEMPTS" ]; then
      echo "apt_retry: giving up after $attempt attempts: $*" >&2
      return 1
    fi
    attempt=$((attempt + 1))
    echo "apt_retry: attempt $((attempt - 1))/$APT_ATTEMPTS failed, retrying: $*" >&2
    sleep $((attempt * 5))
  done
}
