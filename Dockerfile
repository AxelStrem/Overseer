# Overseer's server and its page, in one image, serving the documents on /data.
#
#     docker build -t overseer .
#     docker run -p 4747:4747 -v overseer-documents:/data -e OVERSEER_TOKEN=<a long secret> overseer
#
# It listens on every interface, since a container is reached from outside it, and so it refuses
# to start until it is told who may read the documents: OVERSEER_TOKEN for a token, or
# OVERSEER_OPEN=1 for a home network where anyone who can reach the port may. DEPLOYING.md has
# every setting.

# --- the server ---------------------------------------------------------------------------
FROM rust:1.83-slim-bookworm AS server
WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
COPY src-tauri/Cargo.toml src-tauri/Cargo.lock ./src-tauri/
COPY src-tauri/build.rs ./src-tauri/
COPY src-tauri/src ./src-tauri/src
WORKDIR /src/src-tauri
# The `server` feature without `desktop` leaves Tauri out entirely - no webview, no GTK - which
# is what makes this build possible on a slim image and the result small.
RUN cargo build --release --features server --bin overseer-server

# --- the page -----------------------------------------------------------------------------
FROM node:22-slim AS frontend
WORKDIR /src
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY index.html vite.config.js ./
COPY src ./src
RUN npm run build

# --- what runs ----------------------------------------------------------------------------
FROM debian:bookworm-slim
# gosu for the privilege drop in the entrypoint; ca-certificates and tzdata so that a document
# reading the date reads the one TZ names, rather than UTC.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates gosu tzdata \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=server /src/src-tauri/target/release/overseer-server /usr/local/bin/overseer-server
COPY --from=frontend /src/dist /app/frontend
# The templates that ship with Overseer, which a document can inherit from wherever it is. The
# server finds them by this variable, since nothing about where its binary is says where they are.
COPY library /app/library
COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
# Stripped of carriage returns: a checkout on Windows can give the script CRLF endings, and a
# shebang ending in "\r" fails as a missing interpreter that looks like a broken image.
RUN sed -i 's/\r$//' /usr/local/bin/entrypoint.sh && chmod +x /usr/local/bin/entrypoint.sh

ENV OVERSEER_LIBRARY=/app/library \
    OVERSEER_FRONTEND=/app/frontend \
    OVERSEER_ROOT=/data \
    OVERSEER_BIND=0.0.0.0 \
    MALLOC_ARENA_MAX=2
# No OVERSEER_PORT: it would win over the PORT a host hands the container. Without either, the
# server listens on 4747.

# No VOLUME instruction: some hosts (Railway) reject it, since a volume there is attached to the
# service rather than asked for by the image. Mount one at /data - without it the documents live
# in the container and are lost on the next deploy.
RUN useradd --create-home --uid 10001 overseer && mkdir -p /data && chown overseer /data
ENV RUN_AS=overseer
EXPOSE 4747

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
