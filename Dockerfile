# syntax=docker/dockerfile:1.7

###############################################################################
# Stage 1: Build the Svelte frontend
###############################################################################
FROM node:22-slim AS frontend-builder

WORKDIR /app/frontend

# Install dependencies first (cached layer)
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci

# Build the SPA
COPY frontend/ ./
RUN npm run build

###############################################################################
# Stage 2: Build the Rust backend
###############################################################################
FROM rust:1.95-slim AS backend-builder

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the entire workspace and build. The cargo registry cache mount
# avoids re-downloading crates across rebuilds.
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin opennodia-server \
    && cp /app/target/release/opennodia-server /usr/local/bin/opennodia-server

###############################################################################
# Stage 3: Minimal runtime image
###############################################################################
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        tini \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 1001 opennodia \
    && useradd --system --uid 1001 --gid opennodia \
        --home-dir /app --shell /usr/sbin/nologin opennodia

WORKDIR /app

# Copy the compiled binary
COPY --from=backend-builder /usr/local/bin/opennodia-server /usr/local/bin/opennodia-server

# Copy the built frontend (served as static files)
COPY --from=frontend-builder /app/frontend/dist /app/web

# Copy sample config as a reference
COPY opennodia.sample.toml /app/opennodia.sample.toml

# Data directory for PIN hash, sessions, future SQLite DB
RUN mkdir -p /app/data && chown -R opennodia:opennodia /app

USER opennodia

EXPOSE 30080

ENV OPENNODIA_WEB_DIR=/app/web \
    RUST_LOG=opennodia=info,tower_http=info

CMD ["opennodia-server", \
     "--config", "/app/opennodia.toml", \
     "--web-dir", "/app/web"]
