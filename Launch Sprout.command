#!/bin/sh
set -eu
cd -- "$(dirname -- "$0")"
. ./runtime.sh
if [ ! -x ./sprout ]; then
  echo 'Build Pitwall first: cargo build --locked --bins'
  echo 'Then copy target/debug/sprout and target/debug/reader into this directory.'
  exit 1
fi
exec ./sprout "$@"
