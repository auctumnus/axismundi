default:
  just --list

# Symlink scripts/pre-commit.sh into .git/hooks/ (run once per clone)
install-hooks:
    ln -sf ../../scripts/pre-commit.sh .git/hooks/pre-commit
    @echo "pre-commit hook installed -> scripts/pre-commit.sh"

# fail fast if the toolchain cargo will actually use isn't nix's. checking PATH
# alone is not enough: RUSTC / RUSTC_WRAPPER / RUSTUP_TOOLCHAIN override it, and
# a rustup-proxy cargo picks its own rustc regardless of PATH. building with the
# system toolchain links against nix's libgvc without an rpath, so the binary
# builds fine and then dies with "libgvc.so.6: cannot open shared object file".
# run `just doctor` when this trips and you don't believe it.
_nix-check:
    #!/usr/bin/env sh
    bad=""
    for tool in cc rustc cargo; do
        p=$(command -v "$tool" 2>/dev/null)
        case "$p" in /nix/store/*) ;; *) bad="$bad $tool" ;; esac
    done
    [ -z "${RUSTC:-}" ] || case "$RUSTC" in /nix/store/*) ;; *) bad="$bad \$RUSTC" ;; esac
    [ -z "${RUSTC_WRAPPER:-}" ] && [ -z "${RUSTUP_TOOLCHAIN:-}" ] || bad="$bad rustup-override"
    cargo_v=$(cargo --version 2>/dev/null | cut -d" " -f2)
    rustc_v=$(rustc --version 2>/dev/null | cut -d" " -f2)
    [ "$cargo_v" = "$rustc_v" ] || bad="$bad version-skew"
    [ -z "$bad" ] && exit 0
    echo "!! wrong build toolchain:$bad"
    just doctor
    echo
    echo "   fix: 'direnv reload', or 'nix develop --command just <recipe>'"
    echo "   why: the system toolchain links nix's libgvc without an rpath, and the"
    echo "        binary then dies with 'libgvc.so.6: cannot open shared object file'"
    exit 1

# dump everything that decides which toolchain a build actually uses
doctor:
    #!/usr/bin/env sh
    echo "   IN_NIX_SHELL:     ${IN_NIX_SHELL:-<unset>}"
    echo "   cc:               $(command -v cc || echo '<none>')"
    echo "   rustc:            $(command -v rustc || echo '<none>')  ($(rustc --version 2>/dev/null))"
    echo "   cargo:            $(command -v cargo || echo '<none>')  ($(cargo --version 2>/dev/null))"
    echo "   RUSTC:            ${RUSTC:-<unset>}"
    echo "   RUSTC_WRAPPER:    ${RUSTC_WRAPPER:-<unset>}"
    echo "   RUSTUP_TOOLCHAIN: ${RUSTUP_TOOLCHAIN:-<unset>}"
    echo "   CARGO:            ${CARGO:-<unset>}"
    echo "   CARGO_HOME:       ${CARGO_HOME:-<unset>}"
    echo "   PATH (first 4):"
    echo "$PATH" | tr ":" "\n" | head -4 | sed "s/^/     /"

# target/debug/axismundi is a hardlink to whichever target/debug/deps/axismundi-*
# cargo uplifted last, and a build with nothing to do does NOT re-uplift. so one
# cargo run from outside the devshell (an editor, an agent, a stray shell) leaves
# the system-linked binary sitting there and every later `cargo run` in here
# happily executes it -- "Finished in 0.27s" and then libgvc.so.6 not found.
# deleting it makes the next build re-uplift the right one; it costs nothing when
# the link is already good.
_unstale:
    #!/usr/bin/env sh
    bin=target/debug/axismundi
    [ -e "$bin" ] || exit 0
    ldd "$bin" 2>/dev/null | grep -q "not found" || exit 0
    echo "-- $bin can't resolve its libraries (built outside the devshell); dropping it"
    ldd "$bin" 2>/dev/null | grep "not found" | sed "s/^/     /"
    rm -f "$bin"

dev: _nix-check _unstale
  just dev-full
  @echo "Starting the app..."
  concurrently --names 後,前 --prefix-colors green,blue "just dev-backend" "just dev-frontend"

# -n (--shell=none) is load-bearing: without it watchexec runs the command via
# $SHELL, i.e. `fish -c cargo run`, and a nested fish re-sources config.fish ->
# mise re-activates -> its rust (rustup 1.96) is prepended ahead of the
# devshell's, so the rebuild silently uses the wrong toolchain and links nix's
# libgvc with no rpath. sh is fine; fish is not. keep the -n.
dev-backend: _nix-check _unstale
  CARGO_TERM_COLOR=always watchexec -n -w templates -w src -r -- cargo run

dev-frontend:
  cd frontend && bun run dev

make-test-user:
    @echo "Creating test user ..."
    curl -X POST http://localhost:3000/api/users -H "Content-Type: application/json" -d '{"email":"aaa@aaa.com","password":"kitty paw fuzzy socks","username":"autumn"}'
    docker exec axismundi-db psql -U user -d axismundi -c "UPDATE users SET verified_at = NOW() WHERE email = 'aaa@aaa.com'"

    sleep 3

    @echo "Creating second test user..."
    curl -X POST http://localhost:3000/api/users -H "Content-Type: application/json" -d '{"email":"bbb@bbb.com","password":"kitty paw fuzzy socks","username":"winter"}'
    docker exec axismundi-db psql -U user -d axismundi -c "UPDATE users SET verified_at = NOW() WHERE email = 'bbb@bbb.com'"

    sleep 3

    @echo "Creating admin user..."
    curl -X POST http://localhost:3000/api/users -H "Content-Type: application/json" -d '{"email":"admin@admin.com","password":"kitty paw fuzzy socks","username":"admin"}'
    docker exec axismundi-db psql -U user -d axismundi -c "UPDATE users SET verified_at = NOW() WHERE email = 'admin@admin.com'"

    just make-admin admin@admin.com

# Make a user an admin by email or username
make-admin identifier:
    #!/usr/bin/env sh
    echo "Looking up user: {{identifier}}..."
    user_id=$(docker exec axismundi-db psql -U user -d axismundi -t -c "SELECT id FROM users WHERE email = '{{identifier}}' OR username = '{{identifier}}' LIMIT 1" | xargs)

    if [ -z "$user_id" ]; then
        echo "Error: User not found with identifier '{{identifier}}'"
        exit 1
    fi

    echo "Found user: $user_id"

    # Check if user already has admin tag
    has_admin=$(docker exec axismundi-db psql -U user -d axismundi -t -c "SELECT EXISTS(SELECT 1 FROM user_tags WHERE user_id = '$user_id' AND tag = 'admin')" | xargs)

    if [ "$has_admin" = "t" ]; then
        echo "User already has admin tag!"
        exit 0
    fi

    echo "Adding admin tag..."
    docker exec axismundi-db psql -U user -d axismundi -c "INSERT INTO user_tags (user_id, tag, hidden) VALUES ('$user_id', 'admin', false)"

    echo "Successfully made user an admin!"
    echo "User tags:"
    docker exec axismundi-db psql -U user -d axismundi -c "SELECT tag, hidden, created_at FROM user_tags WHERE user_id = '$user_id'"

# Start only the database
db:
    @echo "Starting PostgreSQL database..."
    docker compose up postgres -d
    @echo "Waiting for database to be ready..."
    @until docker exec axismundi-db pg_isready -U user -d axismundi >/dev/null 2>&1; do \
        echo "Database is unavailable - sleeping"; \
        sleep 1; \
    done
    @echo "Database is ready!"
    @echo "Connection: postgres://user:password@localhost:5432/axismundi"
    @echo "To run the app locally: just run"

export postgres_test_url := "postgres://user_test:password@localhost:2435/axismundi_test"

test_teardown:
    @echo "Tearing down test services..."
    docker compose -f docker-compose.test.yml down -v --timeout 0 2>/dev/null >/dev/null

test flags="" cov="" $RUST_BACKTRACE="0": _nix-check
    #!/usr/bin/env sh
    echo "Bringing up test services..."
    docker compose -f docker-compose.test.yml up -d 2>/dev/null >/dev/null
    if [ $? -ne 0 ]; then \
        echo "Failed to start test services"; \
        exit 1; \
    fi
    echo -n "Waiting for database to be ready..."
    while ! docker exec axismundi-db-test pg_isready -U user_test -d axismundi_test >/dev/null 2>&1; do \
        echo -n {{ if flags == "-q" { "" } else { "." } }}; \
        sleep .5; \
    done
    echo " Database is ready!"
    echo -n "Waiting for Thumbor to be ready..."
    while ! curl -sf http://localhost:7888/healthcheck >/dev/null 2>&1; do \
        echo -n {{ if flags == "-q" { "" } else { "." } }}; \
        sleep .5; \
    done
    echo " Thumbor is ready!"
    echo -n "Waiting for Lexurgy to be ready..."
    while ! curl -sf http://localhost:4000/ >/dev/null 2>&1; do \
        echo -n {{ if flags == "-q" { "" } else { "." } }}; \
        sleep .5; \
    done
    echo " Lexurgy is ready!"
    echo "Creating database..."
    sqlx database create --database-url {{postgres_test_url}}
    if [ $? -ne 0 ]; then \
        echo "Failed to create database"; \
        just test_teardown; \
        exit 1; \
    fi
    echo "Running migrations..."
    sqlx migrate run --database-url {{postgres_test_url}} >/dev/null 2>&1
    if [ $? -ne 0 ]; then \
        echo "Failed to run migrations"; \
        just test_teardown; \
        exit 1; \
    fi
    echo "Running tests..."

    if [ -z "{{cov}}" ]; then
        DATABASE_URL={{postgres_test_url}} {{ if flags == "-q" { "RUSTFLAGS='-Awarnings' cargo test" } else { "cargo test" } }} {{flags}}
        return_code=$?
    else
        DATABASE_URL={{postgres_test_url}} cargo llvm-cov --ignore-filename-regex "nix/store" {{flags}}
        return_code=$?
    fi
    just test_teardown
    exit $return_code

[working-directory: 'frontend']
test-frontend:
    bun test

[working-directory: 'frontend']
test-frontend-coverage:
    bun test --coverage

cov flags="":
    just test "{{flags}}" cov="1"

test-json:
    just test "--json --output-path cov.json" cov="1"

test-lcov:
    just test "--lcov --output-path lcov.info" cov="1"

db-migrate:
    sqlx migrate run

# Stop the database
db-stop:
    docker compose down postgres

# Start both database and application
up:
    docker compose up -d

# Stop both services
down:
    docker compose down

# Run the application locally (requires database to be running)
run: _nix-check _unstale
    cargo run

# Watch frontend for changes during development
watch-frontend:
    cd frontend && bun run dev

# Start all services except the app (db, minio, imagor, imagor proxy)
dev-full:
    @echo "Starting all development services..."
    docker compose up -d postgres minio createbuckets imagor lexurgy
    @echo "Waiting for services to be ready..."
    @until docker exec axismundi-db pg_isready -U user -d axismundi >/dev/null 2>&1; do \
        echo "Database is unavailable - sleeping"; \
        sleep 1; \
    done
    @until curl -f http://localhost:9000/minio/health/live >/dev/null 2>&1; do \
        echo "Minio is unavailable - sleeping"; \
        sleep 1; \
    done
    @until curl -f http://localhost:8888 >/dev/null 2>&1; do \
        echo "Imagor is unavailable - sleeping"; \
        sleep 1; \
    done
    @echo "All services ready!"
    @echo "PostgreSQL: postgres://user:password@localhost:5432/axismundi"
    @echo "Minio Web UI: http://localhost:9001 (minioadmin/minioadmin123)"
    @echo "Minio S3 API: http://localhost:9000"
    @echo "Imagor: http://localhost:8888"
    @echo ""
    @echo "Now you can run: just run"

dev-down:
    @echo "Stopping all development services..."
    docker compose down

# Build the application
build: _nix-check _unstale
    cargo build

# Build frontend assets
build-frontend:
    cd frontend && bun run build

# Build everything (backend + frontend)
build-all:
    just build-frontend
    just build

# Build the Docker image
docker-build:
    docker build -t axismundi .

# View logs for the full stack
logs:
    docker compose logs -f

# View database logs only
db-logs:
    docker compose logs -f postgres

# Clean up all containers and volumes
clean:
    docker compose down -v
    docker compose down postgres -v
    docker system prune -f

# Reset database (stop, remove volume, start fresh)
db-reset:
    docker compose down postgres -v
    just db
    sqlx database create
    just db-migrate

# Start Minio S3 storage
minio:
    @echo "Starting Minio S3 storage..."
    docker compose up minio createbuckets imagor -d
    @echo "Waiting for Minio to be ready..."
    @until curl -f http://localhost:9000/minio/health/live >/dev/null 2>&1; do \
        echo "Minio is unavailable - sleeping"; \
        sleep 1; \
    done
    @echo "Minio is ready!"
    @echo "Web UI: http://localhost:9001 (minioadmin/minioadmin123)"
    @echo "S3 API: http://localhost:9000"

# Stop Minio
minio-stop:
    docker compose down minio createbuckets imagor

export postgres_url := "postgres://user:password@localhost:5432/axismundi"

# Seed the database with test data (scale: 0.25 = small, 1.0 = default, 5.0 = large)
seed scale="1.0":
    DATABASE_URL={{postgres_url}} SEED_SCALE={{scale}} cargo run --bin seed

# Seed with fresh db (clears first)
seed-fresh scale="1.0":
    DATABASE_URL={{postgres_url}} SEED_SCALE={{scale}} SEED_CLEAR=1 cargo run --bin seed

# Take a postgres backup (writes to backups/, prints path)
backup:
    ./scripts/backup-db.sh

# Restore a backup into the database from config.json (DESTRUCTIVE, prompts)
restore backup:
    ./scripts/restore-db.sh {{backup}}

# Dry-run pending migrations against an ephemeral copy of the latest backup
dry-run-migrations backup="":
    ./scripts/dry-run-migrations.sh {{backup}}

# Encrypt a local backup with age and upload to backblaze b2 (defaults to latest)
backup-offsite backup="":
    ./scripts/backup-offsite.sh {{backup}}

# Mirror the minio bucket to b2 (additive copy, excludes imagor cache)
backup-minio:
    ./scripts/backup-minio.sh

# Fresh db backup pushed offsite + minio mirrored to b2
backup-all:
    ./scripts/backup-offsite.sh "$(./scripts/backup-db.sh)"
    ./scripts/backup-minio.sh

# Deploy: backup -> dry-run migrations -> apply migrations -> build axismundi:local -> restart service
deploy *args="":
    ./scripts/deploy.sh {{args}}
