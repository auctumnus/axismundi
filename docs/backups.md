# Backups

We keep two tiers of backups, local and offsite. We back up both the postgres
data and the minio data, although the latter is less important. All offsite
backups are encrypted using `age` and a private key you provide.

## Backup configuration

You will need a Backblaze B2 account, as well as a location on your drive to
put local backups. The key you use for the B2 bucket should be narrowed to:
- `listBuckets`, `readBuckets`
  - This is so `rclone` can identify the bucket.
- `listFiles`, `readFiles`
  - This is used for hash verification.
- `writeFiles`
  - This is needed to upload.

Avoid letting the key having the `deleteFiles`, `deleteBuckets`, or
`writeBuckets` privileges; this would mean that breaching your server also
gives attackers the ability to delete all of your backups.

## Scripts

The following scripts are related to backups:

| script | purpose |
| --- | --- |
| `backup-db.sh` | take a `pg_dump -Fc` to `backups/`, print path on stdout |
| `backup-offsite.sh` | encrypt latest local backup with age, stream to b2 via rclone |
| `backup-minio.sh` | mirror the minio bucket to b2 under a `minio/` prefix (additive, no encryption) |
| `restore-db.sh` | pg_restore a local backup into the db (DESTRUCTIVE, prompts) |

just targets:

```
just backup            # local pg dump only
just backup-offsite    # encrypt latest local + push to b2
just backup-minio      # mirror minio bucket → b2 (manual, dev machine)
just backup-all        # fresh dump pushed offsite + minio mirror to b2
just restore <file>    # destructive, prompts
```

## Monitoring

To monitor backup health, you can provide a `healthcheck_url` to send a payload
to any time that a backup succeeds. On the canonical deployment, this is used
with `healthchecks.io`.

## Systemd timers

Both timers are defined in `nix/module.nix`. Enable via the module options:

```nix
services.axismundi = {
    backup.enable = true;
    backup.schedule = "daily";  # systemd OnCalendar
    backup.offsite = {
        enable = true;
        configFile = "/etc/axismundi/backup.json";
        schedule = "daily";
    };
};
```

The offsite backup occurs after the local backup. (The offsite backup will pick
up on the local backup's output.)

## Restoring

If you run an important instance, it's a good idea to drill this once a month.

### From a local backup

The `just restore` script handles this for you. It will prompt before doing
anything destructive.

```bash
just restore backups/axismundi-<timestamp>.dump
```

### From an offsite backup

```bash
# 0. set up tools
nix-shell -p age rclone postgresql_18 jq

# 1. configure rclone to talk to b2
export RCLONE_CONFIG_B2_TYPE=s3
export RCLONE_CONFIG_B2_PROVIDER=Other
export RCLONE_CONFIG_B2_NO_CHECK_BUCKET=true
export RCLONE_CONFIG_B2_ACCESS_KEY_ID="<from password manager>"
export RCLONE_CONFIG_B2_SECRET_ACCESS_KEY="<from password manager>"
export RCLONE_CONFIG_B2_ENDPOINT="https://s3.us-east-005.backblazeb2.com"

# 2. find the most recent backup
rclone ls b2:axismundi | sort -k2 | tail -5

# 3. download it
rclone copy b2:axismundi/axismundi-<timestamp>.dump.age /tmp/

# 4. write the age private key to a file
cat > /tmp/age-key.txt <<EOF
# created: ...
# public key: age1...
AGE-SECRET-KEY-1...
EOF
chmod 600 /tmp/age-key.txt

# 5. decrypt
age -d -i /tmp/age-key.txt \
    < /tmp/axismundi-<timestamp>.dump.age \
    > /tmp/restore.dump

# 6. confirm it's a valid pg_dump before doing anything destructive
pg_restore --list /tmp/restore.dump | head

# 7. restore into the new db (after creating it fresh)
createdb -U postgres axismundi
pg_restore --clean --if-exists -d "postgres://.../axismundi" /tmp/restore.dump

# 8. sanity-check
psql -c "SELECT count(*) FROM users;"
psql -c "SELECT max(created_at) FROM activity;"

# 9. shred the temp files
shred -u /tmp/age-key.txt /tmp/restore.dump /tmp/*.dump.age
```

## minio (user images)

`backup-minio.sh` mirrors the minio bucket to b2 under a `minio/` prefix using
`rclone copy` (additive — deletes on minio do **not** propagate, and pruning
is the b2 lifecycle rules' job).

how it reaches minio:
- the script reads `s3.endpoint` from `config.json` and tries to resolve
  the host. if it resolves on the host system (dev: `localhost`; or any
  external dns), `rclone` runs directly. if it doesn't (prod with `local`
  or `oci` source: `axismundi-minio` is podman-internal dns), the script
  spawns `rclone/rclone:latest` in a container on the `axismundi` podman
  network so the dns resolves. same trick `backup-db.sh` uses for postgres.
  no `minio.hostPort` plumbing required either way.

caveats:
- not on a systemd timer. invoke manually via `just backup-minio` or as
  part of `just backup-all`.

