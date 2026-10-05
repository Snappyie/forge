# Multi-stage build producing one self-contained image.
#
# The output is a single executable with the API and the compiled console inside
# it: no Node runtime, no `node_modules`, and no static file tree. The runtime
# stage carries only that binary and the shared libraries it needs.
#
# Migrations are embedded via `sqlx::migrate!` and the console via `rust-embed`,
# so neither needs to be present at runtime.

FROM rust:1.94-slim AS builder

# `openssl-sys` (via sqlx's TLS backend) compiles vendored OpenSSL when no system
# copy is present; the build-essential and pkg-config packages let it link
# against the system library instead, which is both faster and reproducible.
#
# Node is needed here, and only here: the console is exported to static files and
# compiled into the binary, so the runtime stage needs neither Node nor the
# exported tree.
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        pkg-config libssl-dev ca-certificates nodejs npm && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Dependency manifests first, so a source-only change reuses the cached layer.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# `.env*` is excluded by .dockerignore, so nothing local leaks in. The empty
# value is set explicitly anyway: `NEXT_PUBLIC_*` is inlined at build time, and
# relying on the source default alone means one stray environment variable - or a
# future edit to that default - silently produces a console pointing at some
# other host.
COPY forge-web ./forge-web

# The build script runs `npm ci` and `npm run build` when `forge-web/out` is
# absent, then the Rust build embeds the result.
#
# `NEXT_PUBLIC_API_URL` is set empty, which the console reads as "same-origin":
# every request goes to `/api/v1/...` on the host serving the page, so the single
# binary needs no CORS configuration and no second origin.
ENV NEXT_PUBLIC_API_URL=

RUN cargo build --release --bin forge-server --bin forge && \
    strip target/release/forge-server target/release/forge

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