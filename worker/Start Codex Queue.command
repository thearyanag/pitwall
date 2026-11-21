#!/bin/sh
set -eu
cd -- "$(dirname -- "$0")/.."
. ./runtime.sh
: "${SPROUT_TEAM:?Set SPROUT_TEAM in .pitwall.env or your environment.}"
command -v "$SPROUT_CODEX" >/dev/null || { echo 'Install and sign in to the Codex CLI first.' >&2; exit 1; }
exec "$SPROUT_PYTHON" worker/listener.py
