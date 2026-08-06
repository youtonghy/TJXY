FROM node:22.22.3-bookworm-slim AS frontend
WORKDIR /build/admin
COPY admin/package.json admin/package-lock.json ./
RUN npm ci
COPY admin/ ./
RUN npm run build

FROM rust:1.88.0-bookworm AS server
WORKDIR /build
COPY . .
RUN cargo build --release --locked -p tjxy-server --bin tjxy-server

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 tjxy \
    && useradd --uid 10001 --gid tjxy --home-dir /app --create-home tjxy \
    && install -d -o tjxy -g tjxy /config /data /app/admin
WORKDIR /app
COPY --from=server --chown=tjxy:tjxy /build/target/release/tjxy-server /usr/local/bin/tjxy-server
COPY --from=frontend --chown=tjxy:tjxy /build/admin/dist /app/admin/dist
USER tjxy
ENV TJXY_CONTAINER=true \
    TJXY_CONFIG_FILE=/config/tjxy.toml \
    TJXY_SETUP_DATA_DIR=/data \
    TJXY_SETUP_BIND=0.0.0.0:8096 \
    TJXY_ASSETS_DIR=/data/assets \
    TJXY_ADMIN_DIST_DIR=/app/admin/dist
VOLUME ["/config", "/data"]
EXPOSE 8096
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=6 \
    CMD curl --fail --silent http://127.0.0.1:8096/health/ready > /dev/null || exit 1
ENTRYPOINT ["/usr/local/bin/tjxy-server"]
