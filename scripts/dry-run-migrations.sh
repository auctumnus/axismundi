#!/usr/bin/env bash
# spin up an ephemeral postgres, restore the latest backup into it,
# run pending migrations, run a couple sanity queries, and tear down.
# exits nonzero if any step fails.
set -euo pipefail

# shellcheck source=_lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

require podman pg_restore sqlx

backup="$(resolve_backup "${1:-}")"
echo "using backup: $backup" >&2

container="axismundi-dryrun-$$"
# avoid colliding with dev (5432) and test (2435)
port=37215

cleanup() {
    podman rm -f "$container" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "starting ephemeral postgres on port $port..." >&2
podman run -d --rm \
    --name "$container" \
    -e POSTGRES_PASSWORD=dryrun \
    -e POSTGRES_USER=dryrun \
    -e POSTGRES_DB=axismundi \
    -p "$port:5432" \
    postgres:18 >/dev/null

echo -n "waiting for postgres" >&2
for _ in $(seq 1 30); do
    if podman exec "$container" pg_isready -U dryrun -d axismundi >/dev/null 2>&1; then
        echo " ready" >&2
        break
    fi
    echo -n "." >&2
    sleep 1
done

ephemeral_url="postgres://dryrun:dryrun@localhost:$port/axismundi"

echo "restoring backup..." >&2
# --no-owner --no-acl: don't try to apply prod's owner/role grants to the ephemeral db
pg_restore --clean --if-exists --no-owner --no-acl -d "$ephemeral_url" "$backup"

echo "applying pending migrations..." >&2
DATABASE_URL="$ephemeral_url" sqlx migrate run --source "$(repo_root)/migrations"

echo "running sanity queries..." >&2
podman exec "$container" psql -U dryrun -d axismundi -c "
    SELECT 'users'      AS table, COUNT(*) FROM users
    UNION ALL SELECT 'languages',    COUNT(*) FROM languages
    UNION ALL SELECT 'words',        COUNT(*) FROM words;
"

echo "" >&2
echo "migrations apply cleanly to a copy of prod data" >&2
