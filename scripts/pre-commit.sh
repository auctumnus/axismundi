#!/usr/bin/env bash
# regenerates .sqlx/ query metadata when rust files are part of the commit
# and stages any resulting changes so they go in the same commit.

set -euo pipefail

# only run when rust or cargo files are staged -- otherwise skip
if ! git diff --cached --name-only --diff-filter=ACMR \
    | grep -qE '\.(rs|toml)$|^Cargo\.lock$'; then
    exit 0
fi

# load DATABASE_URL from .env if direnv hasn't populated it (e.g. when git
# is invoked from an editor without the project shell loaded)
if [[ -z "${DATABASE_URL:-}" ]] && [[ -f .env ]]; then
    set -a
    source ./.env
    set +a
fi

if [[ -z "${DATABASE_URL:-}" ]]; then
    echo "pre-commit: no DATABASE_URL in env or .env -- skipping sqlx prepare" >&2
    exit 0
fi

echo "pre-commit: cargo sqlx prepare..."
if ! cargo sqlx prepare >/dev/null; then
    echo "pre-commit: cargo sqlx prepare failed -- is postgres running? (just db)" >&2
    exit 1
fi

# stage anything that changed (modified, deleted, or newly created) in .sqlx/
changed=0
if ! git diff --quiet --no-ext-diff -- .sqlx/; then
    changed=1
fi
if git ls-files --others --exclude-standard -- .sqlx/ | grep -q .; then
    changed=1
fi

if (( changed )); then
    git add .sqlx/
    echo "pre-commit: staged updated .sqlx/ metadata"
fi
