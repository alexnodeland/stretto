---
description: Install stretto from source today; release binaries, an install script and a Docker image come with the first release.
---

# Installation

stretto is four programs from two Rust packages. Today you build them from source. Prebuilt binaries, an install script and a Docker image are coming with the first release; this page will say when they are available.

| Package | Installs |
|---|---|
| `stretto-report` | `stretto`: learn, review, audit, promote and redact flows and sessions, and measure agents |
| `stretto-proxy` | `stretto-proxy`, the MCP proxy; `stretto-procedure`, which runs a compiled procedure; `stretto-mcp-demo`, a tiny server for trying the proxy |

<!--
  FIRST RELEASE: the three tabs marked "First release" are placeholders for what the
  release branch ships (release binaries, an install script, and the image
  ghcr.io/alexnodeland/stretto). When they are published, replace each tab's body
  with its commands, drop its badge, and make it the first tab if it should be the
  default. This is the only page that mentions them.
-->
<Tabs :tabs="[
  { key: 'source', label: 'From source' },
  { key: 'binaries', label: 'Release binaries', badge: 'First release' },
  { key: 'script', label: 'Install script', badge: 'First release' },
  { key: 'docker', label: 'Docker', badge: 'First release' }
]">
<template #source>

You need Rust 1.87 or later ([rustup](https://rustup.rs)) and git. Install both packages from the `main` branch:

```sh
cargo install --git https://github.com/alexnodeland/stretto \
  stretto-proxy stretto-report
```

Or from a checkout:

```sh
git clone https://github.com/alexnodeland/stretto
cd stretto
cargo install --path crates/stretto-proxy
cargo install --path crates/stretto-report
```

Cargo puts the programs in `~/.cargo/bin`. To pin the first release, add `--tag v0.1.0` to the first command. That tag predates some options these pages document, such as `--flow-decider reach`, `stretto-procedure`, `--flow-tools` and `stretto learn --constants` ([changelog](/community/changelog)).

</template>
<template #binaries>

::: info Coming with the first release
Prebuilt binaries will be attached to each release on [GitHub Releases](https://github.com/alexnodeland/stretto/releases). Until then, install from source.
:::

</template>
<template #script>

::: info Coming with the first release
An install script will download the release binaries for your platform. Until then, install from source.
:::

</template>
<template #docker>

::: info Coming with the first release
A Docker image will be published as `ghcr.io/alexnodeland/stretto`. Until then, install from source.
:::

</template>
</Tabs>

## Check the install

```sh
stretto --version
stretto-proxy --version
stretto-procedure --version
```

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
