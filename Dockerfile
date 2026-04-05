# ─── Stage 1: build ──────────────────────────────────────────────────────────
FROM rust:1.86-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# 1. Copy workspace manifest
COPY Cargo.toml Cargo.lock ./

# 2. Copy all source directories (preserving structure)
COPY ternlang-root/      ternlang-root/
COPY ternlang-translator/ ternlang-translator/
COPY ternlang-audit/      ternlang-audit/

# 3. Build the API
RUN cargo build --release -p ternlang-api

# ─── Stage 2: runtime ────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Note: The binary is in the workspace target folder
COPY --from=builder /build/target/release/ternlang-api /usr/local/bin/ternlang-api

# Fly.io configuration
ENV PORT=8080
EXPOSE 8080

CMD ["ternlang-api"]
