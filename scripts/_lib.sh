#!/usr/bin/env bash
# shared helpers for axismundi scripts.
# source this file: source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

# absolute path to the scripts/ dir, for callers that need to invoke siblings
# (e.g. deploy.sh calling backup-db.sh)
# shellcheck disable=SC2034  # used by sourcing scripts
LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

repo_root() {
    git rev-parse --show-toplevel 2>/dev/null
}

config_file() {
    echo "${AXISMUNDI_CONFIG:-$(repo_root)/config.json}"
}

backup_config_file() {
    echo "${AXISMUNDI_BACKUP_CONFIG:-$(repo_root)/backup.json}"
}

backups_dir() {
    echo "${AXISMUNDI_BACKUPS_DIR:-$(repo_root)/backups}"
}

# read a field out of a json config file.
#   $1 — file path
#   $2 — env var name (shown in the "not found" hint)
#   $3 — jq expression
#   $4 — strict (optional, default 0): if 1, also reject REPLACE_ME and
#        produce a per-field error message instead of jq's default
_json_get() {
    local file="$1" env_var="$2" path="$3" strict="${4:-0}" val
    if [[ ! -f "$file" ]]; then
        echo "config not found at $file" >&2
        echo "set $env_var to point at a different file" >&2
        return 1
    fi
    if (( strict )); then
        if ! val="$(jq -er "$path" "$file")"; then
            echo "config missing field: $path (in $file)" >&2
            return 1
        fi
        if [[ "$val" == "REPLACE_ME" ]]; then
            echo "config field $path is set to REPLACE_ME — fill it in (in $file)" >&2
            return 1
        fi
        echo "$val"
    else
        jq -er "$path" "$file"
    fi
}

config_get()        { _json_get "$(config_file)"        AXISMUNDI_CONFIG        "$1"; }
backup_config_get() { _json_get "$(backup_config_file)" AXISMUNDI_BACKUP_CONFIG "$1" 1; }

# like backup_config_get, but returns empty string for missing/null fields
# instead of erroring. for fields where "not set" is a valid state.
backup_config_get_optional() {
    local cfg val
    cfg="$(backup_config_file)"
    if [[ ! -f "$cfg" ]]; then
        echo ""
        return 0
    fi
    val="$(jq -r "$1 // \"\"" "$cfg" 2>/dev/null || echo "")"
    if [[ "$val" == "REPLACE_ME" ]]; then
        val=""
    fi
    echo "$val"
}

# resolves the database url, in order:
#   1. DATABASE_URL env var (any source — e.g. user-supplied, systemd unit)
#   2. AXISMUNDI_CONFIG (or $(repo_root)/config.json), read directly if readable
#   3. same path read via sudo if it exists but is unreadable as us
#   4. /run/axismundi-runtime/config.json via sudo (the nix module's render
#      path — catches `just deploy` from a freshly-cloned repo on a prod
#      box). must match `runtimeDir` in nix/lib.nix.
db_url() {
    if [[ -n "${DATABASE_URL:-}" ]]; then
        echo "$DATABASE_URL"
        return 0
    fi

    local cfg path
    cfg="$(config_file)"

    if [[ -r "$cfg" ]]; then
        jq -er '.database_url' "$cfg"
        return 0
    fi

    for path in "$cfg" /run/axismundi-runtime/config.json; do
        if [[ -e "$path" ]]; then
            echo "==> reading database url from $path via sudo" >&2
            sudo -p "[sudo] password to read $path: " jq -er '.database_url' "$path"
            return 0
        fi
    done

    echo "no database url found:" >&2
    echo "  - set DATABASE_URL in env, or" >&2
    echo "  - put a readable config.json in $(repo_root), or" >&2
    echo "  - point AXISMUNDI_CONFIG at one" >&2
    return 1
}

require() {
    local missing=()
    for cmd in "$@"; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            missing+=("$cmd")
        fi
    done
    if (( ${#missing[@]} > 0 )); then
        echo "missing required commands: ${missing[*]}" >&2
        echo "if you're not in the nix shell, run: nix develop" >&2
        return 1
    fi
}

# extract the host portion from scheme://[user[:pass]@]host[:port][/path]
url_host() {
    local s="${1#*://}"
    s="${s#*@}"
    s="${s%%/*}"
    s="${s%%:*}"
    echo "$s"
}

# decide whether a url should be reached via a podman container on the
# axismundi network. true when the url's host doesn't resolve from the host
# system — i.e. it's `axismundi-postgres` / `axismundi-minio` / similar
# podman-internal dns. dev hosts (localhost / external dns) resolve and
# bypass this; prod hosts where the supporting services aren't published
# to a host port end up routed through podman.
needs_podman_for() {
    local host
    host="$(url_host "$1")"
    if getent hosts "$host" >/dev/null 2>&1; then
        return 1
    fi
    return 0
}

# run `sqlx migrate run` against $1 (db_url), with migrations from $2 (host
# path). when the db host doesn't resolve from the host (prod with `local`/
# `oci` source and no postgres.hostPort), shells into the axismundi:local
# image on the axismundi network — the image bakes in sqlx-cli for exactly
# this. errors out with a clear message if axismundi:local doesn't exist.
run_sqlx_migrate() {
    local url="$1" migrations="$2"
    if needs_podman_for "$url"; then
        require podman
        if ! podman image exists axismundi:local 2>/dev/null; then
            echo "axismundi:local image not found." >&2
            echo "the db host '$(url_host "$url")' only resolves on the" >&2
            echo "podman network, so sqlx must run inside a container." >&2
            echo "run \`podman build -t axismundi:local .\` first, or" >&2
            echo "set postgres.hostPort to publish the db on 127.0.0.1." >&2
            return 1
        fi
        podman run --rm --network=axismundi \
            -v "$migrations:/migrations:ro,Z" \
            axismundi:local \
            /app/sqlx migrate run --source /migrations --database-url "$url"
    else
        require sqlx
        DATABASE_URL="$url" sqlx migrate run --source "$migrations"
    fi
}

# run `pg_restore` of $2 (backup path) into $1 (db_url). same podman fallback
# rationale as run_sqlx_migrate / backup-db.sh — the prod db host is
# podman-internal-only by default.
run_pg_restore() {
    local url="$1" backup="$2"
    shift 2
    if needs_podman_for "$url"; then
        require podman
        podman run --rm --network=axismundi \
            -v "$backup:/tmp/backup.dump:ro,Z" \
            postgres:18 \
            pg_restore "$@" -d "$url" /tmp/backup.dump
    else
        require pg_restore
        pg_restore "$@" -d "$url" "$backup"
    fi
}

# resolve a backup path: $1 if non-empty, else the newest *.dump in backups_dir.
# also enforces that the resolved path exists and is non-empty.
resolve_backup() {
    local backup="${1:-}" backups
    backups="$(backups_dir)"
    if [[ -z "$backup" ]]; then
        backup=$(ls -1t "$backups"/*.dump 2>/dev/null | head -n1 || true)
        if [[ -z "$backup" ]]; then
            echo "no backups found in $backups" >&2
            echo "run scripts/backup-db.sh first" >&2
            return 1
        fi
    fi
    if [[ ! -f "$backup" ]]; then
        echo "backup file not found: $backup" >&2
        return 1
    fi
    if [[ ! -s "$backup" ]]; then
        echo "backup file is empty: $backup" >&2
        echo "refusing to use a zero-byte backup" >&2
        return 1
    fi
    echo "$backup"
}
