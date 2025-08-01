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
