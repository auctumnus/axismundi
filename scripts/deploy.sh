#!/usr/bin/env bash
# deploy axismundi to a `source = "local"` nixos host:
#   1. take a fresh backup of prod (skippable)
#   2. dry-run pending migrations against that backup
#   3. build the podman image as axismundi:local
#   4. stop podman-axismundi.service (skipped with --no-restart)
#   5. apply migrations to prod (the binary doesn't auto-migrate)
#   6. start podman-axismundi.service (skipped with --no-restart)
#
# run from a clone of the repo on the server, as the user that owns the
# podman image store. step 5 needs sudo.
set -euo pipefail

# shellcheck source=_lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

skip_backup=0
skip_restart=0
for arg in "$@"; do
    case "$arg" in
        --no-backup)  skip_backup=1 ;;
        --no-restart) skip_restart=1 ;;
        -h|--help)
            sed -n '2,10p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *)
            echo "unknown flag: $arg" >&2
            echo "usage: $0 [--no-backup] [--no-restart]" >&2
            exit 2
            ;;
    esac
done

require podman sqlx

backup=""
if (( skip_backup )); then
    echo "==> skipping fresh backup (--no-backup)" >&2
else
    echo "==> taking fresh backup" >&2
    backup="$("$LIB_DIR/backup-db.sh")"
fi

echo "==> dry-running migrations" >&2
"$LIB_DIR/dry-run-migrations.sh" "$backup"

# build before touching prod's schema: if the build fails we don't want
# the running (old) binary stuck against a migrated db it wasn't built
# for. dry-run already proved the migrations apply cleanly.
echo "==> building axismundi:local" >&2
podman build -t axismundi:local "$(repo_root)"

if (( skip_restart )); then
    # leave the running binary alone; user accepts the old-binary-on-new-schema
    # window in exchange for not bouncing the service. no-op when there are
    # no pending migrations.
    echo "==> applying migrations to prod (--no-restart: leaving service running)" >&2
    run_sqlx_migrate "$(db_url)" "$(repo_root)/migrations"
    echo "done. restart manually with: sudo systemctl restart podman-axismundi.service" >&2
    exit 0
fi

# stop the old binary before migrating so it never serves traffic against
# a schema it wasn't built for. caddy serves nix/fallback.html during this
# window. no-op migrate when there are no pending migrations.
echo "==> stopping podman-axismundi.service" >&2
sudo systemctl stop podman-axismundi.service

echo "==> applying migrations to prod" >&2
run_sqlx_migrate "$(db_url)" "$(repo_root)/migrations"

echo "==> starting podman-axismundi.service" >&2
sudo systemctl start podman-axismundi.service
echo "==> recent logs:" >&2
sudo journalctl -u podman-axismundi.service -n 30 --no-pager
