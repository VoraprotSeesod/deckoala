# ---------- Stage 1: build the SvelteKit SPA ----------
FROM node:22.17.0-bookworm-slim AS frontend-builder
WORKDIR /build
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY frontend/ ./
RUN npm run build

# ---------- Stage 2: build the Rust server ----------
FROM rust:1.88.0-bookworm AS backend-builder
WORKDIR /build
COPY backend/ ./
RUN cargo build --release --locked

# ---------- Stage 2b: verify gates (not part of the runtime image) ----------
# Run with: docker build --target backend-verify --progress=plain .
FROM backend-builder AS backend-verify
RUN rustup component add rustfmt clippy \
 && cargo fmt --check \
 && cargo clippy --release --all-targets --locked -- -D warnings \
 && cargo test --release --locked

# ---------- Stage 3: runtime ----------
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends chromium ca-certificates \
 && rm -rf /var/lib/apt/lists/*
# Create /data and chown it BEFORE switching user, so the named volume is
# initialized with ownership the non-root process can write to.
RUN useradd --system --uid 10001 --create-home deckoala \
 && mkdir -p /data \
 && chown deckoala:deckoala /data
ENV CHROME_BIN=/usr/bin/chromium \
    DECKOALA_DATA_DIR=/data \
    DECKOALA_BIND=0.0.0.0:8080 \
    DECKOALA_STATIC_DIR=/app/static
COPY --from=backend-builder /build/target/release/deckoala-server /app/deckoala-server
COPY --from=frontend-builder /build/build /app/static
USER deckoala
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/app/deckoala-server", "healthcheck"]
ENTRYPOINT ["/app/deckoala-server"]
