# Installing stretto

stretto is five binaries, installed together:

- `stretto`, the CLI: `learn`, `flow-show`, `flow-diff`, `audit`, `promote`, `redact`, `init`, `doctor` and the rest ([the CLI reference](cli.md));
- `stretto-proxy`, the MCP proxy that an MCP host runs in place of a server's command;
- `stretto-procedure`, which runs a compiled procedure against an MCP server;
- `stretto-mcp-demo`, a tiny MCP server for trying the proxy, which [the quickstart](../examples/quickstart/README.md) uses;
- `stretto-console`, the web console over `~/.stretto`: servers, sessions, flows and jobs ([the console](console.md)). Releases carry it from the first after 0.1.0.

| Way | Where | Needs |
|---|---|---|
| [Release binaries](#release-binaries) with `install.sh` | Linux x86_64 and aarch64 with glibc 2.35 or later, macOS (Apple silicon and Intel) | curl or wget |
| [`install.ps1`](#windows) | Windows x64 | PowerShell |
| [Homebrew](#homebrew) | macOS, Linux | the tap, which does not exist yet |
| [Docker](#docker) | anywhere Docker runs, linux/amd64 and linux/arm64 | Docker |
| [From source](#from-source) | anywhere Rust 1.88 or later runs | Rust |

Release archives, `install.sh`, `install.ps1` and the container image come with every release, from 0.1.0 on; until 0.1.0 is tagged, [install from source](#from-source). Then [check the installation](#check-it-stretto-doctor) and [add shell completions](#shell-completions).

## Release binaries

```sh
curl -fsSL https://github.com/alexnodeland/stretto/releases/latest/download/install.sh | sh
```

[`install.sh`](../install.sh) picks the archive for the system, downloads it and the release's `SHA256SUMS`, checks the archive's checksum, and copies the binaries into `~/.local/bin`. It changes nothing else: if that directory is not on PATH, it prints the line to add. To read it before running it, download it, then `sh install.sh`. Its options:

- `--version v0.2.0` installs that release instead of the latest. A pre-release (`v0.2.0-rc.1`) is installed only when named.
- `--prefix DIR` installs into `DIR/bin`: `curl -fsSL https://github.com/alexnodeland/stretto/releases/latest/download/install.sh | sudo sh -s -- --prefix /usr/local`.
- `--base-url URL` fetches the release's files from a mirror instead of GitHub.

By hand, download the archive for your system and `SHA256SUMS` from [the release](https://github.com/alexnodeland/stretto/releases), check it, and copy the binaries onto PATH:

```sh
sha256sum --ignore-missing --check SHA256SUMS    # macOS: grep stretto-aarch64-apple-darwin.tar.gz SHA256SUMS | shasum -a 256 --check
tar -xzf stretto-x86_64-unknown-linux-gnu.tar.gz
cp stretto-x86_64-unknown-linux-gnu/stretto* ~/.local/bin/
```

| System | Archive |
|---|---|
| Linux x86_64 | `stretto-x86_64-unknown-linux-gnu.tar.gz` |
| Linux aarch64 | `stretto-aarch64-unknown-linux-gnu.tar.gz` |
| macOS, Apple silicon | `stretto-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `stretto-x86_64-apple-darwin.tar.gz` |
| Windows x64 | `stretto-x86_64-pc-windows-msvc.zip` |

Each archive holds the binaries, `LICENSE` and `README.md`.

- **Linux** binaries are built on Ubuntu 22.04, so they need glibc 2.35 or later: Debian 12, Ubuntu 22.04, Fedora 36 and later. On Alpine and other musl systems, use the [container image](#docker) or [build from source](#from-source).
- **macOS** binaries are not signed with a Developer ID or notarized. A file downloaded with curl, as `install.sh` does, runs as it is. An archive downloaded with a browser is quarantined, and macOS refuses to run its binaries until the attribute is removed: `xattr -d com.apple.quarantine ~/.local/bin/stretto*`.

## Windows

In PowerShell:

```powershell
irm https://github.com/alexnodeland/stretto/releases/latest/download/install.ps1 | iex
```

[`install.ps1`](../install.ps1) does what `install.sh` does, into `%LOCALAPPDATA%\Programs\stretto\bin`, and prints the command that adds it to your PATH. With options: `& ([scriptblock]::Create((irm https://github.com/alexnodeland/stretto/releases/latest/download/install.ps1))) -Version v0.2.0 -Prefix C:\tools\stretto`.

The Windows build is compiled and run with `--version` for each release; CI runs the tests on Linux, and the console's on macOS and Windows as well. The quickstart needs a POSIX shell: run it in WSL or in the container.

## Homebrew

Once the tap exists ([releasing](releasing.md#homebrew)):

```sh
brew install alexnodeland/tap/stretto
```

The formula installs the release binaries and the completions for bash, zsh and fish.

## Docker

```sh
docker run --rm ghcr.io/alexnodeland/stretto --version
docker run --rm --entrypoint /usr/local/share/stretto/quickstart/run.sh ghcr.io/alexnodeland/stretto
```

`ghcr.io/alexnodeland/stretto` is built from the [Dockerfile](../Dockerfile) for linux/amd64 and linux/arm64, tagged with each release's version (`0.2.0`), its minor version (`0.2`) and `latest`. It is Debian 13 slim with the binaries in `/usr/local/bin` and the quickstart in `/usr/local/share/stretto/quickstart`. It runs `stretto` as the user 10001, with `HOME` and the working directory `/data`, so `~/.stretto` is `/data/.stretto`.

To keep the logs and flows, mount a directory there, and run as your own user so that the files are yours:

```sh
docker run --rm --user "$(id -u):$(id -g)" -v "$HOME/.stretto:/data/.stretto" \
  ghcr.io/alexnodeland/stretto learn --sessions /data/.stretto/logs/notes \
  --domain notes --habit-only --out /data/.stretto/notes.flow.json
```

`--entrypoint stretto-proxy` (or `stretto-procedure`, `stretto-mcp-demo`) runs the other binaries. The image has no Node or Python, so as a host's MCP server it can only wrap a server it reaches over Streamable HTTP (`--upstream`), with `docker run -i`:

```json
{
  "mcpServers": {
    "orders": {
      "command": "docker",
      "args": ["run", "-i", "--rm", "--user", "1000:1000", "-v", "/home/me/.stretto:/data/.stretto",
               "--entrypoint", "stretto-proxy", "ghcr.io/alexnodeland/stretto",
               "--record", "/data/.stretto/logs/orders", "--domain", "orders",
               "--upstream", "https://example.com/mcp"]
    }
  }
}
```

**The console** has an image of its own, `ghcr.io/alexnodeland/stretto-console`, from the same build: `stretto-console` as the entrypoint, on port 8080, with a health check. [`compose.yaml`](../compose.yaml) runs it over your `~/.stretto` (`STRETTO_UID=$(id -u) STRETTO_GID=$(id -g) docker compose up -d`); [the console's page](console.md#in-a-container) has the rest.

To build the images from a checkout: `docker build -t stretto .`, and `docker build --target console -t stretto-console .` for the console. Behind a TLS-inspecting proxy, pass the proxy's CA bundle as a build secret and the proxy as a build argument: `docker build --secret id=ca,src=ca.pem --build-arg HTTPS_PROXY=http://proxy:3128 -t stretto .`. Cargo uses them while it builds; neither reaches the image.

## From source

With Rust 1.88 or later ([rustup](https://rustup.rs)):

```sh
cargo install --locked --git https://github.com/alexnodeland/stretto stretto-report stretto-proxy
```

That builds `main`; add `--tag vX.Y.Z` to build a release.

`stretto-report` installs `stretto`, and `stretto-proxy` installs `stretto-proxy`, `stretto-procedure` and `stretto-mcp-demo`. From a checkout:

```sh
cargo install --locked --path crates/stretto-report
cargo install --locked --path crates/stretto-proxy
```

`stretto-console` embeds its UI, so build the UI first (Node 22.12 or later), then install it:

```sh
npm --prefix console ci && npm --prefix console run build
cargo install --locked --path crates/stretto-console
```

stretto is not on crates.io yet ([why](releasing.md#cratesio)).

## Check it: `stretto doctor`

```sh
stretto doctor
```

In the container image, with nothing recorded yet, it prints:

```text
stretto 0.1.0 (/usr/local/bin/stretto)

ok       stretto-proxy 0.1.0 (/usr/local/bin/stretto-proxy)
ok       stretto-procedure 0.1.0 (/usr/local/bin/stretto-procedure)
ok       stretto-mcp-demo 0.1.0 (/usr/local/bin/stretto-mcp-demo)
note     /data/.stretto does not exist yet; `stretto-proxy --record` creates it
note     TYPESAFE_API_KEY is not set. It is optional: recording, `learn --habit-only`, `flow-show`, `flow-diff`, `audit`, `promote` and serving with `--flow-decider habit` or `reach` need no key. With one, stretto asks TypeSafe's Jev: `learn` fits an arbiter, the proxy serves with `--flow-decider arbiter` and runs the confirmation judge, and `phase0 --oracle jev` measures Jev on τ²-bench.
note     no flows in ~/.stretto yet (`stretto learn` writes one)
note     no recorded sessions in ~/.stretto yet
```

It finds the other three binaries on PATH and checks that they are the same version as `stretto`, checks that `~/.stretto` is writable, says whether `TYPESAFE_API_KEY` (or `TYPESAFE_API_KEY_FILE`) is set, without showing it, and lists the flows and recorded sessions in `~/.stretto`. It exits with 1 when something needs fixing, such as `stretto-proxy` missing from PATH. It makes no network request; `stretto doctor --network` also asks Jev one question when a key is set, as `stretto jev-check` does.

## Shell completions

`stretto completions SHELL` prints the completion script for bash, zsh, fish, powershell or elvish:

```sh
# bash (with bash-completion 2)
mkdir -p ~/.local/share/bash-completion/completions
stretto completions bash > ~/.local/share/bash-completion/completions/stretto

# zsh: into a directory on fpath, set in ~/.zshrc before compinit
mkdir -p ~/.zfunc
stretto completions zsh > ~/.zfunc/_stretto
#   in ~/.zshrc: fpath=(~/.zfunc $fpath); autoload -Uz compinit; compinit

# fish
mkdir -p ~/.config/fish/completions
stretto completions fish > ~/.config/fish/completions/stretto.fish
```

```powershell
# PowerShell: add this line to $PROFILE
stretto completions powershell | Out-String | Invoke-Expression
```

For elvish, add `eval (stretto completions elvish | slurp)` to `~/.config/elvish/rc.elv`. Homebrew installs the bash, zsh and fish completions itself.

## Next

`stretto init` prints the configuration that runs your MCP server behind `stretto-proxy`, recording its sessions, for Claude Code, Claude Desktop, Cursor or VS Code, and the steps from there to a served flow:

```sh
stretto init --host claude-code --domain notes -- npx -y @modelcontextprotocol/server-filesystem ~/notes
```

[The quickstart](../examples/quickstart/README.md) runs the whole loop in about a second with no key, and [the walkthrough](walkthrough.md) runs it on the official MCP filesystem server.

## Uninstall

Delete the four binaries: `rm ~/.local/bin/stretto ~/.local/bin/stretto-proxy ~/.local/bin/stretto-procedure ~/.local/bin/stretto-mcp-demo` (or `brew uninstall stretto`, or `cargo uninstall stretto-report stretto-proxy`). `~/.stretto` holds the recorded sessions, the answer cache and the flows; delete it too if you want them gone.
