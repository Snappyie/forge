# Multi-stage build producing a small runtime image.
#
# The build stage compiles the whole workspace; the runtime stage carries only
# the binaries and the shared libraries they need. Migrations are embedded in
# the binary (via sqlx::migrate!), so no .sql files are needed at runtime.

FROM rust:1.92-slim AS builder

# `openssl-sys` (via sqlx's TLS backend) compiles vendored OpenSSL when no system
# copy is present; the build-essential and pkg-config packages let it link
# against the system library instead, which is both faster and reproducible.
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        pkg-config libssl-dev ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Dependency manifests first, so a source-only change reuses the cached layer.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY forge-web ./forge-web

# Build the server, worker-capable CLI, and the tooling the scripts call.
RUN cargo build --release --bin forge-server --bin forge && \
    strip target/release/forge-server target/release/forge || true

FROM debian:bookworm-slim AS runtime

# `ca-certificates` is required for outbound HTTPS (the HTTP executor), and
# `curl` backs the container healthcheck.
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

# Run unprivileged: nothing here needs root.
RUN useradd --create-home --uid 10001 forge
WORKDIR /app

COPY --from=builder /build/target/release/forge-server /usr/local/bin/forge-server
COPY --from=builder /build/target/release/forge /usr/local/bin/forge

USER forge
EXPOSE 3000

ENV FORGE_SERVER_HOST=0.0.0.0 \
    FORGE_SERVER_PORT=3000 \
    FORGE_LOG_FORMAT=json

# The liveness probe is public and touches no dependency, so a database blip
# does not cause the orchestrator to restart a merely-waiting process.
HEALTHCHECK --interval=15s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:3000/api/v1/health/live || exit 1

ENTRYPOINT ["/usr/local/bin/forge-server"]