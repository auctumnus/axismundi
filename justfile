default:
  just --list

dev:
  just dev-full
  @echo "Starting the app..."
  concurrently --names 後,前 --prefix-colors green,blue "just dev-backend" "just dev-frontend"

dev-backend:
  CARGO_TERM_COLOR=always watchexec -w templates -w src -r cargo run

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

test flags="" cov="" $RUST_BACKTRACE="0":
    #!/usr/bin/env sh
    echo "Bringing up test services..."
    docker compose -f docker-compose.test.yml up -d 2>/dev/null >/dev/null
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
    echo "Waiting for Thumbor to be ready..."
    while ! curl -sf http://localhost:7888/healthcheck >/dev/null 2>&1; do \
        echo "Thumbor is unavailable - sleeping"; \
        sleep .5; \
    done
    echo "Thumbor is ready!"
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
run:
    cp .env.docker .env
    cargo run

# Watch frontend for changes during development
watch-frontend:
    cd frontend && bun run dev

# Start all services except the app (db, minio, thumbor, thumbor proxy)
dev-full:
    @echo "Starting all development services..."
    docker compose up -d postgres minio createbuckets thumbor
    @echo "Waiting for services to be ready..."
    @until docker exec axismundi-db pg_isready -U user -d axismundi >/dev/null 2>&1; do \
        echo "Database is unavailable - sleeping"; \
        sleep 1; \
    done
    @until curl -f http://localhost:9000/minio/health/live >/dev/null 2>&1; do \
        echo "Minio is unavailable - sleeping"; \
        sleep 1; \
    done
    @until curl -f http://localhost:8888/healthcheck >/dev/null 2>&1; do \
        echo "Thumbor is unavailable - sleeping"; \
        sleep 1; \
    done
    @echo "All services ready!"
    @echo "PostgreSQL: postgres://user:password@localhost:5432/axismundi"
    @echo "Minio Web UI: http://localhost:9001 (minioadmin/minioadmin123)"
    @echo "Minio S3 API: http://localhost:9000"
    @echo "Thumbor: http://localhost:8888"
    @echo ""
    @echo "Now you can run: just run"

dev-down:
    @echo "Stopping all development services..."
    docker compose down

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
    docker compose up minio createbuckets thumbor -d
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
    docker compose down minio createbuckets thumbor

export postgres_url := "postgres://user:password@localhost:5432/axismundi"

# Seed the database with test data (scale: 0.25 = small, 1.0 = default, 5.0 = large)
seed scale="1.0":
    DATABASE_URL={{postgres_url}} SEED_SCALE={{scale}} cargo run --bin seed

# Seed with fresh db (clears first)
seed-fresh scale="1.0":
    DATABASE_URL={{postgres_url}} SEED_SCALE={{scale}} SEED_CLEAR=1 cargo run --bin seed
