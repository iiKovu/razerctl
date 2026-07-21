#!/usr/bin/env bash
# Update the maintained portable fork, then rebuild its in-folder binaries.
set -euo pipefail

ROOT="$(cd -P "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

command -v git >/dev/null 2>&1 || {
    echo "blackshark: git is required to update" >&2
    exit 1
}

if ! git -C "$ROOT" diff --quiet || ! git -C "$ROOT" diff --cached --quiet; then
    echo "blackshark: refusing to update with uncommitted source changes" >&2
    exit 1
fi

git -C "$ROOT" pull --ff-only
exec "$ROOT/install.sh"
