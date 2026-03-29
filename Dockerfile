# ═══════════════════════════════════════════════════════════════
#  AnyServer — Multi-stage Docker build
# ═══════════════════════════════════════════════════════════════
#
#   docker build -t anyserver .
#   docker run -d -p 3001:3001 -p 2222:2222 -v anyserver-data:/app/data anyserver
#
# ═══════════════════════════════════════════════════════════════

# ── Stage 1: Build the frontend ──────────────────────────────
FROM node:20-slim AS frontend-build

RUN corepack enable && corepack prepare pnpm@latest --activate

WORKDIR /src/frontend

# Install dependencies first (layer cache optimisation)
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile

# Copy the rest of the frontend source and build
COPY frontend/ ./
RUN pnpm run build


# ── Stage 2: Build the backend ───────────────────────────────
FROM rust:1.94-bookworm AS backend-build

WORKDIR /src

# Copy manifests, lock file, cargo config, and SQLx offline cache for dependency caching
COPY backend/Cargo.toml backend/Cargo.lock ./backend/
COPY backend/.cargo ./backend/.cargo
COPY backend/.sqlx ./backend/.sqlx
COPY backend/migrations ./backend/migrations

# Create a dummy main.rs / lib.rs so cargo can fetch and compile dependencies
RUN mkdir -p backend/src && \
    echo 'fn main() {}' > backend/src/main.rs && \
    echo '' > backend/src/lib.rs

WORKDIR /src/backend

# Pre-build dependencies (this layer is cached until Cargo.toml/lock changes)
RUN cargo build --release --features bundle-frontend 2>/dev/null || true

# Now copy the real source code
COPY backend/src ./src
COPY backend/build.rs ./build.rs

# Copy the built frontend from stage 1 so rust-embed can bundle it
COPY --from=frontend-build /src/frontend/dist /src/frontend/dist

# Touch the source files so cargo knows they're newer than the dummy ones
RUN touch src/main.rs src/lib.rs

# Build the final release binary with bundled frontend
RUN cargo build --release --features bundle-frontend

# Verify the binary exists
RUN test -f /src/backend/target/release/anyserver


# ── Stage 3: Minimal runtime image ──────────────────────────
FROM debian:bookworm-slim AS runtime

# Install only the minimal runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        tini \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN groupadd --gid 1000 anyserver && \
    useradd --uid 1000 --gid anyserver --shell /bin/sh --create-home anyserver

# Set up the application directory and data volume
RUN mkdir -p /app/data && chown -R anyserver:anyserver /app

WORKDIR /app

# Copy the compiled binary from the build stage
COPY --from=backend-build /src/backend/target/release/anyserver /app/anyserver

# Ensure the binary is executable
RUN chmod +x /app/anyserver

# Default environment variables
ENV ANYSERVER_DATA_DIR=/app/data \
    ANYSERVER_HTTP_PORT=3001 \
    ANYSERVER_SFTP_PORT=2222 \
    RUST_LOG=anyserver=info,tower_http=info

# Expose HTTP and SFTP ports
EXPOSE 3001 2222

# Declare the data directory as a volume so it persists
VOLUME /app/data

# Switch to non-root user
USER anyserver

# Use tini as init to handle signals properly (graceful shutdown)
ENTRYPOINT ["tini", "--"]
CMD ["/app/anyserver"]
