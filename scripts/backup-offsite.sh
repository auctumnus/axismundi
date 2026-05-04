#!/usr/bin/env bash
# encrypt a local backup with age and upload it to backblaze b2.
# usage: ./scripts/backup-offsite.sh [backup-file]
# if no file given, uses the most recent one in backups/.
# streams encrypt+upload in one pipe, so the encrypted blob never touches local disk.
set -euo pipefail

# shellcheck source=_lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

require age rclone jq

# read backup config first so the healthcheck trap can be in place before
# anything that could fail (file checks, encryption, upload).
recipient="$(backup_config_get '.age.recipient')"
bucket="$(backup_config_get '.backblaze.bucket')"
healthcheck_url="$(backup_config_get_optional '.healthcheck_url')"

# if a healthchecks.io url is configured, ping /start now and install an EXIT
# trap that pings /fail on any nonzero exit. the success ping happens at the
# end of the script on the success path.
if [[ -n "$healthcheck_url" ]]; then
    require curl
    curl -fsS -m 10 --retry 3 "${healthcheck_url}/start" >/dev/null 2>&1 || true
    trap 'rc=$?; if [[ $rc -ne 0 ]]; then curl -fsS -m 10 --retry 3 "$healthcheck_url/fail" >/dev/null 2>&1 || true; fi' EXIT
fi

backup="$(resolve_backup "${1:-}")"

# b2 unified-model keys auth via the s3-compatible api, not the native b2 api.
# we configure rclone's s3 backend pointed at b2's s3 endpoint, but keep the remote
# named "b2" since that's what it conceptually is.
# assignment is split from export so backup_config_get failures aren't masked
# (export always returns 0, defeating set -e on a failed substitution).
RCLONE_CONFIG_B2_ACCESS_KEY_ID="$(backup_config_get '.backblaze.key_id')"
RCLONE_CONFIG_B2_SECRET_ACCESS_KEY="$(backup_config_get '.backblaze.application_key')"
RCLONE_CONFIG_B2_ENDPOINT="$(backup_config_get '.backblaze.endpoint')"
export RCLONE_CONFIG_B2_TYPE=s3
export RCLONE_CONFIG_B2_PROVIDER=Other
# the narrow b2 key isn't entitled to CreateBucket; tell rclone not to verify
# the bucket exists (it does — we just can't list-or-create it from the s3 root).
export RCLONE_CONFIG_B2_NO_CHECK_BUCKET=true
export RCLONE_CONFIG_B2_ACCESS_KEY_ID RCLONE_CONFIG_B2_SECRET_ACCESS_KEY RCLONE_CONFIG_B2_ENDPOINT

filename="$(basename "$backup").age"
remote_path="b2:$bucket/$filename"

local_size="$(du -h "$backup" | cut -f1)"
echo "encrypting and uploading: $backup ($local_size)" >&2
echo "  → $remote_path" >&2
start=$SECONDS

# pipefail (set above) ensures pg_dump/age failures aren't masked by rclone succeeding
age -r "$recipient" < "$backup" | rclone rcat "$remote_path"

duration=$((SECONDS - start))
echo "uploaded in ${duration}s" >&2

if [[ -n "$healthcheck_url" ]]; then
    curl -fsS -m 10 --retry 3 "$healthcheck_url" >/dev/null 2>&1 || \
        echo "warning: healthcheck success ping failed (upload itself succeeded)" >&2
fi

# stdout: the remote path, so this can be piped or captured
echo "$remote_path"
