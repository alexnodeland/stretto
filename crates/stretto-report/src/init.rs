//! `stretto init`: the configuration that runs an MCP server behind
//! `stretto-proxy` in an MCP host, recording its sessions, and the steps
//! from recorded sessions to a served flow.
//!
//! [`Setup::args`] is the proxy's command line, [`snippet`] what the host
//! takes (its JSON configuration, or for Claude Code a `claude mcp add`
//! command), [`config_text`] the whole configuration file that `--write`
//! writes, and [`next_steps`] the loop `docs/walkthrough.md` runs, from the
//! step the configuration is at.
//!
//! With [`Setup::listen`], one proxy serves every host over Streamable HTTP
//! (`stretto-proxy --listen`): [`Setup::args`] starts it, and a host's
//! configuration names its URL, save Claude Desktop's, which starts a proxy
//! of its own.

use crate::flow::Decider;
use crate::surprise::Override;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};

/// An MCP host that `init` configures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    /// Claude Code, configured with `claude mcp add`.
    ClaudeCode,
    /// Claude Desktop, `claude_desktop_config.json`.
    ClaudeDesktop,
    /// Cursor, `mcp.json`.
    Cursor,
    /// VS Code, `.vscode/mcp.json`.
    VsCode,
}

impl Host {
    /// The name `--host` takes.
    pub fn name(self) -> &'static str {
        match self {
            Host::ClaudeCode => "claude-code",
            Host::ClaudeDesktop => "claude-desktop",
            Host::Cursor => "cursor",
            Host::VsCode => "vscode",
        }
    }

    /// Where the configuration goes.
    pub fn placement(self) -> &'static str {
        match self {
            Host::ClaudeCode => {
                "Run the command in the project where you use Claude Code. With `--scope user` \
                 it applies to every project; `--scope project` writes it to .mcp.json, to share."
            }
            Host::ClaudeDesktop => {
                "Merge it into claude_desktop_config.json (on macOS in ~/Library/Application \
                 Support/Claude/, on Windows in %APPDATA%\\Claude\\), then restart Claude Desktop."
            }
            Host::Cursor => {
                "Merge it into ~/.cursor/mcp.json (every project) or .cursor/mcp.json (this \
                 project)."
            }
            Host::VsCode => {
                "Merge it into .vscode/mcp.json in the workspace, or into your user mcp.json \
                 (the command MCP: Open User Configuration)."
            }
        }
    }
}

/// A flow the proxy runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Served {
    /// The flow's file, as the host passes it (the proxy expands `~`).
    pub path: String,
    /// Whether it has an arbiter, which asks a System-One model. A flow
    /// without one decides by counting: by its chance of use before the
    /// next write (`reach`) when it holds those counts, as every flow
    /// `stretto learn` writes does, else on its habit alone.
    pub arbiter: bool,
    /// Whether it holds the counts `--flow-decider reach` reads.
    pub reach: bool,
    /// Shadow mode: decide and log, but look nothing up.
    pub shadow: bool,
    /// The decider to serve it with, in place of the one [`Served::decider`]
    /// picks.
    pub decide_with: Option<Decider>,
    /// The threshold to serve it at (`--flow-threshold`), in place of the
    /// proxy's.
    pub threshold: Option<f64>,
    /// Its surprise gate, off or at another threshold (`--flow-surprise`),
    /// in place of the one it stores.
    pub surprise: Option<Override>,
    /// The only tools it may call on its own (`--flow-tools`); none, every
    /// tool it reads as a lookup.
    pub tools: Vec<String>,
}

impl Served {
    /// The decider to pass the proxy: the one asked for, else none for a
    /// flow with an arbiter, which the proxy serves by default, else `reach`
    /// where the flow holds its counts and `habit` where it does not.
    pub fn decider(&self) -> Option<&'static str> {
        if let Some(decider) = self.decide_with {
            return Some(decider.name());
        }
        match (self.arbiter, self.reach) {
            (true, _) => None,
            (false, true) => Some("reach"),
            (false, false) => Some("habit"),
        }
    }
}

/// What the proxy does besides recording and running a flow: the policy
/// guards on the agent's calls, the confirmation judge on its writes,
/// `stretto_commit`, and how long it keeps sessions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Policy {
    /// Check each of the agent's calls against the domain's policy guards,
    /// and refuse the ones they fail (`--guards`).
    pub guards: bool,
    /// Put each write the guards check for a confirmation to the System-One
    /// model too (`--confirm-judge`).
    pub judge: Option<Judge>,
    /// Offer `stretto_commit`, several calls in one, each checked by the
    /// guards (`--commit`).
    pub commit: bool,
    /// When the proxy starts, delete what is older than this many days: the
    /// sessions it recorded, with the logs beside them, and its cached
    /// answers (`--retain-days`).
    pub retain_days: Option<u64>,
}

/// The confirmation judge the proxy asks about writes.
#[derive(Clone, Debug, PartialEq)]
pub struct Judge {
    /// Refuse the writes it fails, rather than only log its judgments.
    pub enforce: bool,
    /// The file the host appends the conversation to, which the judge reads
    /// (`--context`), as the host passes it.
    pub context: String,
}

impl Policy {
    /// The proxy's arguments for it.
    fn args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(days) = self.retain_days {
            args.extend(["--retain-days".to_string(), days.to_string()]);
        }
        if self.guards {
            args.push("--guards".to_string());
        }
        if let Some(judge) = &self.judge {
            let mode = if judge.enforce { "enforce" } else { "log" };
            args.extend([
                "--confirm-judge".to_string(),
                mode.to_string(),
                "--context".to_string(),
                judge.context.clone(),
            ]);
        }
        if self.commit {
            args.push("--commit".to_string());
        }
        args
    }
}

/// A Streamable HTTP server behind the proxy (`stretto-proxy --upstream`),
/// in place of a command.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Upstream {
    /// The server's MCP endpoint, such as `https://example.com/mcp`.
    pub url: String,
    /// Each header to send, `(NAME, VAR)`: the header, and the environment
    /// variable the proxy reads its value from (`--upstream-header NAME=VAR`).
    /// Values never appear in the configuration.
    pub headers: Vec<(String, String)>,
}

/// The environment variable a host's JSON configuration reads the listening
/// proxy's token from.
pub const TOKEN_VAR: &str = "STRETTO_PROXY_TOKEN";

/// One proxy that hosts connect to by URL (`stretto-proxy --listen`), in
/// place of each host starting its own over stdio.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listening {
    /// The address it listens on, such as `127.0.0.1:8931`.
    pub addr: String,
    /// The file that holds the token hosts send (`--listen-token-file`).
    pub token_file: Option<String>,
}

impl Listening {
    /// Whether it listens on every address (`0.0.0.0` or `[::]`).
    pub fn everywhere(&self) -> bool {
        self.addr
            .parse::<SocketAddr>()
            .is_ok_and(|addr| addr.ip().is_unspecified())
    }

    /// The proxy's MCP endpoint, with loopback named for an unspecified
    /// address, as the proxy prints it.
    pub fn url(&self) -> String {
        let host = match self.addr.parse::<SocketAddr>() {
            Ok(addr) if addr.ip().is_unspecified() => {
                let loopback: IpAddr = if addr.is_ipv4() {
                    Ipv4Addr::LOCALHOST.into()
                } else {
                    Ipv6Addr::LOCALHOST.into()
                };
                SocketAddr::new(loopback, addr.port()).to_string()
            }
            _ => self.addr.clone(),
        };
        format!("http://{host}/mcp")
    }

    /// The proxy's flags for it.
    pub fn args(&self) -> Vec<String> {
        let mut args = vec!["--listen".to_string(), self.addr.clone()];
        if let Some(file) = &self.token_file {
            args.extend(["--listen-token-file".to_string(), file.clone()]);
        }
        args
    }
}

/// A server behind the proxy, as `init` sets it up.
#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    /// The host's name for the server, which is also the sessions' and the
    /// flow's domain.
    pub domain: String,
    /// Where the proxy records sessions, as the host passes it.
    pub record: String,
    /// The flow to run, if any.
    pub flow: Option<Served>,
    /// The proxy's command: `stretto-proxy`, or its path.
    pub proxy: String,
    /// The server's command and its arguments; empty for an [`Upstream`].
    pub server: Vec<String>,
    /// A Streamable HTTP server, in place of a command.
    pub upstream: Option<Upstream>,
    /// The guards, the judge, `stretto_commit` and retention.
    pub policy: Policy,
    /// One proxy that hosts connect to by URL, in place of stdio.
    pub listen: Option<Listening>,
}

impl Setup {
    /// The directory sessions are recorded in by default: one per domain,
    /// since `stretto learn` reads every session in a directory, and shadow
    /// runs apart from the rest, since `stretto promote` reads those.
    pub fn default_record(domain: &str, shadow: bool) -> String {
        let kind = if shadow { "shadow" } else { "logs" };
        format!("~/.stretto/{kind}/{domain}")
    }

    /// The proxy's arguments: with [`Setup::listen`], those of the one
    /// proxy hosts connect to.
    pub fn args(&self) -> Vec<String> {
        let mut args = self.stdio_args();
        if let Some(listen) = &self.listen {
            // Before the server, which comes last.
            let at = args.len() - self.target().len();
            args.splice(at..at, listen.args());
        }
        args
    }

    /// The arguments of a proxy a host starts itself, over stdio.
    pub fn stdio_args(&self) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "--record".into(),
            self.record.clone(),
            "--domain".into(),
            self.domain.clone(),
        ];
        if let Some(flow) = &self.flow {
            args.extend(["--flow".into(), flow.path.clone()]);
            if let Some(decider) = flow.decider() {
                args.extend(["--flow-decider".into(), decider.into()]);
            }
            if let Some(threshold) = flow.threshold {
                args.extend(["--flow-threshold".into(), threshold.to_string()]);
            }
            if let Some(surprise) = flow.surprise {
                args.extend(["--flow-surprise".into(), surprise.to_string()]);
            }
            if !flow.tools.is_empty() {
                args.extend(["--flow-tools".into(), flow.tools.join(",")]);
            }
            if flow.shadow {
                args.push("--flow-shadow".into());
            }
        }
        args.extend(self.policy.args());
        args.extend(self.target());
        args
    }

    /// The URL `host` connects to, if it connects to the listening proxy:
    /// Claude Desktop starts its servers as commands, so it starts its own.
    fn url_for(&self, host: Host) -> Option<String> {
        self.listen
            .as_ref()
            .filter(|_| host != Host::ClaudeDesktop)
            .map(Listening::url)
    }

    /// The `Authorization` header a host's JSON configuration sends to the
    /// listening proxy, naming the token's variable as `host` expands it.
    fn bearer(&self, host: Host) -> Option<String> {
        self.listen.as_ref()?.token_file.as_ref()?;
        Some(match host {
            Host::ClaudeCode => format!("Bearer ${{{TOKEN_VAR}}}"),
            _ => format!("Bearer ${{env:{TOKEN_VAR}}}"),
        })
    }

    /// The server, as the proxy's last arguments: `--upstream` and its
    /// headers, or `--` and the command.
    fn target(&self) -> Vec<String> {
        match &self.upstream {
            Some(upstream) => {
                let mut args = vec!["--upstream".to_string(), upstream.url.clone()];
                for (name, var) in &upstream.headers {
                    args.extend(["--upstream-header".to_string(), format!("{name}={var}")]);
                }
                args
            }
            None => std::iter::once("--".to_string())
                .chain(self.server.iter().cloned())
                .collect(),
        }
    }
}

/// A domain names a directory and files, so it keeps to letters, digits,
/// `-`, `_` and `.`, and does not start with a dot or a dash.
pub fn check_domain(domain: &str) -> Result<()> {
    let plain = domain
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    if domain.is_empty() || !plain || domain.starts_with(['.', '-']) {
        bail!(
            "--domain {domain:?}: the domain names the server, a directory and a flow's file, \
             so use letters, digits, `-`, `_` and `.`"
        );
    }
    Ok(())
}

/// The host's whole configuration file with only this server. For Claude
/// Code it is a project's `.mcp.json`.
pub fn config_file(host: Host, setup: &Setup) -> Value {
    let (key, mut server) = match host {
        Host::VsCode => ("servers", json!({"type": "stdio"})),
        Host::ClaudeCode | Host::ClaudeDesktop | Host::Cursor => ("mcpServers", json!({})),
    };
    match setup.url_for(host) {
        Some(url) => {
            server = match host {
                Host::Cursor => json!({ "url": url }),
                _ => json!({ "type": "http", "url": url }),
            };
            if let Some(bearer) = setup.bearer(host) {
                server["headers"] = json!({ "Authorization": bearer });
            }
        }
        None => {
            server["command"] = json!(setup.proxy);
            server["args"] = json!(setup.stdio_args());
        }
    }
    json!({ key: { setup.domain.as_str(): server } })
}

/// [`config_file`] as text, laid out as the hosts' documentation lays it
/// out: `command` before `args`, and the arguments on one line.
pub fn config_text(host: Host, setup: &Setup) -> String {
    let s = |text: &str| Value::from(text).to_string();
    let (key, kind) = match host {
        Host::VsCode => ("servers", "      \"type\": \"stdio\",\n"),
        Host::ClaudeCode | Host::ClaudeDesktop | Host::Cursor => ("mcpServers", ""),
    };
    let body = match setup.url_for(host) {
        Some(url) => {
            let mut fields = Vec::new();
            if host != Host::Cursor {
                fields.push("      \"type\": \"http\"".to_string());
            }
            fields.push(format!("      \"url\": {}", s(&url)));
            if let Some(bearer) = setup.bearer(host) {
                fields.push(format!(
                    "      \"headers\": {{ \"Authorization\": {} }}",
                    s(&bearer)
                ));
            }
            fields.join(",\n") + "\n"
        }
        None => {
            let args: Vec<String> = setup.stdio_args().iter().map(|a| s(a)).collect();
            format!(
                "{kind}      \"command\": {},\n      \"args\": [{}]\n",
                s(&setup.proxy),
                args.join(", ")
            )
        }
    };
    format!(
        "{{\n  \"{key}\": {{\n    {}: {{\n{body}    }}\n  }}\n}}\n",
        s(&setup.domain)
    )
}

/// What the host takes: the configuration as JSON, or for Claude Code the
/// `claude mcp add` command.
pub fn snippet(host: Host, setup: &Setup) -> String {
    match (host, setup.url_for(host)) {
        (Host::ClaudeCode, Some(url)) => {
            let mut words: Vec<String> = [
                "claude",
                "mcp",
                "add",
                "--transport",
                "http",
                &setup.domain,
                &url,
            ]
            .into_iter()
            .map(shell_quote)
            .collect();
            if let Some(file) = setup.listen.as_ref().and_then(|l| l.token_file.as_ref()) {
                // The shell reads the token from its file as it adds the server.
                words.push("--header".to_string());
                words.push(format!(
                    "\"Authorization: Bearer $(cat {})\"",
                    shell_quote(file)
                ));
            }
            format!("{}\n", words.join(" "))
        }
        (Host::ClaudeCode, None) => {
            let words: Vec<String> = ["claude", "mcp", "add", &setup.domain, "--", &setup.proxy]
                .into_iter()
                .map(String::from)
                .chain(setup.stdio_args())
                .collect();
            let quoted: Vec<String> = words.iter().map(|w| shell_quote(w)).collect();
            format!("{}\n", quoted.join(" "))
        }
        _ => config_text(host, setup),
    }
}

/// Where `host`'s configuration goes ([`Host::placement`]), and with
/// [`Setup::listen`], what else the host needs ([`listen_note`]).
pub fn placement(host: Host, setup: &Setup) -> String {
    let mut text = host.placement().to_string();
    if let Some(note) = listen_note(host, setup, false) {
        text.push(' ');
        text.push_str(&note);
    }
    text
}

/// With [`Setup::listen`], what else `host` needs: the token's variable,
/// when its configuration reads the token from there, and this machine's
/// name, for a proxy on every address; or for Claude Desktop, why it starts
/// a proxy of its own. `written`: the configuration file ([`config_text`]),
/// which reads the token from the variable for Claude Code too, rather than
/// [`snippet`]'s command, which reads its file.
pub fn listen_note(host: Host, setup: &Setup, written: bool) -> Option<String> {
    let listen = setup.listen.as_ref()?;
    if host == Host::ClaudeDesktop {
        return Some(format!(
            "Claude Desktop starts its servers as commands, so it runs a proxy of its own, over \
             stdio, rather than connecting to the one at {}.",
            listen.url()
        ));
    }
    let mut notes = Vec::new();
    match &listen.token_file {
        Some(file) if written || host != Host::ClaudeCode => notes.push(format!(
            "The host sends the token from {TOKEN_VAR}: set it, in the environment the host \
             starts in, to the token in {file}."
        )),
        _ => {}
    }
    if listen.everywhere() {
        notes.push(
            "The proxy listens on every address: a host on another machine puts this one's name \
             in the URL."
                .to_string(),
        );
    }
    (!notes.is_empty()).then(|| notes.join(" "))
}

/// `word` as a POSIX shell reads it back: bare when it is plain, else in
/// single quotes. A leading `~` stays bare, so the shell expands it.
pub fn shell_quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,~".contains(c));
    if plain {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// Write `text` to `path`, creating its directory, but never over an
/// existing file unless `force`.
pub fn write_new(path: &Path, text: &str, force: bool) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut file = match options.open(path) {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => bail!(
            "{} exists: merge the configuration into it by hand, or pass --force to replace the \
             whole file, with any other servers in it",
            path.display()
        ),
        other => other.with_context(|| format!("writing {}", path.display()))?,
    };
    file.write_all(text.as_bytes())
        .with_context(|| format!("writing {}", path.display()))
}

/// `path` with a leading `~` expanded to `home`.
pub fn expand_home(path: &str, home: Option<&Path>) -> PathBuf {
    let rest = match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            rest.trim_start_matches(['/', '\\'])
        }
        _ => return PathBuf::from(path),
    };
    match home {
        Some(home) if rest.is_empty() => home.to_path_buf(),
        Some(home) => home.join(rest),
        None => PathBuf::from(path),
    }
}

/// The steps from where `setup` is to a served flow, with the commands to
/// run. `init` is how to call `stretto init` again for the same host and
/// server, up to the options that differ.
pub fn next_steps(host: Host, setup: &Setup, init: &str) -> String {
    // The guards, the judge, `stretto_commit`, retention and listening are
    // init's options too, the same at every step.
    let mut init = init.to_string();
    let listen = setup.listen.iter().flat_map(Listening::args);
    for word in setup.policy.args().into_iter().chain(listen) {
        init.push(' ');
        init.push_str(&shell_quote(&word));
    }
    let init = init.as_str();
    let d = &setup.domain;
    let server: Vec<String> = setup.target().iter().map(|w| shell_quote(w)).collect();
    let server = server.join(" ");
    let (flow, promoted) = (
        format!("~/.stretto/{d}.flow.json"),
        format!("~/.stretto/{d}-promoted.flow.json"),
    );
    let mut steps: Vec<String> = Vec::new();
    // A host that connects by URL keeps its configuration; the proxy changes.
    let (replace_above, replace) = match setup.url_for(host) {
        Some(url) => {
            let words: Vec<String> = std::iter::once(setup.proxy.clone())
                .chain(setup.args())
                .map(|w| shell_quote(&w))
                .collect();
            steps.push(format!(
                "Start the proxy, and keep it running: each host that connects to {url} gets a \
                 session of its own.\n{}",
                words.join(" ")
            ));
            let restart = "Restart the proxy with the command this prints";
            (restart, restart)
        }
        None => (
            "Replace the configuration above with what this prints",
            "Replace the configuration with what this prints",
        ),
    };
    match &setup.flow {
        None => {
            steps.push(format!(
                "Use the agent as usual. The proxy records each session in {}.",
                setup.record
            ));
            steps.push(format!(
                "Learn a flow from the sessions, with no key:\n\
                 stretto learn --sessions {} --domain {d} --habit-only --out {flow}",
                setup.record
            ));
            steps.push(format!(
                "Review what it may do: the tools it may call, the lookups it may make and \
                 where their arguments come from:\nstretto flow-show {flow}"
            ));
            steps.push(format!(
                "Run it in shadow: it decides and logs, but looks nothing up. {replace_above}:\n\
                 {init} --flow {flow} --shadow {server}"
            ));
            let shadow = Setup::default_record(d, true);
            // `stretto learn` writes a flow with the counts `reach` reads.
            steps.push(promote_step(
                &flow,
                &shadow,
                &promoted,
                false,
                Some("reach"),
            ));
            steps.push(serve_step(init, &promoted, &server, replace));
        }
        Some(served) if served.shadow => {
            steps.push(format!(
                "Use the agent as usual. The proxy records each session in {}, and next to it \
                 what the flow would have looked up (<session>.flow.jsonl).",
                setup.record
            ));
            steps.push(promote_step(
                &served.path,
                &setup.record,
                &promoted,
                served.arbiter,
                served.decider(),
            ));
            steps.push(serve_step(init, &promoted, &server, replace));
        }
        Some(served) => {
            steps.push(format!(
                "Use the agent as usual. After each of its calls, the flow's lookups ride in \
                 the same result. The proxy records each session in {}, and next to it each of \
                 the flow's decisions (<session>.flow.jsonl).",
                setup.record
            ));
            steps.push(format!(
                "When the agent, its prompts or the server change, learn again and review what \
                 changed (flow-diff exits with 1 when a change needs review):\n\
                 stretto learn --sessions {} --domain {d} --habit-only --out ~/.stretto/{d}-new.flow.json\n\
                 stretto flow-diff {} ~/.stretto/{d}-new.flow.json",
                setup.record, served.path
            ));
        }
    }
    let mut out = String::from("Next steps (docs/walkthrough.md runs them on a real server):\n");
    for (i, step) in steps.iter().enumerate() {
        let mut lines = step.lines();
        out.push_str(&format!(
            "\n{}. {}\n",
            i + 1,
            lines.next().unwrap_or_default()
        ));
        for line in lines {
            out.push_str(&format!("     {line}\n"));
        }
    }
    if setup.flow.as_ref().is_some_and(|f| f.arbiter) {
        out.push_str(
            "\nThis flow has an arbiter, which asks TypeSafe's Jev at each decision: give the \
             server TYPESAFE_API_KEY (or TYPESAFE_API_KEY_FILE) in the host's `env`, or serve it \
             with `--flow-decider habit`, which needs no key.\n",
        );
    }
    if let Some(judge) = &setup.policy.judge {
        out.push_str(&format!(
            "\nThe confirmation judge asks TypeSafe's Jev about each write the guards check for a \
             confirmation: give the server TYPESAFE_API_KEY (or TYPESAFE_API_KEY_FILE) in the \
             host's `env`. It reads the conversation from {}, which the host appends to, one JSON \
             line per message: {{\"role\": \"user\" | \"assistant\", \"content\": text}}.\n",
            judge.context
        ));
    }
    out
}

/// Promote `flow` on the shadow sessions in `sessions`, scoring the decisions
/// the flow will be served with.
fn promote_step(
    flow: &str,
    sessions: &str,
    promoted: &str,
    arbiter: bool,
    decider: Option<&str>,
) -> String {
    // A flow with an arbiter replays the answers the proxy cached in shadow.
    let cache = if arbiter {
        " --oracle-cache ~/.stretto/oracle-cache"
    } else {
        ""
    };
    let decider = decider.map_or(String::new(), |d| format!(" --decider {d}"));
    format!(
        "Keep the flow to the calls where its lookups were the agent's own:\n\
         stretto promote --flow {flow} --sessions {sessions}{decider}{cache} --out {promoted}"
    )
}

fn serve_step(init: &str, promoted: &str, server: &str, replace: &str) -> String {
    format!(
        "Serve it: after each of the agent's calls, the flow's lookups ride in the same result. \
         {replace}:\n{init} --flow {promoted} {server}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(flow: Option<Served>) -> Setup {
        let shadow = flow.as_ref().is_some_and(|f| f.shadow);
        Setup {
            domain: "notes".to_string(),
            record: Setup::default_record("notes", shadow),
            flow,
            proxy: "stretto-proxy".to_string(),
            server: [
                "npx",
                "-y",
                "@modelcontextprotocol/server-filesystem",
                "/home/me/notes",
            ]
            .map(String::from)
            .to_vec(),
            upstream: None,
            policy: Policy::default(),
            listen: None,
        }
    }

    const INIT: &str = "stretto init --host cursor --domain notes";

    #[test]
    fn the_proxy_records_each_domain_apart_and_serves_a_flow_without_an_arbiter_by_counting() {
        assert_eq!(
            setup(None).args(),
            [
                "--record",
                "~/.stretto/logs/notes",
                "--domain",
                "notes",
                "--",
                "npx",
                "-y",
                "@modelcontextprotocol/server-filesystem",
                "/home/me/notes"
            ]
        );
        // A flow `stretto learn` writes holds the counts `reach` reads.
        let shadow = setup(Some(Served {
            path: "/home/me/.stretto/notes.flow.json".to_string(),
            arbiter: false,
            reach: true,
            shadow: true,
            decide_with: None,
            threshold: None,
            surprise: None,
            tools: Vec::new(),
        }));
        assert_eq!(
            shadow.args()[..9],
            [
                "--record",
                "~/.stretto/shadow/notes",
                "--domain",
                "notes",
                "--flow",
                "/home/me/.stretto/notes.flow.json",
                "--flow-decider",
                "reach",
                "--flow-shadow"
            ]
        );
        // One learned before flows held them is served on its habit.
        let older = setup(Some(Served {
            path: "f.json".to_string(),
            arbiter: false,
            reach: false,
            shadow: false,
            decide_with: None,
            threshold: None,
            surprise: None,
            tools: Vec::new(),
        }));
        assert_eq!(
            older.args()[4..8],
            ["--flow", "f.json", "--flow-decider", "habit"]
        );
        // A flow with an arbiter is served as the proxy serves it by default.
        let arbiter = setup(Some(Served {
            path: "f.json".to_string(),
            arbiter: true,
            reach: false,
            shadow: false,
            decide_with: None,
            threshold: None,
            surprise: None,
            tools: Vec::new(),
        }));
        assert!(!arbiter.args().contains(&"--flow-decider".to_string()));
        assert!(!arbiter.args().contains(&"--flow-shadow".to_string()));
    }

    #[test]
    fn json_hosts_get_their_own_shape() {
        let s = setup(None);
        for host in [Host::ClaudeDesktop, Host::Cursor, Host::ClaudeCode] {
            let config = config_file(host, &s);
            let server = &config["mcpServers"]["notes"];
            assert_eq!(server["command"], "stretto-proxy", "{}", host.name());
            assert_eq!(server["args"], json!(s.args()), "{}", host.name());
            assert!(server.get("type").is_none());
        }
        let vscode = config_file(Host::VsCode, &s);
        assert_eq!(vscode["servers"]["notes"]["type"], "stdio");
        assert_eq!(vscode["servers"]["notes"]["args"], json!(s.args()));
        // What is printed and written is that JSON, command first.
        let mut odd = s.clone();
        odd.server.push("a \"quoted\" \\ path".to_string());
        for host in [
            Host::ClaudeDesktop,
            Host::Cursor,
            Host::VsCode,
            Host::ClaudeCode,
        ] {
            let text = config_text(host, &odd);
            let parsed: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(parsed, config_file(host, &odd), "{text}");
            assert!(text.find("\"command\"") < text.find("\"args\""), "{text}");
        }
        assert_eq!(
            snippet(Host::Cursor, &s),
            "{\n  \"mcpServers\": {\n    \"notes\": {\n      \"command\": \"stretto-proxy\",\n      \
             \"args\": [\"--record\", \"~/.stretto/logs/notes\", \"--domain\", \"notes\", \"--\", \
             \"npx\", \"-y\", \"@modelcontextprotocol/server-filesystem\", \"/home/me/notes\"]\n    \
             }\n  }\n}\n"
        );
    }

    #[test]
    fn claude_code_gets_a_command_a_shell_reads_back() {
        let mut s = setup(None);
        s.server.push("it's here".to_string());
        assert_eq!(
            snippet(Host::ClaudeCode, &s),
            "claude mcp add notes -- stretto-proxy --record ~/.stretto/logs/notes \
             --domain notes -- npx -y @modelcontextprotocol/server-filesystem /home/me/notes \
             'it'\\''s here'\n"
        );
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("$HOME"), "'$HOME'");
        assert_eq!(shell_quote("~/x"), "~/x");
    }

    #[test]
    fn the_next_steps_start_where_the_configuration_is() {
        let record = next_steps(Host::Cursor, &setup(None), INIT);
        for line in [
            "stretto learn --sessions ~/.stretto/logs/notes --domain notes --habit-only \
             --out ~/.stretto/notes.flow.json",
            "stretto flow-show ~/.stretto/notes.flow.json",
            "stretto init --host cursor --domain notes --flow ~/.stretto/notes.flow.json \
             --shadow -- npx -y @modelcontextprotocol/server-filesystem /home/me/notes",
            "stretto promote --flow ~/.stretto/notes.flow.json --sessions \
             ~/.stretto/shadow/notes --decider reach --out ~/.stretto/notes-promoted.flow.json",
            "stretto init --host cursor --domain notes --flow \
             ~/.stretto/notes-promoted.flow.json -- npx -y",
        ] {
            assert!(record.contains(line), "{line}\n\n{record}");
        }
        assert!(record.contains("\n6. Serve it"), "{record}");
        assert!(!record.contains("TYPESAFE_API_KEY"), "{record}");

        let shadow = next_steps(
            Host::Cursor,
            &setup(Some(Served {
                path: "~/.stretto/notes.flow.json".to_string(),
                arbiter: true,
                reach: false,
                shadow: true,
                decide_with: None,
                threshold: None,
                surprise: None,
                tools: Vec::new(),
            })),
            INIT,
        );
        assert!(shadow.starts_with("Next steps"), "{shadow}");
        assert!(shadow.contains("\n2. Keep the flow"), "{shadow}");
        assert!(
            shadow.contains("--oracle-cache ~/.stretto/oracle-cache"),
            "{shadow}"
        );
        assert!(shadow.contains("TYPESAFE_API_KEY"), "{shadow}");
        let mut custom = setup(Some(Served {
            path: "~/.stretto/notes.flow.json".to_string(),
            arbiter: false,
            reach: true,
            shadow: true,
            decide_with: None,
            threshold: None,
            surprise: None,
            tools: Vec::new(),
        }));
        custom.record = "/srv/shadow".to_string();
        let custom = next_steps(Host::Cursor, &custom, INIT);
        assert!(
            custom.contains("--sessions /srv/shadow --decider reach --out"),
            "{custom}"
        );
        assert!(!custom.contains("--oracle-cache"), "{custom}");

        let served = next_steps(
            Host::Cursor,
            &setup(Some(Served {
                path: "~/.stretto/notes-promoted.flow.json".to_string(),
                arbiter: false,
                reach: false,
                shadow: false,
                decide_with: None,
                threshold: None,
                surprise: None,
                tools: Vec::new(),
            })),
            INIT,
        );
        assert!(
            served.contains("stretto flow-diff ~/.stretto/notes-promoted.flow.json"),
            "{served}"
        );
        assert!(!served.contains("promote --flow"), "{served}");
    }

    #[test]
    fn an_http_server_is_an_upstream_with_headers_by_variable() {
        let mut s = setup(Some(Served {
            path: "~/.stretto/notes.flow.json".to_string(),
            arbiter: true,
            reach: true,
            shadow: false,
            decide_with: Some(Decider::Habit),
            threshold: Some(0.5),
            surprise: Some(Override::Off),
            tools: vec!["read_text_file".to_string(), "list_directory".to_string()],
        }));
        s.server.clear();
        s.upstream = Some(Upstream {
            url: "https://example.com/mcp".to_string(),
            headers: vec![("Authorization".to_string(), "NOTES_AUTH".to_string())],
        });
        assert_eq!(
            s.args()[4..],
            [
                "--flow",
                "~/.stretto/notes.flow.json",
                "--flow-decider",
                "habit",
                "--flow-threshold",
                "0.5",
                "--flow-surprise",
                "off",
                "--flow-tools",
                "read_text_file,list_directory",
                "--upstream",
                "https://example.com/mcp",
                "--upstream-header",
                "Authorization=NOTES_AUTH"
            ]
        );
        assert!(!s.args().contains(&"--".to_string()));
        let steps = next_steps(Host::Cursor, &s, INIT);
        assert!(
            steps.contains(
                "stretto flow-diff ~/.stretto/notes.flow.json ~/.stretto/notes-new.flow.json"
            ),
            "{steps}"
        );
        s.flow = None;
        let steps = next_steps(Host::Cursor, &s, INIT);
        assert!(
            steps.contains(
                "--flow ~/.stretto/notes.flow.json --shadow --upstream https://example.com/mcp \
                 --upstream-header Authorization=NOTES_AUTH"
            ),
            "{steps}"
        );
    }

    #[test]
    fn a_domain_names_files_so_it_stays_plain() {
        for ok in ["notes", "orders-v2", "a.b_c"] {
            assert!(check_domain(ok).is_ok(), "{ok}");
        }
        for bad in ["", "../x", "a/b", ".hidden", "-x", "my notes"] {
            assert!(check_domain(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn write_refuses_an_existing_file_without_force() {
        let dir = std::env::temp_dir().join(format!("stretto-init-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join(".vscode").join("mcp.json");
        write_new(&path, "first", false).unwrap();
        let refused = write_new(&path, "second", false).unwrap_err().to_string();
        assert!(refused.contains("--force"), "{refused}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");
        write_new(&path, "second", true).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_leading_tilde_is_the_home_directory() {
        let home = Path::new("/home/me");
        assert_eq!(
            expand_home("~/.stretto/f.json", Some(home)),
            Path::new("/home/me/.stretto/f.json")
        );
        assert_eq!(expand_home("~", Some(home)), home);
        assert_eq!(expand_home("~bob/f", Some(home)), Path::new("~bob/f"));
        assert_eq!(expand_home("~/f", None), Path::new("~/f"));
        assert_eq!(expand_home("/abs/f", Some(home)), Path::new("/abs/f"));
    }

    #[test]
    fn the_next_steps_keep_the_policy_and_say_what_the_judge_needs() {
        let mut s = setup(None);
        s.domain = "retail".to_string();
        s.policy = Policy {
            guards: true,
            judge: Some(Judge {
                enforce: false,
                context: "~/.stretto/context/retail.jsonl".to_string(),
            }),
            commit: true,
            retain_days: Some(30),
        };
        let steps = next_steps(
            Host::Cursor,
            &s,
            "stretto init --host cursor --domain retail",
        );
        assert!(
            steps.contains(
                "stretto init --host cursor --domain retail --retain-days 30 --guards \
                 --confirm-judge log --context ~/.stretto/context/retail.jsonl --commit \
                 --flow ~/.stretto/retail.flow.json --shadow -- npx"
            ),
            "{steps}"
        );
        assert!(
            steps.contains("reads the conversation from ~/.stretto/context/retail.jsonl"),
            "{steps}"
        );
        assert!(!next_steps(Host::Cursor, &setup(None), INIT).contains("confirmation judge"));
    }

    #[test]
    fn a_listening_proxy_gives_hosts_its_url_and_the_command_that_starts_it() {
        let mut s = setup(None);
        s.listen = Some(Listening {
            addr: "0.0.0.0:8931".into(),
            token_file: Some("~/.stretto/proxy-token".into()),
        });
        // The proxy listens, and the server comes last.
        assert_eq!(
            s.args()[4..9],
            [
                "--listen",
                "0.0.0.0:8931",
                "--listen-token-file",
                "~/.stretto/proxy-token",
                "--"
            ]
        );
        assert!(!s.stdio_args().contains(&"--listen".to_string()));
        // Each host connects by URL, loopback for an unspecified address,
        // and sends the token.
        assert_eq!(
            snippet(Host::ClaudeCode, &s),
            "claude mcp add --transport http notes http://127.0.0.1:8931/mcp \
             --header \"Authorization: Bearer $(cat ~/.stretto/proxy-token)\"\n"
        );
        let cursor = config_file(Host::Cursor, &s);
        assert_eq!(
            cursor,
            json!({"mcpServers": {"notes": {
                "url": "http://127.0.0.1:8931/mcp",
                "headers": {"Authorization": "Bearer ${env:STRETTO_PROXY_TOKEN}"}}}})
        );
        for host in [Host::ClaudeCode, Host::Cursor, Host::VsCode] {
            let text: Value = serde_json::from_str(&config_text(host, &s)).unwrap();
            assert_eq!(text, config_file(host, &s), "{host:?}");
        }
        let code = &config_file(Host::ClaudeCode, &s)["mcpServers"]["notes"];
        assert_eq!(
            (
                code["type"].clone(),
                code["headers"]["Authorization"].clone()
            ),
            (json!("http"), json!("Bearer ${STRETTO_PROXY_TOKEN}"))
        );
        assert_eq!(
            config_file(Host::VsCode, &s)["servers"]["notes"]["type"],
            "http"
        );
        // Claude Desktop starts a proxy of its own, over stdio, and says why.
        let desktop = config_file(Host::ClaudeDesktop, &s);
        assert_eq!(
            desktop["mcpServers"]["notes"]["args"],
            json!(s.stdio_args())
        );
        assert!(placement(Host::ClaudeDesktop, &s)
            .ends_with("rather than connecting to the one at http://127.0.0.1:8931/mcp."));
        // Beyond this machine, a host names it in the URL.
        let everywhere = " The proxy listens on every address: a host on another machine puts \
                          this one's name in the URL.";
        assert_eq!(
            placement(Host::Cursor, &s),
            format!(
                "{} The host sends the token from STRETTO_PROXY_TOKEN: set it, in the environment \
                 the host starts in, to the token in ~/.stretto/proxy-token.{everywhere}",
                Host::Cursor.placement()
            )
        );
        assert_eq!(
            placement(Host::ClaudeCode, &s),
            format!("{}{everywhere}", Host::ClaudeCode.placement())
        );
        // Written, Claude Code's configuration reads the token from the
        // variable too.
        assert!(listen_note(Host::ClaudeCode, &s, true)
            .unwrap()
            .starts_with("The host sends the token from STRETTO_PROXY_TOKEN"));
        // The steps start the proxy, then restart it with each change.
        let steps = next_steps(Host::Cursor, &s, INIT);
        assert!(
            steps.contains(
                "1. Start the proxy, and keep it running: each host that connects to \
                 http://127.0.0.1:8931/mcp gets a session of its own.\n     stretto-proxy \
                 --record ~/.stretto/logs/notes --domain notes --listen 0.0.0.0:8931 \
                 --listen-token-file ~/.stretto/proxy-token -- npx"
            ),
            "{steps}"
        );
        assert!(
            steps.contains(
                "Restart the proxy with the command this prints:\n     stretto init --host \
                 cursor --domain notes --listen 0.0.0.0:8931 --listen-token-file \
                 ~/.stretto/proxy-token --flow"
            ),
            "{steps}"
        );
        assert!(!next_steps(Host::ClaudeDesktop, &s, INIT).contains("Start the proxy"));

        // Without a token, no header; a named address is its own URL.
        s.listen = Some(Listening {
            addr: "127.0.0.1:8931".into(),
            token_file: None,
        });
        assert_eq!(
            snippet(Host::ClaudeCode, &s),
            "claude mcp add --transport http notes http://127.0.0.1:8931/mcp\n"
        );
        assert!(config_file(Host::Cursor, &s)["mcpServers"]["notes"]
            .get("headers")
            .is_none());
        assert_eq!(placement(Host::Cursor, &s), Host::Cursor.placement());
        assert_eq!(listen_note(Host::VsCode, &s, true), None);
        let url = |addr: &str| {
            Listening {
                addr: addr.into(),
                token_file: None,
            }
            .url()
        };
        assert_eq!(url("[::]:1"), "http://[::1]:1/mcp");
        assert_eq!(url("localhost:1"), "http://localhost:1/mcp");
    }

    #[test]
    fn a_file_that_cannot_be_made_says_why() {
        let dir = std::env::temp_dir().join(format!("stretto-init-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("taken");
        std::fs::write(&file, "x").unwrap();
        let error = |e: anyhow::Error| format!("{e:#}");
        let under = error(write_new(&file.join("mcp.json"), "{}", false).unwrap_err());
        assert!(under.starts_with("creating "), "{under}");
        // No directory to make, and no file to write in its place.
        let root = error(write_new(Path::new("/"), "{}", true).unwrap_err());
        assert!(root.starts_with("writing /"), "{root}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_file_that_takes_no_bytes_says_so() {
        let full = format!(
            "{:#}",
            write_new(Path::new("/dev/full"), "{}", true).unwrap_err()
        );
        assert!(full.starts_with("writing /dev/full"), "{full}");
    }
}
