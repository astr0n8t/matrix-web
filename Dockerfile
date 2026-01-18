# Build Stage
ARG BUILDPLATFORM
FROM --platform=${BUILDPLATFORM} rust:latest AS rust-source
FROM --platform=${BUILDPLATFORM} ghcr.io/cross-rs/x86_64-unknown-linux-gnu:edge AS build_amd64
FROM --platform=${BUILDPLATFORM} ghcr.io/cross-rs/aarch64-unknown-linux-gnu:edge AS build_arm64
FROM --platform=${BUILDPLATFORM} ghcr.io/cross-rs/armv7-unknown-linux-gnueabi:edge AS build_armv7
FROM --platform=${BUILDPLATFORM} ghcr.io/cross-rs/arm-unknown-linux-gnueabi:edge AS build_arm

ARG TARGETARCH
ARG TARGETVARIANT
FROM --platform=${BUILDPLATFORM} build_${TARGETARCH}${TARGETVARIANT} AS builder

COPY --from=rust-source /usr/local/rustup /usr/local
COPY --from=rust-source /usr/local/cargo /usr/local

RUN apt update && apt install openssl libssl-dev pkg-config -y

RUN rustup default stable

WORKDIR /app

ARG TARGETPLATFORM
RUN if [ "$TARGETPLATFORM" = "linux/amd64" ]; then rustup target add x86_64-unknown-linux-gnu; fi

RUN if [ "$TARGETPLATFORM" = "linux/arm64" ]; then rustup target add aarch64-unknown-linux-gnu; fi

RUN if [ "$TARGETPLATFORM" = "linux/arm" ]; then rustup target add arm-unknown-linux-gnueabi; fi

RUN if [ "$TARGETPLATFORM" = "linux/arm/v7" ]; then rustup target add armv7-unknown-linux-gnueabi; fi

RUN cargo install cargo-build-deps

# create a new empty project
RUN cargo init
COPY Cargo.toml ./

# cache deps compile
RUN if [ "$TARGETPLATFORM" = "linux/amd64" ]; then cargo build-deps --release --target=x86_64-unknown-linux-gnu; fi
RUN if [ "$TARGETPLATFORM" = "linux/arm64" ]; then cargo build-deps --release --target=aarch64-unknown-linux-gnu; fi
RUN if [ "$TARGETPLATFORM" = "linux/arm" ]; then cargo build-deps --release --target=arm-unknown-linux-gnueabi; fi
RUN if [ "$TARGETPLATFORM" = "linux/arm/v7" ]; then cargo build-deps --release --target=armv7-unknown-linux-gnueabi; fi

COPY ./src src
COPY ./static static

# Translate docker platforms to rust platforms
RUN if [ "$TARGETPLATFORM" = "linux/amd64" ]; then \
  cargo build --release --target x86_64-unknown-linux-gnu; \
  cp /app/target/x86_64-unknown-linux-gnu/release/matrix-web /app/matrix-web; \
  fi

RUN if [ "$TARGETPLATFORM" = "linux/arm64" ]; then \
  cargo build --release --target aarch64-unknown-linux-gnu; \
  cp /app/target/aarch64-unknown-linux-gnu/release/matrix-web /app/matrix-web; \
  fi

RUN if [ "$TARGETPLATFORM" = "linux/arm/v7" ]; then \
  cargo build --release --target armv7-unknown-linux-gnueabi; \
  cp /app/target/armv7-unknown-linux-gnueabi/release/matrix-web /app/matrix-web; \
  fi

RUN if [ "$TARGETPLATFORM" = "linux/arm" ]; then \
  cargo build --release --target arm-unknown-linux-gnueabi; \
  cp /app/target/arm-unknown-linux-gnueabi/release/matrix-web /app/matrix-web; \
  fi

# Create directory for mounting in the final stage
RUN mkdir -p /app/data

# second stage.
FROM gcr.io/distroless/cc-debian12 AS build-release-stage

ENV RUST_LOG=info

COPY --from=builder /app/matrix-web /matrix-web

# Create /data directory for database and matrix store
# Copy empty directory with proper ownership
COPY --from=builder --chown=nonroot:nonroot /app/data /data

WORKDIR /data

USER nonroot:nonroot

VOLUME ["/data"]

ENTRYPOINT ["/matrix-web"]
