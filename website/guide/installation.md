---
description: Install stretto with the install script, a release archive, Docker or from source.
---

# Installation

stretto is four programs from two Rust packages. Each release carries prebuilt binaries for Linux, macOS and Windows, install scripts that check each archive's checksum, and a Docker image; you can also build it from source ([every way to install](../../docs/install.md)).

| Package | Installs |
|---|---|
| `stretto-report` | `stretto`: learn, review, audit, promote and redact flows and sessions, and measure agents |
| `stretto-proxy` | `stretto-proxy`, the MCP proxy; `stretto-procedure`, which runs a compiled procedure; `stretto-mcp-demo`, a tiny server for trying the proxy |

<Tabs :tabs="[
  { key: 'script', label: 'Install script' },
  { key: 'binaries', label: 'Release binaries' },
  { key: 'docker', label: 'Docker' },
  { key: 'source', label: 'From source' }
]">
<template #script>

The script picks the archive for your system, checks its checksum against the release's `SHA256SUMS`, and copies the binaries into `~/.local/bin`, changing nothing else:

```sh
curl -fsSL https://github.com/alexnodeland/stretto/releases/latest/download/install.sh | sh
```

On Windows, in PowerShell:

```powershell
irm https://github.com/alexnodeland/stretto/releases/latest/download/install.ps1 | iex
```

</template>
<template #binaries>

Each release on [GitHub Releases](https://github.com/alexnodeland/stretto/releases) carries an archive of the four binaries for Linux (x86_64 and aarch64, glibc 2.35 or later), macOS (Apple silicon and Intel) and Windows x64, with `SHA256SUMS`:

```sh
sha256sum --ignore-missing --check SHA256SUMS
tar -xzf stretto-x86_64-unknown-linux-gnu.tar.gz
cp stretto-x86_64-unknown-linux-gnu/stretto* ~/.local/bin/
```

</template>
<template #docker>

`ghcr.io/alexnodeland/stretto` is published for linux/amd64 and linux/arm64 with each release. It runs as a non-root user, with `/data` as its home: mount a volume there to keep `~/.stretto`.

```sh
docker run --rm ghcr.io/alexnodeland/stretto --help
docker run --rm --entrypoint /usr/local/share/stretto/quickstart/run.sh ghcr.io/alexnodeland/stretto
```

</template>
<template #source>

You need Rust 1.88 or later ([rustup](https://rustup.rs)) and git. Install both packages from a release tag:

```sh
cargo install --locked --git https://github.com/alexnodeland/stretto --tag v0.1.0 \
  stretto-proxy stretto-report
```

Or from a checkout, to follow the `main` branch:

```sh
git clone https://github.com/alexnodeland/stretto
cd stretto
cargo install --locked --path crates/stretto-proxy
cargo install --locked --path crates/stretto-report
```

Cargo puts the programs in `~/.cargo/bin`.

</template>
</Tabs>

## Check the install

```sh
stretto doctor
```

`stretto doctor` checks the programs on your `PATH`, whether `~/.stretto` is writable, whether a TypeSafe key is set (never its value), and the flows and sessions you have. It exits with 1 when something needs fixing. `stretto completions bash|zsh|fish|powershell|elvish` prints shell completions.

MCP hosts start servers without your shell, and some do not see your `PATH`. If a host cannot start `stretto-proxy`, give its absolute path in the host's configuration: `which stretto-proxy` prints it.

::: warning Not `cargo install stretto`
The crate named `stretto` on crates.io is an unrelated cache library, and nothing of this project is on crates.io yet: it depends on a [fugue](https://github.com/alexnodeland/fugue) feature (`program`) that is not in a fugue release yet. Install with `--git` as above.
:::

## Other tools you may want

| Tool | For |
|---|---|
| Node 18 or later | [The walkthrough](./walkthrough)'s example server, the official MCP filesystem server, which runs with `npx` |
| Python 3 | The scripts in `scripts/` and `pilot/`: the walkthrough as one command, and reproducing the research |
| A τ²-bench checkout | [Reproducing the results](/research/reproduce) on τ²-bench's published trajectories |
| A TypeSafe API key | Only for the arbiter decider and the confirmation judge, which ask a System-One model. See [environment variables](/reference/environment) |

## Update and uninstall

Run the install command again to update; `cargo install` replaces the programs. To remove them:

```sh
cargo uninstall stretto-proxy stretto-report
```
