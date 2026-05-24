# Build stage. trixie (debian 13) for graphviz 12.x — vizoxide v1.0.5
# uses const char* args that bookworm's graphviz 2.43.0 headers don't
# match, so the build fails with `expected *mut i8, found *const i8`.
FROM rust:1.88-slim-trixie AS builder

# Install system dependencies for building. libclang-dev is needed by
# bindgen (graphviz-sys) — without it we get "Unable to find libclang"
# during the cargo build. libltdl-dev is required for graphviz's plugin
# loader: without it, cmake silently disables `with_ltdl` and libgvc is
# built with the entire dlopen path compiled out, so every render fails
# with "Format: X not recognized. No formats found." — even `-Tdot`. We
# install graphviz from source below, so no libgraphviz-dev here.
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libclang-dev \
    clang \
    curl \
    unzip \
    ca-certificates \
    cmake \
    g++ \
    flex \
    bison \
    libexpat1-dev \
    libltdl-dev \
    && rm -rf /var/lib/apt/lists/*

# Build sqlx-cli early so it caches across source changes — it only needs
# rust + ssl/pkg-config from the apt step above. Pin to ~0.8 to match the
# sqlx crate in Cargo.toml (0.9 needs rustc >= 1.94). native-tls only (no
# rustls) to avoid the ring crate's dep tree.
RUN cargo install --locked sqlx-cli --version "^0.8" \
        --no-default-features \
        --features postgres,native-tls \
    && cp /usr/local/cargo/bin/sqlx /sqlx

# Build graphviz 12.x from source. Debian's libgraphviz-dev is stuck at
# 2.42.x, which has the pre-const-correct `agattr(... char *value)`
# signature; vizoxide v1.0.5 expects the modern `const char *value` and
# fails with "expected *mut i8, found *const i8" otherwise. Pin a
# specific tag so this is deterministic.
ARG GRAPHVIZ_VERSION=12.2.1
RUN curl -fsSL "https://gitlab.com/graphviz/graphviz/-/archive/${GRAPHVIZ_VERSION}/graphviz-${GRAPHVIZ_VERSION}.tar.gz" \
        | tar -xz \
    && cd "graphviz-${GRAPHVIZ_VERSION}" \
    # CMAKE_INSTALL_LIBDIR=lib forces a flat lib dir; GNUInstallDirs would
    # otherwise pick lib/x86_64-linux-gnu on debian, which we'd then have to
    # mirror in the runtime stage's COPY paths.
    && cmake -B build -DCMAKE_INSTALL_PREFIX=/usr/local -DCMAKE_INSTALL_LIBDIR=lib -DCMAKE_BUILD_TYPE=Release \
    && cmake --build build -j"$(nproc)" \
    && cmake --install build \
    && ldconfig \
    && cd .. && rm -rf "graphviz-${GRAPHVIZ_VERSION}"

# Install bun (the frontend uses bun, not npm — package.json has bun.lock,
# no package-lock.json, and the build script is `bun build.ts`).
RUN curl -fsSL https://bun.sh/install | bash \
    && ln -s /root/.bun/bin/bun /usr/local/bin/bun

WORKDIR /app

# Copy frontend manifests and install deps from the lockfile
COPY frontend/package.json frontend/bun.lock ./frontend/
RUN cd frontend && bun install --frozen-lockfile

COPY frontend ./frontend
RUN cd frontend && bun run build

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY migrations ./migrations
COPY templates ./templates
COPY .sqlx ./.sqlx

# Build the application
ENV SQLX_OFFLINE=true
RUN cargo build --release

# Runtime stage. trixie matches the build stage's libc abi. graphviz is
# copied from the builder (where we built it from source) — apt's
# graphviz is too old to provide a matching SONAME for the binary.
FROM debian:trixie-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libwebp7 \
    libexpat1 \
    libltdl7 \
    && rm -rf /var/lib/apt/lists/*

# Pull graphviz runtime from the builder. We need `dot` here too: `dot -c`
# enumerates the plugin .so files at their *runtime* paths and writes the
# config6 manifest libgvc reads on every load — running it in the builder
# would record builder-stage paths and leave the runtime image without a
# manifest.
COPY --from=builder /usr/local/bin/dot /usr/local/bin/dot
COPY --from=builder /usr/local/lib/ /usr/local/lib/
RUN ldconfig && /usr/local/bin/dot -c

WORKDIR /app

# Copy the binary + sqlx-cli from builder stage
COPY --from=builder /app/target/release/axismundi ./
COPY --from=builder /sqlx ./
COPY --from=builder /app/migrations ./migrations
COPY --from=builder /app/templates ./templates
COPY --from=builder /app/frontend/dist ./frontend/dist
COPY assets ./assets

EXPOSE 3000

CMD ["./axismundi"]