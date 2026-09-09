# syntax=docker/dockerfile:1
FROM node:24.11.1-bookworm-slim AS ui
WORKDIR /build/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

FROM debian:bookworm-slim AS stockfish
ARG TARGETARCH
ARG STOCKFISH_COMMIT=cb3d4ee9b47d0c5aae855b12379378ea1439675c
RUN apt-get update && apt-get install -y --no-install-recommends build-essential ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build/stockfish
RUN curl --fail --location --retry 3 "https://github.com/official-stockfish/Stockfish/archive/${STOCKFISH_COMMIT}.tar.gz" \
    | tar -xz --strip-components=1
# Portable targets use the complete Stockfish 18 networks, without requiring
# AVX2 on the server. Networks are downloaded, checksum-checked and embedded
# by Stockfish's own Makefile. Limit parallel compilers to keep builds bounded.
RUN case "$TARGETARCH" in amd64) arch=x86-64 ;; arm64) arch=armv8 ;; *) exit 1 ;; esac \
    && make -C src -j2 build ARCH="$arch" \
    && cp src/stockfish /usr/local/bin/stockfish \
    && strip /usr/local/bin/stockfish \
    && make -C src objclean \
    && rm -f src/stockfish \
    && tar -czf /build/stockfish-source.tar.gz .

FROM rust:1.97.1-bookworm AS rust
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --locked --release --bin chess-review

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl libstdc++6 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 tempo \
    && useradd --uid 10001 --gid tempo --no-create-home tempo \
    && mkdir -p /data /app/ui /usr/share/stockfish \
    && chown tempo:tempo /data
COPY --from=rust /build/target/release/chess-review /usr/local/bin/chess-review
COPY --from=stockfish /usr/local/bin/stockfish /usr/local/bin/stockfish
COPY --from=stockfish /build/stockfish/Copying.txt /usr/share/stockfish/COPYING.txt
COPY --from=stockfish /build/stockfish-source.tar.gz /usr/share/stockfish/source.tar.gz
COPY --from=ui /build/web/dist/selfhost/ /app/ui/
WORKDIR /app
ENV TEMPO_BIND=0.0.0.0:8080 TEMPO_DATA_DIR=/data TEMPO_UI_DIR=/app/ui STOCKFISH_PATH=/usr/local/bin/stockfish
USER 10001:10001
EXPOSE 8080
VOLUME ["/data"]
HEALTHCHECK --interval=15s --timeout=5s --start-period=20s --retries=3 \
    CMD curl --fail --silent http://127.0.0.1:8080/api/health || exit 1
ENTRYPOINT ["chess-review", "serve"]
