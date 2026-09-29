#!/usr/bin/env bash
# one-shot: copy the old minio bucket into garage, after the nix module has
# switched storage over (minio's images were deleted from docker hub on
# 2026-09-11, so storage moved to garage).
#
#   sudo ./scripts/migrate-minio-to-garage.sh <minio-data-dir> <old-keys-dir>
#
#   <minio-data-dir>  the old minio volume, e.g. /tank/axismundi/state/minio
#   <old-keys-dir>    dir holding the pre-switch s3-access-key / s3-secret-key
#                     (minio's root creds)
#
# boots the old data once in a throwaway minio container (from the image still
# in podman's store; it can't be pulled any more) on the axismundi network, then
# copies server-to-server with rclone so Content-Type survives. skips results/
# (imagor's cache, regenerated on demand). safe to re-run: rclone copy only
# transfers what's missing or changed. minio only touches its own .minio.sys
# metadata on boot; the objects themselves are left alone.
#
# new creds, bucket and region come from the rendered prod config
# (AXISMUNDI_CONFIG, default /run/axismundi-runtime/config.json). run as root:
# the axismundi containers live in root's podman store.
set -euo pipefail

# shellcheck source=_lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

if [[ $# -ne 2 ]]; then
    sed -n '2,21p' "$0" | sed 's/^# \?//' >&2
    exit 2
fi
minio_dir="$1"
old_keys_dir="$2"

require podman jq
export AXISMUNDI_CONFIG="${AXISMUNDI_CONFIG:-/run/axismundi-runtime/config.json}"

minio_image="docker.io/minio/minio:latest"
legacy_name="axismundi-minio-legacy"
rclone_image="docker.io/rclone/rclone:1.71"

[[ -d "$minio_dir" ]] || { echo "no minio data at $minio_dir" >&2; exit 1; }
for f in s3-access-key s3-secret-key; do
    [[ -s "$old_keys_dir/$f" ]] || { echo "missing $old_keys_dir/$f" >&2; exit 1; }
done
if ! podman image exists "$minio_image"; then
    echo "$minio_image isn't in podman's image store, and it can't be pulled" >&2
    echo "any more. restore the minio data from the b2 mirror instead" >&2
    echo "(docs/backups.md) and copy it in with rclone." >&2
    exit 1
fi

bucket="$(config_get '.s3.bucket')"
region="$(config_get '.s3.region')"

# creds go through env files, not argv, so they don't show up in ps
envdir="$(mktemp -d)"
cleanup() {
    podman stop --ignore --time 5 "$legacy_name" >/dev/null 2>&1 || true
    rm -rf "$envdir"
}
trap cleanup EXIT
(
    umask 0077
    {
        printf 'MINIO_ROOT_USER=%s\n' "$(cat "$old_keys_dir/s3-access-key")"
        printf 'MINIO_ROOT_PASSWORD=%s\n' "$(cat "$old_keys_dir/s3-secret-key")"
    } > "$envdir/minio.env"
    {
        printf 'RCLONE_CONFIG_OLD_TYPE=s3\n'
        printf 'RCLONE_CONFIG_OLD_PROVIDER=Minio\n'
        printf 'RCLONE_CONFIG_OLD_ENDPOINT=http://%s:9000\n' "$legacy_name"
        printf 'RCLONE_CONFIG_OLD_ACCESS_KEY_ID=%s\n' "$(cat "$old_keys_dir/s3-access-key")"
        printf 'RCLONE_CONFIG_OLD_SECRET_ACCESS_KEY=%s\n' "$(cat "$old_keys_dir/s3-secret-key")"
        printf 'RCLONE_CONFIG_NEW_TYPE=s3\n'
        printf 'RCLONE_CONFIG_NEW_PROVIDER=Other\n'
        printf 'RCLONE_CONFIG_NEW_ENDPOINT=http://axismundi-garage:3900\n'
        printf 'RCLONE_CONFIG_NEW_REGION=%s\n' "$region"
        printf 'RCLONE_CONFIG_NEW_ACCESS_KEY_ID=%s\n' "$(config_get '.s3.access_key')"
        printf 'RCLONE_CONFIG_NEW_SECRET_ACCESS_KEY=%s\n' "$(config_get '.s3.secret_key')"
    } > "$envdir/rclone.env"
)

rclone() {
    podman run --rm --network=axismundi --env-file="$envdir/rclone.env" "$rclone_image" "$@"
}

echo "==> starting $legacy_name on $minio_dir" >&2
podman run -d --rm --pull=never --name "$legacy_name" --network=axismundi \
    --env-file="$envdir/minio.env" \
    -v "$minio_dir:/data" \
    "$minio_image" server /data >/dev/null

echo "==> waiting for it to answer" >&2
for i in $(seq 1 30); do
    if rclone lsf --max-depth 1 "old:$bucket" >/dev/null 2>&1; then break; fi
    if (( i == 30 )); then
        echo "legacy minio never came up; its logs:" >&2
        podman logs "$legacy_name" >&2 || true
        exit 1
    fi
    sleep 2
done

echo "==> copying old:$bucket → new:$bucket (excluding results/)" >&2
start=$SECONDS
rclone copy --exclude "results/**" --transfers 8 --stats 30s --stats-one-line \
    "old:$bucket" "new:$bucket"
echo "    done in $((SECONDS - start))s" >&2

echo "==> verifying (every old object present in garage with a matching hash)" >&2
rclone check --one-way --exclude "results/**" "old:$bucket" "new:$bucket"
echo "==> migration complete. $minio_dir still has every object; delete it once you're happy." >&2
