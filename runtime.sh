# Shared launcher settings. Source only from the Pitwall project directory.
# .pitwall.env is a trusted, local shell configuration file and is never committed.
if [ -f ./.pitwall.env ]; then
  set -a
  . ./.pitwall.env
  set +a
fi
: "${SPROUT_NODE:=node}"
: "${SPROUT_PYTHON:=python3}"
: "${SPROUT_CODEX:=codex}"
export SPROUT_NODE SPROUT_PYTHON SPROUT_CODEX SPROUT_BOARD
