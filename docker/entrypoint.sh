#!/bin/sh
# Start Overseer's server in the image, as an ordinary user.
set -eu

ROOT="${OVERSEER_ROOT:-/data}"

# A volume arrives owned by root on most hosts, so this starts as root, gives the folders the
# server writes to the user it runs as, and drops privileges straight away. PUID and PGID name
# that user by number, as Unraid's templates do (99:100 is its nobody:users), so the files on a
# share belong to someone the host knows; without them it is the image's own user. Started as
# that user already - with `--user`, say - it carries on as it is.
if [ "$(id -u)" = "0" ] && [ -n "${RUN_AS:-}" ]; then
    if [ -n "${PUID:-}" ]; then
        [ "$PUID" = "0" ] && { echo "PUID=0 would run the server as root; give another id" >&2; exit 1; }
        OWNER="$PUID:${PGID:-$PUID}"
    else
        OWNER="$RUN_AS"
    fi
    for folder in "$ROOT" "${OVERSEER_DISK_CACHE:-}" "$(dirname "${OVERSEER_ACCOUNTS:-/}")"; do
        case "$folder" in "" | "/" | ".") continue ;; esac
        mkdir -p "$folder"
        chown -R "$OWNER" "$folder"
    done
    exec gosu "$OWNER" "$0" "$@"
fi

mkdir -p "$ROOT"
# Everything else - the address, the port, the token, --open - comes from the environment, which
# the server reads itself; anything passed to `docker run` after the image name is added as flags.
exec overseer-server "$@"
