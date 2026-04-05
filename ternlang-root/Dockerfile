# ─── Stage 1: build ──────────────────────────────────────────────────────────
FROM rust:1.86-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace manifest + all member Cargo.tomls first (layer cache)
COPY Cargo.toml Cargo.lock ./
COPY ternlang-core/Cargo.toml      ternlang-core/Cargo.toml
COPY ternlang-codegen/Cargo.toml   ternlang-codegen/Cargo.toml
COPY ternlang-ml/Cargo.toml        ternlang-ml/Cargo.toml
COPY ternlang-cli/Cargo.toml       ternlang-cli/Cargo.toml
COPY ternlang-api/Cargo.toml       ternlang-api/Cargo.toml
COPY ternlang-mcp/Cargo.toml       ternlang-mcp/Cargo.toml
COPY ternlang-moe/Cargo.toml       ternlang-moe/Cargo.toml
COPY ternlang-test/Cargo.toml      ternlang-test/Cargo.toml
COPY ternlang-runtime/Cargo.toml   ternlang-runtime/Cargo.toml
COPY ternlang-hdl/Cargo.toml       ternlang-hdl/Cargo.toml
COPY ternlang-lsp/Cargo.toml       ternlang-lsp/Cargo.toml
COPY ternpkg/Cargo.toml            ternpkg/Cargo.toml
COPY ternlang-compat/Cargo.toml    ternlang-compat/Cargo.toml

# Stub out all src dirs so cargo can resolve deps without full source
RUN for crate in ternlang-core ternlang-codegen ternlang-ml ternlang-cli \
      ternlang-api ternlang-mcp ternlang-moe ternlang-test ternlang-runtime \
      ternlang-hdl ternlang-lsp ternpkg ternlang-compat; do \
      mkdir -p $crate/src && echo "fn main() {}" > $crate/src/main.rs && \
      echo "" > $crate/src/lib.rs; \
    done

# Pre-fetch dependencies (cached layer)
RUN cargo fetch

# Now copy the real source
COPY stdlib/                stdlib/
COPY ternlang-core/src      ternlang-core/src
COPY ternlang-core/stdlib   ternlang-core/stdlib
COPY ternlang-codegen/src   ternlang-codegen/src
COPY ternlang-ml/src        ternlang-ml/src
COPY ternlang-cli/src       ternlang-cli/src
COPY ternlang-api/src       ternlang-api/src
COPY ternlang-web/index.html   ternlang-web/index.html
COPY ternlang-web/pricing.html ternlang-web/pricing.html
COPY ternlang-mcp/src       ternlang-mcp/src
COPY ternlang-moe/src       ternlang-moe/src
COPY ternlang-test/src      ternlang-test/src
COPY ternlang-runtime/src   ternlang-runtime/src
COPY ternlang-hdl/src       ternlang-hdl/src
COPY ternlang-lsp/src       ternlang-lsp/src
COPY ternpkg/src            ternpkg/src
COPY ternlang-compat/src    ternlang-compat/src

RUN cargo build --release -p ternlang-api

# ─── Stage 2: runtime ────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/ternlang-api /usr/local/bin/ternlang-api

# Fly.io injects PORT; our app reads it (default 3731)
ENV PORT=8080

EXPOSE 8080

CMD ["ternlang-api"]
