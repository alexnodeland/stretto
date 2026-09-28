# The stretto dev container: Rust (rust-toolchain.toml pins stable, with
# rustfmt and clippy), Python 3 for scripts/ and pilot/, and shellcheck for
# the shell scripts CI lints. Node 22 and Claude Code come from the features
# in devcontainer.json.
FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      python3 python3-venv shellcheck jq \
 && rm -rf /var/lib/apt/lists/*

# Claude Code's settings and sign-in live in a volume mounted here; creating
# the directory first gives the volume to the container's user.
RUN install -d -o vscode -g vscode /home/vscode/.claude
