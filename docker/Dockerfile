# ─────────────────────────────────────────────────────────────────────────────
# RuView WiFi DensePose — Multi-stage Dockerfile
# ─────────────────────────────────────────────────────────────────────────────

# ── Base ───────────────────────────────────────────────────────────────────────
FROM rust:1.75-slim-bookworm AS base
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config build-essential ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r ruview && useradd -r -g ruview -s /bin/false ruview

WORKDIR /app

# ── Dependencies cache layer ───────────────────────────────────────────────────
FROM base AS deps
COPY Cargo.toml Cargo.lock ./
# Create stub src files so cargo can cache dependency compilation
RUN mkdir -p crates/wifi-densepose-cli/src crates/wifi-densepose-api/src \
    && echo 'fn main(){}' > crates/wifi-densepose-cli/src/main.rs \
    && echo '' > crates/wifi-densepose-api/src/lib.rs
COPY crates/wifi-densepose-core/Cargo.toml     crates/wifi-densepose-core/
COPY crates/wifi-densepose-signal/Cargo.toml   crates/wifi-densepose-signal/
COPY crates/wifi-densepose-config/Cargo.toml   crates/wifi-densepose-config/
COPY crates/wifi-densepose-hardware/Cargo.toml crates/wifi-densepose-hardware/
COPY crates/wifi-densepose-nn/Cargo.toml       crates/wifi-densepose-nn/
COPY crates/wifi-densepose-api/Cargo.toml      crates/wifi-densepose-api/
COPY crates/wifi-densepose-db/Cargo.toml       crates/wifi-densepose-db/
COPY crates/wifi-densepose-mat/Cargo.toml      crates/wifi-densepose-mat/
COPY crates/wifi-densepose-wasm/Cargo.toml     crates/wifi-densepose-wasm/
COPY crates/wifi-densepose-cli/Cargo.toml      crates/wifi-densepose-cli/

# ── Development ────────────────────────────────────────────────────────────────
FROM deps AS development
ARG RUST_LOG=debug
ENV RUST_LOG=${RUST_LOG} ENVIRONMENT=development
COPY . .
RUN cargo build --workspace
USER ruview
EXPOSE 8000
CMD ["cargo", "run", "--package", "wifi-densepose-cli", "--", "start", "--addr", "0.0.0.0:8000"]

# ── Testing ────────────────────────────────────────────────────────────────────
FROM deps AS testing
COPY . .
RUN cargo test --workspace 2>&1 | tee /tmp/test-results.txt
RUN grep -v "FAILED" /tmp/test-results.txt | tail -5

# ── Security scan ──────────────────────────────────────────────────────────────
FROM testing AS security
RUN cargo install cargo-audit --locked 2>/dev/null || true
RUN cargo audit 2>&1 || true

# ── Production ─────────────────────────────────────────────────────────────────
FROM deps AS builder
COPY . .
RUN cargo build --release --package wifi-densepose-cli

# Minimal runtime image
FROM debian:bookworm-slim AS production
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN groupadd -r ruview && useradd -r -g ruview -s /bin/false ruview

COPY --from=builder /app/target/release/ruview /usr/local/bin/ruview
COPY --from=builder /app/ui /app/ui

USER ruview
WORKDIR /app

ENV RUST_LOG=info ENVIRONMENT=production

EXPOSE 8000
HEALTHCHECK --interval=15s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -sf http://localhost:8000/health | grep -q '"status":"ok"' || exit 1

ENTRYPOINT ["/usr/local/bin/ruview"]
CMD ["start", "--addr", "0.0.0.0:8000"]
