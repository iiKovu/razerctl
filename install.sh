#!/usr/bin/env bash
# Build the portable bundle in place. Nothing is installed outside this folder.
set -euo pipefail

ROOT="$(cd -P "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
PACKAGES=(blacksharkd blackshark-ctl blackshark-gui)

command -v cargo >/dev/null 2>&1 || {
    echo "blackshark: cargo is required to build from source" >&2
    exit 1
}

was_running=false
if [[ "$($ROOT/blackshark status --brief 2>/dev/null || true)" == "active" ]]; then
    was_running=true
    "$ROOT/blackshark" stop >/dev/null
fi

echo "blackshark: building portable release binaries"
CARGO_TARGET_DIR="$TARGET_DIR" cargo build --release --locked \
    -p blacksharkd -p blackshark-ctl -p blackshark-gui

for binary in "${PACKAGES[@]}"; do
    install -m 0755 "$TARGET_DIR/release/$binary" "$ROOT/$binary"
done

if [[ "$was_running" == true ]]; then
    "$ROOT/blackshark" start
fi

echo "blackshark: portable bundle updated in $ROOT"
