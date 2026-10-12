# Translation Hub: the API, the soft poll and the web app in one image.
#
#   docker build -t translation-hub .
#   docker run --rm -p 8787:8787 \
#     -v /path/to/forgejo/repositories:/repos:ro \
#     -v hub-data:/data \
#     -e HUB_FORGEJO_DB='mysql://hub_ro:…@mariadb/gitea' \
#     translation-hub
#
# The repos are mounted read-only (on the wacs2 host: Forgejo's volume, `:ro,z`
# for SELinux); the hub never writes to them. It runs as uid 1000, Forgejo's,
# so it can read repos Forgejo created. Everything it
# owns is in /data (the SQLite) and /logs (rotated JSON lines). See AGENTS.md
# for every variable and docker-compose.yml for a dev-data setup.

# --- web: the static app ------------------------------------------------------
FROM node:22-bookworm-slim AS web
RUN corepack enable
WORKDIR /src
# The viewer is aliased from the spike, so both trees are needed.
COPY spikes/viewer/package.json spikes/viewer/pnpm-lock.yaml spikes/viewer/
COPY web/package.json web/pnpm-lock.yaml web/
RUN apt-get update && apt-get install -y --no-install-recommends git ca-certificates && rm -rf /var/lib/apt/lists/*
RUN cd spikes/viewer && pnpm install --frozen-lockfile --ignore-workspace
RUN cd web && pnpm install --frozen-lockfile --ignore-workspace
COPY spikes/viewer spikes/viewer
COPY web web
RUN cd web && pnpm build

# --- server: the Rust binary --------------------------------------------------
FROM rust:1-bookworm AS server
# cmake: vendored libgit2. git: scripture-kitchen is a git dependency.
RUN apt-get update && apt-get install -y --no-install-recommends cmake git && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo
COPY crates crates
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --release -p hub-server --features mariadb,otel,vendored-libgit2 \
 && cp target/release/hub-server /hub-server

# --- runtime ------------------------------------------------------------------
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 1000 --home /data hub && mkdir -p /data /logs && chown hub /data /logs
COPY --from=server /hub-server /usr/local/bin/hub-server
COPY --from=web /src/web/dist /srv/web
USER hub
ENV HUB_REPO_ROOT=/repos \
    HUB_DB=/data/hub.sqlite \
    HUB_LOG_DIR=/logs \
    HUB_WEB_DIR=/srv/web \
    HUB_LISTEN=0.0.0.0:8787
VOLUME ["/data", "/logs"]
EXPOSE 8787
HEALTHCHECK --interval=30s --timeout=5s CMD ["hub-server", "--health"]
ENTRYPOINT ["hub-server"]
