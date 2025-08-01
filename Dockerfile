# syntax=docker/dockerfile:1.4

FROM alpine:3.21 AS build-environment

ARG TARGETARCH
WORKDIR /opt

RUN apk add clang lld curl build-base linux-headers git pkgconfig openssl-dev perl \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs > rustup.sh \
    && chmod +x ./rustup.sh \
    && ./rustup.sh -y

RUN [[ "$TARGETARCH" = "arm64" ]] && echo "export CFLAGS=-mno-outline-atomics" >> $HOME/.profile || true

WORKDIR /opt/foundry
COPY . .

# see <https://github.com/foundry-rs/foundry/issues/7925>
RUN git update-index --force-write-index

RUN --mount=type=cache,target=/root/.cargo/registry --mount=type=cache,target=/root/.cargo/git --mount=type=cache,target=/opt/foundry/target \
    source $HOME/.profile && cargo build --release --features cast/aws-kms,cast/gcp-kms,forge/aws-kms,forge/gcp-kms \
    && mkdir out \
    && mv target/local/forge out/forge \
    && mv target/local/cast out/cast \
    && mv target/local/anvil out/anvil \
    && mv target/local/chisel out/chisel \
    && strip out/forge \
    && strip out/cast \
    && strip out/chisel \
    && strip out/anvil;

FROM alpine:3.21 AS foundry-client

RUN apk add --no-cache linux-headers git clang openssl gcompat libstdc++

COPY --from=build-environment /opt/foundry/out/forge /usr/local/bin/forge
COPY --from=build-environment /opt/foundry/out/cast /usr/local/bin/cast
COPY --from=build-environment /opt/foundry/out/anvil /usr/local/bin/anvil
COPY --from=build-environment /opt/foundry/out/chisel /usr/local/bin/chisel

RUN adduser -Du 1000 foundry

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
