#!/bin/sh
# pitwall-managed-launcher-v1
set -eu
self=$0
while [ -L "$self" ]; do
  base=$(CDPATH= cd -- "$(dirname -- "$self")" && pwd -P)
  link=$(readlink "$self")
  case "$link" in /*) self=$link ;; *) self=$base/$link ;; esac
done
PITWALL_HOME=$(CDPATH= cd -- "$(dirname -- "$self")" && pwd -P)
export PITWALL_HOME
release=$PITWALL_HOME/current
[ -f "$release/VERSION" ] || { echo 'Pitwall installation is incomplete. Run the installer again.' >&2; exit 1; }
export PITWALL_CONFIG="${PITWALL_CONFIG:-$PITWALL_HOME/config.env}"
export SPROUT_STATE_DIR="${SPROUT_STATE_DIR:-$PITWALL_HOME/state}"
cd -P -- "$release"
. ./runtime.sh
case "${1:-}" in
  --version|-V)
    exec ./sprout --version ;;
  --help|-h|help)
    printf 'Pitwall %s\nAll your agent work, one view.\n\n' "$(cat VERSION)"
    printf '%s\n' 'Usage:' '  pitwall --demo                     Try fictional sample data' '  pitwall login                      Sign in to Vercel' '  pitwall --team SLUG [--board NAME]  Open your Board' '  pitwall worker                     Start the local agent queue' '  pitwall agents                     List installed agents' '  pitwall --version'
    printf '\nSettings: %s\nDrafts and run history: %s\n' "$PITWALL_CONFIG" "$SPROUT_STATE_DIR"
    printf '\nWorker: Python 3.11+ and a signed-in supported agent CLI; Rust is needed only for code-editing tasks.\n' ;;
  login)
    shift
    exec "$SPROUT_NODE" board-runtime/node_modules/.bin/vercel login "$@" ;;
  agents)
    exec "$SPROUT_NODE" board-runtime/agent-runner.mjs list ;;
  worker)
    shift
    [ "$#" -eq 0 ] || { echo 'Usage: pitwall worker (set SPROUT_TEAM in config.env)' >&2; exit 2; }
    exec './worker/Start Codex Queue.command' ;;
  *) exec ./sprout "$@" ;;
esac
