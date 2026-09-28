# syntax=docker/dockerfile:1

# stretto on Debian 13 slim, run as a non-root user. One build, two images:
#
# - The default target: the CLI and the proxy (stretto, stretto-proxy,
#   stretto-procedure, stretto-mcp-demo), with `stretto` as the entrypoint.
# - `console`: the same image with stretto-console, the web console, as the
#   entrypoint, serving its UI and API on port 8080.
#
#   docker build -t stretto .
#   docker run --rm stretto --help
#   docker run --rm --entrypoint /usr/local/share/stretto/quickstart/run.sh stretto
#
#   docker build --target console -t stretto-console .
#   docker run --rm -p 127.0.0.1:8080:8080 -v ~/.stretto:/data/.stretto \
#     --user "$(id -u):$(id -g)" stretto-console
#
# docs/install.md and docs/console.md show how to use them, and compose.yaml
# runs the console; .github/workflows/container.yml publishes both images to
# ghcr.io (alexnodeland/stretto and alexnodeland/stretto-console) on a
# version tag.

# The Rust release that builds the binaries, pinned. The builder and the
# runtime share a Debian release, so the binaries find the glibc they were
# linked against.
ARG RUST_VERSION=1.98.1
ARG DEBIAN_RELEASE=trixie
# The Node release that builds the console's UI, pinned.
ARG NODE_VERSION=22.22.2

# The console's UI (console/, and the brand kit it imports), built once on the
# build machine's own platform: its output is the same files on every one.
FROM --platform=$BUILDPLATFORM node:${NODE_VERSION}-slim AS ui
WORKDIR /src/console
COPY console/package.json console/package-lock.json ./
# Behind a TLS-inspecting proxy, the build secret `ca` and the HTTPS_PROXY
# build argument serve npm as they serve cargo below.
RUN --mount=type=cache,id=stretto-npm,target=/root/.npm,sharing=locked \
    --mount=type=secret,id=ca \
    if [ -s /run/secrets/ca ]; then export NODE_EXTRA_CA_CERTS=/run/secrets/ca; fi \
 && npm ci --no-audit --no-fund
COPY brand/tokens.css /src/brand/tokens.css
COPY brand/fonts /src/brand/fonts
COPY brand/logo /src/brand/logo
COPY console ./
RUN npm run build

FROM rust:${RUST_VERSION}-slim-${DEBIAN_RELEASE} AS builder
WORKDIR /src
# rust-toolchain.toml stays out of the build context (.dockerignore): its
# `stable` channel would make rustup fetch another toolchain than this one.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# stretto-console embeds the UI at compile time, from console/dist.
COPY --from=ui /src/console/dist ./console/dist
# Behind a TLS-inspecting proxy, pass its CA bundle as the build secret `ca`
# (docker build --secret id=ca,src=FILE) and the proxy as the HTTPS_PROXY
# build argument. Cargo uses them for this step only: secrets and the proxy
# arguments are never written to a layer or to the image's history.
RUN --mount=type=cache,id=stretto-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=stretto-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=secret,id=ca \
    if [ -s /run/secrets/ca ]; then export CARGO_HTTP_CAINFO=/run/secrets/ca; fi \
 && cargo build --locked --release -p stretto-report -p stretto-proxy -p stretto-console \
 && mkdir /out \
 && cp target/release/stretto target/release/stretto-proxy \
       target/release/stretto-procedure target/release/stretto-mcp-demo \
       target/release/stretto-console /out/

# What both images share: the five binaries, the quickstart, a user.
FROM debian:${DEBIAN_RELEASE}-slim AS base
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
LABEL org.opencontainers.image.source="https://github.com/alexnodeland/stretto" \
      org.opencontainers.image.url="https://alexnodeland.github.io/stretto/" \
      org.opencontainers.image.licenses="MIT"
# ~/.stretto is /data/.stretto: mount a volume on /data (or your own
# ~/.stretto on /data/.stretto) to keep the logs, the answer cache and the
# flows.
ENV HOME=/data
WORKDIR /data
USER 10001:10001

# The console: `docker build --target console`.
FROM base AS console
LABEL org.opencontainers.image.title="stretto-console" \
      org.opencontainers.image.description="stretto's web console: the MCP servers stretto fronts, their recorded sessions, the flows learned from them, and jobs, over ~/.stretto." \
      org.opencontainers.image.documentation="https://github.com/alexnodeland/stretto/blob/main/docs/console.md"
# Every address in the container, so that a published port reaches it:
# publish it on 127.0.0.1 only (-p 127.0.0.1:8080:8080). The console asks
# for its token either way, and prints the URL that carries it when it
# starts.
ENV STRETTO_CONSOLE_LISTEN=0.0.0.0:8080
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["stretto-console", "healthcheck"]
ENTRYPOINT ["stretto-console"]

# The CLI, the default target: last, so that `docker build .` builds it.
FROM base AS cli
LABEL org.opencontainers.image.title="stretto" \
      org.opencontainers.image.description="Learns which reads an LLM agent makes next from its recorded tool calls, and serves them through an MCP proxy." \
      org.opencontainers.image.documentation="https://github.com/alexnodeland/stretto/blob/main/docs/install.md"
ENTRYPOINT ["stretto"]
CMD ["--help"]
