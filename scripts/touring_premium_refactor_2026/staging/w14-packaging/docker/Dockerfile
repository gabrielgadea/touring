# Touring multi-stage Docker build
FROM rust:1.83-slim-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release --frozen

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/touring /usr/local/bin/touring
ENTRYPOINT ["touring"]
LABEL org.opencontainers.image.source="https://github.com/touring/touring"
LABEL org.opencontainers.image.licenses="MIT OR Apache-2.0"
