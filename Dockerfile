# syntax=docker/dockerfile:1

# stretto's four binaries (stretto, stretto-proxy, stretto-procedure,
# stretto-mcp-demo) on Debian 13 slim, run as a non-root user.
#
#   docker build -t stretto .
#   docker run --rm stretto --help
#   docker run --rm --entrypoint /usr/local/share/stretto/quickstart/run.sh stretto
#
# docs/install.md shows how to use it; .github/workflows/container.yml
# publishes it to ghcr.io/alexnodeland/stretto on a version tag.

# The Rust release that builds the binaries, pinned. The builder and the
# runtime share a Debian release, so the binaries find the glibc they were
# linked against.
ARG RUST_VERSION=1.98.1
ARG DEBIAN_RELEASE=trixie

FROM rust:${RUST_VERSION}-slim-${DEBIAN_RELEASE} AS builder
WORKDIR /src
# rust-toolchain.toml stays out of the build context (.dockerignore): its
# `stable` channel would make rustup fetch another toolchain than this one.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# Behind a TLS-inspecting proxy, pass its CA bundle as the build secret `ca`
# (docker build --secret id=ca,src=FILE) and the proxy as the HTTPS_PROXY
# build argument. Cargo uses them for this step only: secrets and the proxy
# arguments are never written to a layer or to the image's history.
RUN --mount=type=cache,id=stretto-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=stretto-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=secret,id=ca \
    if [ -s /run/secrets/ca ]; then export CARGO_HTTP_CAINFO=/run/secrets/ca; fi \
 && cargo build --locked --release -p stretto-report -p stretto-proxy \
 && mkdir /out \
 && cp target/release/stretto target/release/stretto-proxy \
       target/release/stretto-procedure target/release/stretto-mcp-demo /out/

FROM debian:${DEBIAN_RELEASE}-slim
# No CA certificates are installed: the HTTP client (reqwest with rustls)
# carries Mozilla's roots in the binaries, and the Jev client also trusts
# the certificates in SSL_CERT_FILE, for a TLS-inspecting proxy.
RUN groupadd --gid 10001 stretto \
 && useradd --uid 10001 --gid 10001 --home-dir /data --no-create-home \
            --shell /usr/sbin/nologin stretto \
 && install -d -o 10001 -g 10001 -m 0755 /data
COPY --from=builder /out/ /usr/local/bin/
COPY --chmod=0755 examples/quickstart/run.sh /usr/local/share/stretto/quickstart/run.sh
COPY LICENSE README.md /usr/local/share/doc/stretto/
LABEL org.opencontainers.image.title="stretto" \
      org.opencontainers.image.description="Learns which reads an LLM agent makes next from its recorded tool calls, and serves them through an MCP proxy." \
      org.opencontainers.image.source="https://github.com/alexnodeland/stretto" \
      org.opencontainers.image.url="https://alexnodeland.github.io/stretto/" \
      org.opencontainers.image.documentation="https://github.com/alexnodeland/stretto/blob/main/docs/install.md" \
      org.opencontainers.image.licenses="MIT"
# ~/.stretto is /data/.stretto: mount a volume on /data to keep the logs,
# the answer cache and the flows.
ENV HOME=/data
WORKDIR /data
USER 10001:10001
ENTRYPOINT ["stretto"]
CMD ["--help"]
