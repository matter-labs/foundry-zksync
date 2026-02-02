# syntax=docker/dockerfile:1

FROM rust:1-bookworm AS chef
WORKDIR /app

RUN apt update && apt install -y build-essential libssl-dev git pkg-config curl perl
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | sh
RUN cargo binstall cargo-chef sccache

RUN apk add clang lld curl build-base linux-headers git pkgconfig openssl-dev perl \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs > rustup.sh \
    && chmod +x ./rustup.sh \
    && ./rustup.sh -y

RUN [[ "$TARGETARCH" = "arm64" ]] && echo "export CFLAGS=-mno-outline-atomics" >> $HOME/.profile || true

WORKDIR /opt/foundry
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Build the project.
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

RUN --mount=type=cache,target=/root/.cargo/registry --mount=type=cache,target=/root/.cargo/git --mount=type=cache,target=/opt/foundry/target \
    source $HOME/.profile && cargo build --release --features anvil/js-tracer,cast/aws-kms,cast/gcp-kms,forge/aws-kms,forge/gcp-kms \
    && mkdir out \
    && mv target/local/forge out/forge \
    && mv target/local/cast out/cast \
    && mv target/local/anvil out/anvil \
    && mv target/local/chisel out/chisel \
    && strip out/forge \
    && strip out/cast \
    && strip out/chisel \
    && strip out/anvil;

ENV CARGO_INCREMENTAL=0 \
    RUSTC_WRAPPER=sccache \
    SCCACHE_DIR=/sccache

RUN apk add --no-cache linux-headers git clang openssl gcompat libstdc++

ARG TAG_NAME="dev"
ENV TAG_NAME=$TAG_NAME
ARG VERGEN_GIT_SHA="ffffffffffffffffffffffffffffffffffffffff"
ENV VERGEN_GIT_SHA=$VERGEN_GIT_SHA

# Build the project.
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=shared \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=shared \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=shared \
    cargo build --profile ${RUST_PROFILE} --no-default-features --features "${RUST_FEATURES}"

# `dev` profile outputs to the `target/debug` directory.
RUN ln -s /app/target/debug /app/target/dev \
    && mkdir -p /app/output \
    && mv \
    /app/target/${RUST_PROFILE}/forge \
    /app/target/${RUST_PROFILE}/cast \
    /app/target/${RUST_PROFILE}/anvil \
    /app/target/${RUST_PROFILE}/chisel \
    /app/output/

RUN sccache --show-stats || true

FROM ubuntu:22.04 AS runtime

# Install runtime dependencies.
RUN apt update && apt install -y git

COPY --from=builder /app/output/* /usr/local/bin/

RUN groupadd -g 1000 foundry && \
    useradd -m -u 1000 -g foundry foundry
USER foundry

ENTRYPOINT ["/bin/sh", "-c"]

LABEL org.label-schema.build-date=$BUILD_DATE \
    org.label-schema.name="Foundry" \
    org.label-schema.description="Foundry" \
    org.label-schema.url="https://getfoundry.sh" \
    org.label-schema.vcs-ref=$VCS_REF \
    org.label-schema.vcs-url="https://github.com/foundry-rs/foundry.git" \
    org.label-schema.vendor="Foundry-rs" \
    org.label-schema.version=$VERSION \
    org.label-schema.schema-version="1.0"
