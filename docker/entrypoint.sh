#!/bin/sh
# Start Overseer's server in the image, as an ordinary user.
set -eu

ROOT="${OVERSEER_ROOT:-/data}"

# A volume arrives owned by root on most hosts, so this starts as root, gives the volume to the
# user the server runs as, and drops privileges straight away. Started as that user already -
# with `--user`, say - it carries on as it is.
if [ "$(id -u)" = "0" ] && [ -n "${RUN_AS:-}" ]; then
    mkdir -p "$ROOT"
    chown -R "$RUN_AS" "$ROOT"
    exec gosu "$RUN_AS" "$0" "$@"
fi

mkdir -p "$ROOT"
# Everything else - the address, the port, the token, --open - comes from the environment, which
# the server reads itself; anything passed to `docker run` after the image name is added as flags.
exec overseer-server "$@"
