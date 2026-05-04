#!/usr/bin/env bash
# take a postgres backup using the database_url from config.json.
# writes a custom-format dump to backups/ and prints its path on stdout.
set -euo pipefail

# shellcheck source=_lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

require jq

backups="$(backups_dir)"
mkdir -p "$backups"

# UTC, filesystem-safe (no colons), lex-sortable
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
out="$backups/axismundi-$timestamp.dump"

url="$(db_url)"

echo "backing up database to $out" >&2
start=$SECONDS

# -Fc = custom format: compressed, parallelizable on restore, table-selectable.
# when the db host only resolves on the podman-internal network (prod with
# `local`/`oci` source and no postgres.hostPort published), run pg_dump in
# a container on that network. the postgres:18 image matches the server.
if needs_podman_for "$url"; then
    require podman
    podman run --rm --network=axismundi \
        postgres:18 pg_dump -Fc -d "$url" > "$out"
else
    require pg_dump
    pg_dump -Fc -d "$url" -f "$out"
fi

duration=$((SECONDS - start))
size=$(du -h "$out" | cut -f1)
echo "done in ${duration}s (${size})" >&2

# stdout: the path, so this can be piped or captured
echo "$out"
