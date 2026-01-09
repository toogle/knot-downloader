# syntax=docker/dockerfile:1

FROM ghcr.io/rust-cross/rust-musl-cross:x86_64-musl AS builder
WORKDIR /app

COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl \
    && musl-strip target/x86_64-unknown-linux-musl/release/knot-downloader

FROM scratch AS runtime
WORKDIR /app

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/knot-downloader /knot-downloader

ENTRYPOINT ["/knot-downloader"]
