# Shared launcher settings. Source only from the Pitwall project directory.
# .pitwall.env is a trusted, local shell configuration file and is never committed.
pitwall_config=${PITWALL_CONFIG:-./.pitwall.env}
if [ -f "$pitwall_config" ]; then
  set -a
  . "$pitwall_config"
  set +a
fi
if [ -z "${SPROUT_NODE:-}" ]; then
  if [ -x ./runtimes/node/bin/node ]; then
    SPROUT_NODE="$PWD/runtimes/node/bin/node"
  else
    SPROUT_NODE=node
  fi
fi
: "${SPROUT_PYTHON:=python3}"
: "${SPROUT_CODEX:=codex}"
export SPROUT_NODE SPROUT_PYTHON SPROUT_CODEX SPROUT_BOARD

pitwall_check_worker() {
  : "${SPROUT_TEAM:?Set SPROUT_TEAM in your Pitwall config or environment.}"
  "$SPROUT_PYTHON" -c 'import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)' || {
    echo 'The worker requires Python 3.11 or newer; set SPROUT_PYTHON to its executable.' >&2
    return 1
  }
}
