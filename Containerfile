# Nocturne as a server (experimental): the real app behind HTTP for a browser, with the private
# engine inside. See docs/adr/0012-server-mode.md and docs/guide/install.md.
#
#   podman build --format docker --jobs 1 -f Containerfile -t localhost/nocturne:dev .
#
# `--format docker` keeps the HEALTHCHECK, which the OCI format has no field for.

# The interface, built for the server (ui/dist-server).
FROM docker.io/library/node:22.23.1-trixie-slim@sha256:e6d9a389d34ff9678438af985c9913fbd1eb6ed36e80fea56644f4b4f6dd70ba AS ui
WORKDIR /src/ui
COPY ui/package.json ui/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci --no-audit --no-fund
COPY ui/ ./
RUN npx vite build --mode server

# The server. The app crate links the desktop toolkit even when it runs headless, so its
# development headers are needed here and its libraries in the final stage.
FROM docker.io/library/rust:1.100.0-slim-trixie@sha256:e9d95b74296f7126a83ae3d041aeaba1ff4c478f13abc5cc034fcf7c131c3d4a AS server
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev libdbus-1-dev \
        libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev \
    && rm -rf /var/lib/apt/lists/*
# Set it to limit the build's parallel jobs on a shared machine.
ARG CARGO_BUILD_JOBS
WORKDIR /src
COPY Cargo.toml Cargo.lock helpers.lock ./
COPY crates/ crates/
COPY src-tauri/ src-tauri/
# The guide ships inside the app (help with Nocturne).
COPY docs/guide/ docs/guide/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release -p nocturne-app --features server --bin nocturne-server \
    && install -D -m 0755 target/release/nocturne-server /out/nocturne-server

# The private engine, pinned and checked against lemonade.lock by the repository's own script.
FROM docker.io/library/ubuntu:resolute-20261105.1@sha256:66b8084a0d45bd4877992bd38e04b51bfa99bd57293364cb6391fb674771846e AS engine
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY lemonade.lock ./
COPY scripts/fetch-lemonade.sh scripts/sha256.sh scripts/
RUN scripts/fetch-lemonade.sh \
    && mkdir -p /out \
    && cp -a vendor/lemonade/linux-*/. /out/ \
    && rm -f /out/.sha256

# Ubuntu 26.04, because the engine's Linux build needs its C library (glibc 2.43).
FROM docker.io/library/ubuntu:resolute-20261105.1@sha256:66b8084a0d45bd4877992bd38e04b51bfa99bd57293364cb6391fb674771846e
# Only what is loaded at run time: the toolkit the server links (libgtk-3-0t64, libdbus-1-3), the
# engine's libraries (libstdc++6, libssl3t64, zlib1g, libzstd1, libdrm2, libdrm-amdgpu1), the
# model servers' (libgomp1, libvulkan1), ffmpeg for browser recordings, and the roots for the
# consented downloads. EXTRA_PACKAGES adds more, such as mesa-vulkan-drivers for a graphics card.
ARG EXTRA_PACKAGES=""
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg \
        libgtk-3-0t64 libdbus-1-3 \
        libstdc++6 libssl3t64 zlib1g libzstd1 libdrm2 libdrm-amdgpu1 \
        libgomp1 libvulkan1 ${EXTRA_PACKAGES} \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 nocturne \
    && useradd --system --uid 10001 --gid 10001 --home-dir /data --no-create-home \
        --shell /usr/sbin/nologin nocturne \
    && install -d -o nocturne -g nocturne -m 0700 /data
COPY --from=engine /out/ /opt/nocturne/lemonade/
COPY --from=ui /src/ui/dist-server/ /usr/share/nocturne/ui/
COPY --from=server /out/nocturne-server /usr/bin/nocturne-server
ENV NOCTURNE_DATA_DIR=/data \
    NOCTURNE_BIND=0.0.0.0:55066 \
    NOCTURNE_UI=/usr/share/nocturne/ui \
    NOCTURNE_LEMOND=/opt/nocturne/lemonade
USER nocturne
VOLUME /data
EXPOSE 55066
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD ["nocturne-server", "health"]
ENTRYPOINT ["nocturne-server"]
