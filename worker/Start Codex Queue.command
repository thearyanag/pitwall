#!/bin/sh
set -eu
cd -- "$(dirname -- "$0")/.."
. ./runtime.sh
pitwall_check_worker
exec "$SPROUT_PYTHON" worker/listener.py
