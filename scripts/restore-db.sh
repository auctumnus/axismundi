#!/usr/bin/env bash
# restore a backup into the database from config.json.
# DESTRUCTIVE: drops and recreates objects.
set -euo pipefail

# shellcheck source=_lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

require jq

if [[ $# -lt 1 ]]; then
    echo "usage: $0 <backup-file>" >&2
    exit 1
fi

backup="$(resolve_backup "$1")"

# pg_dump's custom format starts with the magic bytes "PGDMP". reject
# bogus files (truncated downloads, wrong format) before we go near the
# live db.
if [[ "$(head -c 5 "$backup" 2>/dev/null)" != "PGDMP" ]]; then
    echo "$backup is not a pg_dump custom-format file (missing PGDMP magic)" >&2
    echo "expected output of \`pg_dump -Fc\`. did you point at a plain .sql by mistake?" >&2
    exit 1
fi

url="$(db_url)"
# parse out user@host:port/db without printing the password
safe_target="$(jq -nr --arg url "$url" '
    $url | capture("//(?<u>[^:]+):[^@]*@(?<rest>.+)") | "\(.u)@\(.rest)"
')"

echo "WARNING: this will overwrite the database at $safe_target" >&2
echo "         (config: $(config_file))" >&2
echo "         (backup: $backup)" >&2
echo -n "type 'yes' to continue: " >&2
read -r confirm
if [[ "$confirm" != "yes" ]]; then
    echo "aborted" >&2
    exit 1
fi

# --clean drops objects before recreating; --if-exists avoids errors on first
# restore. run_pg_restore wraps this in a postgres:18 container on the
# axismundi network when the db host is podman-internal-only.
run_pg_restore "$url" "$backup" --clean --if-exists
echo "restored from $backup" >&2
