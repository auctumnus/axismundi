# Start only the database
db:
    @echo "Starting PostgreSQL database..."
    docker compose -f docker-compose.db.yml up -d
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
    docker compose -f docker-compose.db.test.yml -f docker-compose.minio.test.yml down -v --timeout 0 2>/dev/null >/dev/null

test flags="" cov="" $RUST_BACKTRACE="1":
    #!/usr/bin/env sh
    set -uo pipefail
    echo "Bringing up test services..."
    docker compose -f docker-compose.db.test.yml -f docker-compose.minio.test.yml up -d 2>/dev/null >/dev/null
    if [ $? -ne 0 ]; then \
        echo "Failed to start test services"; \
        exit 1; \
    fi
    echo "Waiting for database to be ready..."
    while ! docker exec axismundi-db-test pg_isready -U user_test -d axismundi_test >/dev/null 2>&1; do \
        echo "Database is unavailable - sleeping"; \
        sleep .5; \
    done
    echo "Database is ready!"
    echo "Creating database..."
    sqlx database create --database-url {{postgres_test_url}}
    if [ $? -ne 0 ]; then \
        echo "Failed to create database"; \
        just test_teardown; \
        exit 1; \
    fi
    echo "Running migrations..."
    sqlx migrate run --database-url {{postgres_test_url}}
    if [ $? -ne 0 ]; then \
        echo "Failed to run migrations"; \
        just test_teardown; \
        exit 1; \
    fi
    echo "Running tests..."

    if [ -z "{{cov}}" ]; then
        DATABASE_URL={{postgres_test_url}} cargo test {{flags}}
        return_code=$?
    else
        DATABASE_URL={{postgres_test_url}} cargo llvm-cov --ignore-filename-regex "nix/store" {{flags}}
        return_code=$?
    fi
    just test_teardown
    exit $return_code

cov flags="":
    just test "{{flags}}" cov="1"

test-json:
    just test covflags="--json --output-path cov.json"

test-lcov:
    just test covflags="--lcov --output-path lcov.info"

db-migrate:
    sqlx migrate run

# Stop the database
db-stop:
    docker compose -f docker-compose.db.yml down

# Start both database and application
up:
    docker compose up -d

# Stop both services
down:
    docker compose down

# Run the application locally (requires database to be running)
run:
    cp .env.docker .env
    cargo run

# Watch frontend for changes during development
watch-frontend:
    cd frontend && bun run dev

# Full development setup (database + backend + frontend watching)
dev-full:
    just db &
    sleep 3
    cd frontend && bun run dev &
    just run

# Build the application
build:
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
    docker compose -f docker-compose.db.yml logs -f

# Clean up all containers and volumes
clean:
    docker compose down -v
    docker compose -f docker-compose.db.yml down -v
    docker system prune -f

# Reset database (stop, remove volume, start fresh)
db-reset:
    docker compose -f docker-compose.db.yml down -v
    just db

# Start Minio S3 storage
minio:
    @echo "Starting Minio S3 storage..."
    docker compose -f docker-compose.minio.yml up -d
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
    docker compose -f docker-compose.minio.yml down

# View Minio logs
minio-logs:
    docker compose -f docker-compose.minio.yml logs -f
