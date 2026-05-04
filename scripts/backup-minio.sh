#!/usr/bin/env bash
# mirror the minio bucket to b2 under a "minio/" prefix.
# uses rclone copy (additive only) — accidental deletes on minio do NOT propagate to b2.
# server-side b2 lifecycle rules are responsible for eventual pruning.
# excludes results/ since that's the regenerable imagor cache.
#
# no encryption: contents are user-uploaded images already served publicly by axismundi.
# revisit if private user content is added later.
set -euo pipefail

# shellcheck source=_lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

require jq

# source: local minio (creds in config.json).
# assignment is split from export so config_get failures aren't masked
# (export always returns 0, defeating set -e on a failed substitution).
RCLONE_CONFIG_MINIO_ACCESS_KEY_ID="$(config_get '.s3.access_key')"
RCLONE_CONFIG_MINIO_SECRET_ACCESS_KEY="$(config_get '.s3.secret_key')"
RCLONE_CONFIG_MINIO_ENDPOINT="$(config_get '.s3.endpoint')"
src_bucket="$(config_get '.s3.bucket')"
export RCLONE_CONFIG_MINIO_TYPE=s3
export RCLONE_CONFIG_MINIO_PROVIDER=Minio
export RCLONE_CONFIG_MINIO_ACCESS_KEY_ID RCLONE_CONFIG_MINIO_SECRET_ACCESS_KEY RCLONE_CONFIG_MINIO_ENDPOINT

# dest: b2 (creds in backup.json) — same s3-via-other config as backup-offsite.sh
RCLONE_CONFIG_B2_ACCESS_KEY_ID="$(backup_config_get '.backblaze.key_id')"
RCLONE_CONFIG_B2_SECRET_ACCESS_KEY="$(backup_config_get '.backblaze.application_key')"
RCLONE_CONFIG_B2_ENDPOINT="$(backup_config_get '.backblaze.endpoint')"
dest_bucket="$(backup_config_get '.backblaze.bucket')"
export RCLONE_CONFIG_B2_TYPE=s3
export RCLONE_CONFIG_B2_PROVIDER=Other
export RCLONE_CONFIG_B2_NO_CHECK_BUCKET=true
export RCLONE_CONFIG_B2_ACCESS_KEY_ID RCLONE_CONFIG_B2_SECRET_ACCESS_KEY RCLONE_CONFIG_B2_ENDPOINT

src="minio:$src_bucket"
dst="b2:$dest_bucket/minio/"

echo "copying $src → $dst (excluding results/)" >&2
start=$SECONDS

# when minio only resolves on the podman-internal network (prod with
# `local`/`oci` source and no minio.hostPort published), run rclone in a
# container on that network. the b2 endpoint is reached over the network's
# default egress. forwarding env vars by name (--env FOO, no value) reuses
# the exports above so we don't have to repeat them.
if needs_podman_for "$RCLONE_CONFIG_MINIO_ENDPOINT"; then
    require podman
    podman run --rm --network=axismundi \
        --env RCLONE_CONFIG_MINIO_TYPE \
        --env RCLONE_CONFIG_MINIO_PROVIDER \
        --env RCLONE_CONFIG_MINIO_ACCESS_KEY_ID \
        --env RCLONE_CONFIG_MINIO_SECRET_ACCESS_KEY \
        --env RCLONE_CONFIG_MINIO_ENDPOINT \
        --env RCLONE_CONFIG_B2_TYPE \
        --env RCLONE_CONFIG_B2_PROVIDER \
        --env RCLONE_CONFIG_B2_NO_CHECK_BUCKET \
        --env RCLONE_CONFIG_B2_ACCESS_KEY_ID \
        --env RCLONE_CONFIG_B2_SECRET_ACCESS_KEY \
        --env RCLONE_CONFIG_B2_ENDPOINT \
        rclone/rclone:latest \
        copy --exclude "results/**" --transfers 4 --stats 0 "$src" "$dst"
else
    require rclone
    rclone copy --exclude "results/**" --transfers 4 --stats 0 "$src" "$dst"
fi

duration=$((SECONDS - start))
echo "done in ${duration}s" >&2
echo "$dst"
