#!/bin/sh
set -eu
cd -- "$(dirname -- "$0")/.."
if [ "$#" -ne 1 ]; then
  echo 'Usage: Launch Codex Worker.command TASK_ID'
  echo 'Claims one Pitwall task, runs Codex, verifies changes, then stops.'
  exit 2
fi
. ./runtime.sh
: "${SPROUT_TEAM:?Set SPROUT_TEAM in .pitwall.env or your environment.}"
exec "$SPROUT_NODE" board-runtime/node_modules/@vercel/board/dist/cli.mjs \
  --scope "$SPROUT_TEAM" --name "${SPROUT_BOARD:-General}" --as codex-worker \
  tasks run "$1" --lease 5m \
  --result 'Codex finished this request. See the conversation for the response and any change verification.' \
  -- "$SPROUT_PYTHON" worker/run_codex.py
