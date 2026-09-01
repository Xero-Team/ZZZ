#!/usr/bin/env sh
set -eu

# ZZZ does not ship hosted binaries. This script must not download a
# remote release tarball.

cat <<'EOF'
ZZZ has no hosted binaries.

Build from this repository:

    cargo run

See the local build guides:

    docs/src/development/macos.md
    docs/src/development/linux.md
    docs/src/development/windows.md
    docs/src/installation.md
EOF

exit 1
