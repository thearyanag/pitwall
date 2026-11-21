#!/bin/sh
# Preserve the existing launcher and process name for configuration compatibility.
exec "$(dirname -- "$0")/Launch Sprout.command" "$@"
