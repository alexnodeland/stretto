//! The server registry, `servers.json` in the data directory: the MCP
//! servers stretto fronts, how each is proxied, and with which flow.
//!
//! ```json
//! {"stretto_servers": 1, "servers": [{"name": "shop", "upstream": {"kind": "stdio", …}, …}]}
//! ```
//!
//! It never holds a secret: a server's environment variables and HTTP
//! headers are kept by name, and their values come from the proxy's own
//! environment. Arguments and URLs that look like credentials are refused
//! ([`validate`]). It is written whole to a temporary file beside it, then
//! renamed over it ([`save`]), so a reader never sees half of it.

use crate::data::paths;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(feature = "ts")]
use ts_rs::TS;

/// The registry's file name in the data directory.
pub const FILE: &str = "servers.json";
/// The format this build reads and writes.
pub const VERSION: u32 = 1;

/// `servers.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct Registry {
    pub stretto_servers: u32,
    pub servers: Vec<ServerEntry>,
}

/// How the proxy runs in front of a server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[serde(rename_all = "snake_case")]
pub enum ServerMode {
    /// Record sessions, run no flow.
    Record,
    /// Run the flow in shadow: decide and log, look nothing up.
    Shadow,
    /// Serve the flow.
    Serve,
}

/// Where a flow's probabilities come from (`--flow-decider`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[serde(rename_all = "snake_case")]
pub enum DeciderName {
    Arbiter,
    Habit,
    Reach,
}

impl DeciderName {
    pub fn of(decider: stretto_report::flow::Decider) -> Self {
        use stretto_report::flow::Decider;
        match decider {
            Decider::Arbiter => Self::Arbiter,
            Decider::Habit => Self::Habit,
            Decider::Reach => Self::Reach,
        }
    }

    pub fn decider(self) -> stretto_report::flow::Decider {
        use stretto_report::flow::Decider;
        match self {
            Self::Arbiter => Decider::Arbiter,
            Self::Habit => Decider::Habit,
            Self::Reach => Decider::Reach,
        }
    }
}

/// An HTTP header the proxy sends, by the variable holding its value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct HeaderRef {
    /// The header, such as `Authorization`.
    pub name: String,
    /// The environment variable holding its value, such as `ORDERS_AUTH`.
    pub env: String,
}

/// The server behind the proxy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Upstream {
    /// A command the proxy starts.
    Stdio {
        command: Vec<String>,
        /// Variables the server needs, by name: documentation for whoever
        /// configures the host; values are never stored.
        #[serde(default)]
        env: Vec<String>,
    },
    /// A Streamable HTTP server (`--upstream`).
    Http {
        url: String,
        #[serde(default)]
        headers: Vec<HeaderRef>,
    },
}

/// A server in the registry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct ServerEntry {
    /// The host's name for the server, which is also the domain of its
    /// sessions and flows (`a-z`, `0-9`, `_`, `-`).
    pub name: String,
    pub description: Option<String>,
    pub upstream: Upstream,
    pub mode: ServerMode,
    /// The flow's file: relative to the data directory, `~/…`, or absolute
    /// inside the data directory.
    pub flow: Option<String>,
    /// Where the proxy records sessions; by default `logs/<name>` in the
    /// data directory (`shadow/<name>` in shadow mode).
    pub record_dir: Option<String>,
    /// The decider to serve the flow with, in place of its default.
    pub decider: Option<DeciderName>,
    /// The threshold to serve the flow at, in place of the proxy's 0.3.
    pub threshold: Option<f64>,
    pub created_unix_ms: u64,
    pub updated_unix_ms: u64,
}

/// A server as a request describes it: an entry without its timestamps.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct ServerInput {
    pub name: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub description: Option<String>,
    pub upstream: Upstream,
    pub mode: ServerMode,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub flow: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub record_dir: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub decider: Option<DeciderName>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub threshold: Option<f64>,
}

impl ServerInput {
    /// The entry, created at `created` and changed at `now`.
    pub fn entry(self, created: u64, now: u64) -> ServerEntry {
        let blank = |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        ServerEntry {
            name: self.name,
            description: blank(self.description),
            upstream: self.upstream,
            mode: self.mode,
            flow: blank(self.flow),
            record_dir: blank(self.record_dir),
            decider: self.decider,
            threshold: self.threshold,
            created_unix_ms: created,
            updated_unix_ms: now,
        }
    }
}

/// The registry in `root`: empty when there is none yet.
pub fn load(root: &Path) -> Result<Registry, String> {
    let path = root.join(FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Registry {
                stretto_servers: VERSION,
                servers: Vec::new(),
            })
        }
        Err(e) => return Err(format!("reading {FILE}: {e}")),
    };
    #[derive(Deserialize)]
    struct Version {
        stretto_servers: u32,
    }
    let v: Version =
        serde_json::from_str(&text).map_err(|e| format!("{FILE} is not a server registry: {e}"))?;
    if v.stretto_servers != VERSION {
        return Err(format!(
            "{FILE} is format {}; this build reads {VERSION}",
            v.stretto_servers
        ));
    }
    serde_json::from_str(&text).map_err(|e| format!("{FILE}: {e}"))
}

/// Write `registry` to `root`, whole or not at all: to a temporary file
/// beside it, flushed to disk, then renamed over it.
pub fn save(root: &Path, registry: &Registry) -> Result<(), String> {
    let path = root.join(FILE);
    let temp = root.join(format!(
        ".{FILE}.{}.{}.tmp",
        std::process::id(),
        crate::now_unix_ms()
    ));
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(root)?;
        let mut file = std::fs::File::create(&temp)?;
        let mut text = serde_json::to_string_pretty(registry).map_err(std::io::Error::other)?;
        text.push('\n');
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temp, &path)
    };
    let result = write();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(|e| format!("writing {FILE}: {e}"))
}

/// Why `input` cannot go in the registry of `root`, if it cannot: a name
/// that is not a plain domain, an upstream without a command or with an
/// unusable URL, a credential where only names belong, a path outside the
/// data directory, or a threshold outside 0 to 1.
pub fn validate(input: &ServerInput, root: &Path, home: Option<&Path>) -> Result<(), String> {
    let name = &input.name;
    let plain = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if name.is_empty() || name.len() > 64 || !plain || name.starts_with(['-', '_']) {
        return Err(format!(
            "name {name:?}: the name is the server's in the host and the domain of its sessions \
             and flows, so use a-z, 0-9, _ and -, starting with a letter or digit, at most 64"
        ));
    }
    if input
        .description
        .as_ref()
        .is_some_and(|d| d.chars().count() > 1000)
    {
        return Err("the description is longer than 1,000 characters".to_string());
    }
    match &input.upstream {
        Upstream::Stdio { command, env } => {
            if command.first().is_none_or(|c| c.trim().is_empty()) {
                return Err("the server's command is empty".to_string());
            }
            if command.len() > 256 || command.iter().any(|a| a.contains('\0') || a.len() > 4096) {
                return Err("the server's command has too many or too long arguments".to_string());
            }
            if let Some(i) = credential_argument(command) {
                return Err(format!(
                    "argument {} of the command looks like a credential: pass it to the server \
                     through an environment variable, and list the variable's name in env \
                     (the registry keeps names, never values)",
                    i + 1
                ));
            }
            for var in env {
                check_var(var)?;
            }
        }
        Upstream::Http { url, headers } => {
            check_url(url)?;
            for h in headers {
                let token = !h.name.is_empty()
                    && h.name.len() <= 128
                    && h.name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c));
                if !token {
                    return Err(format!("header {:?} is not a header name", h.name));
                }
                check_var(&h.env)?;
            }
        }
    }
    for (what, path) in [("flow", &input.flow), ("record_dir", &input.record_dir)] {
        if let Some(p) = path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            paths::resolve(root, home, p).map_err(|e| format!("{what}: {e}"))?;
        }
    }
    if let Some(t) = input.threshold {
        if !t.is_finite() || !(0.0..=1.0).contains(&t) {
            return Err(format!("threshold {t}: a probability, from 0 to 1"));
        }
    }
    Ok(())
}

/// An environment variable's name.
fn check_var(var: &str) -> Result<(), String> {
    let ok = !var.is_empty()
        && var.len() <= 128
        && !var.starts_with(|c: char| c.is_ascii_digit())
        && var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "{var:?} is not an environment variable's name (letters, digits and _): the registry \
             keeps variables by name, and their values stay in the environment"
        ))
    }
}

/// An HTTP server's URL, with no credential in it.
fn check_url(url: &str) -> Result<(), String> {
    let Some((scheme, rest)) = url.split_once("://") else {
        return Err(format!("url {url:?}: expected http:// or https://"));
    };
    if scheme != "http" && scheme != "https" {
        return Err(format!("url {url:?}: expected http:// or https://"));
    }
    if url.len() > 2048 || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("the url is too long or holds whitespace".to_string());
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() {
        return Err(format!("url {url:?} names no host"));
    }
    if authority.contains('@') {
        return Err(
            "the url holds a user or a password: send credentials in a header, from an \
             environment variable"
                .to_string(),
        );
    }
    let query = rest.split_once('?').map(|(_, q)| q).unwrap_or_default();
    let query = query.split('#').next().unwrap_or_default();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let name = pair.split('=').next().unwrap_or_default();
        if names_secret(name) && pair.contains('=') {
            return Err(format!(
                "the url's parameter {name:?} looks like a credential: send it in a header, \
                 from an environment variable"
            ));
        }
    }
    Ok(())
}

/// The index of the first argument that looks like a credential, as the
/// proxy's log header would redact it: the value after a flag that names a
/// key, token, secret, password or auth (`--api-key X`), a value assigned
/// to such a name (`--token=X`, `GITHUB_TOKEN=X`), or an authorization
/// header. Names of files and directories (`--key-file`) are not.
pub fn credential_argument(command: &[String]) -> Option<usize> {
    let mut after_flag = false;
    for (i, arg) in command.iter().enumerate() {
        let lower = arg.to_ascii_lowercase();
        if std::mem::take(&mut after_flag) && !arg.starts_with('-') {
            return Some(i);
        }
        if lower.starts_with("authorization:") || lower.contains("bearer ") {
            return Some(i);
        }
        match arg.split_once('=') {
            Some((name, value)) if names_secret(name) && !value.is_empty() => return Some(i),
            Some(_) => {}
            None => after_flag = arg.starts_with('-') && names_secret(arg),
        }
    }
    None
}

/// Whether a flag or variable name suggests its value is a credential, as
/// the proxy decides; a name for a file or directory never does.
fn names_secret(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let file = ["file", "path", "dir"]
        .iter()
        .any(|end| name.ends_with(end));
    !file
        && [
            "key",
            "token",
            "secret",
            "password",
            "passwd",
            "credential",
            "auth",
        ]
        .iter()
        .any(|word| name.contains(word))
}

/// Where `entry`'s flow is, if it names one.
pub fn flow_path(entry: &ServerEntry, root: &Path, home: Option<&Path>) -> Option<PathBuf> {
    paths::resolve(root, home, entry.flow.as_deref()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stdio(command: &[&str]) -> ServerInput {
        ServerInput {
            name: "shop".to_string(),
            description: None,
            upstream: Upstream::Stdio {
                command: command.iter().map(|s| s.to_string()).collect(),
                env: vec!["SHOP_KEY".to_string()],
            },
            mode: ServerMode::Record,
            flow: None,
            record_dir: None,
            decider: None,
            threshold: None,
        }
    }

    fn http(url: &str) -> ServerInput {
        ServerInput {
            upstream: Upstream::Http {
                url: url.to_string(),
                headers: vec![HeaderRef {
                    name: "Authorization".to_string(),
                    env: "SHOP_AUTH".to_string(),
                }],
            },
            ..stdio(&["x"])
        }
    }

    #[test]
    fn a_valid_entry_and_what_is_refused() {
        let root = Path::new("/data");
        let v = |i: &ServerInput| validate(i, root, Some(Path::new("/home/me")));
        assert!(v(&stdio(&["npx", "-y", "server", "--key-file", "/k"])).is_ok());
        assert!(v(&http("https://example.com/mcp?region=eu")).is_ok());
        for name in ["", "Shop", "a b", "-x", "_x", "a.b", &"x".repeat(65)] {
            let mut i = stdio(&["x"]);
            i.name = name.to_string();
            assert!(v(&i).is_err(), "{name:?}");
        }
        assert!(v(&stdio(&[])).is_err());
        assert!(v(&stdio(&["  "])).is_err());
        let mut bad_env = stdio(&["x"]);
        bad_env.upstream = Upstream::Stdio {
            command: vec!["x".into()],
            env: vec!["SHOP_KEY=secret".into()],
        };
        assert!(v(&bad_env)
            .unwrap_err()
            .contains("environment variable's name"));
        for url in [
            "ftp://x/mcp",
            "example.com/mcp",
            "https://user:pw@example.com/mcp",
            "https://example.com/mcp?api_key=abc",
            "https://",
            "https://exa mple.com",
        ] {
            assert!(v(&http(url)).is_err(), "{url}");
        }
        let mut bad_header = http("https://example.com/mcp");
        bad_header.upstream = Upstream::Http {
            url: "https://example.com/mcp".into(),
            headers: vec![HeaderRef {
                name: "Bad Header".into(),
                env: "X".into(),
            }],
        };
        assert!(v(&bad_header).is_err());
        let mut far = stdio(&["x"]);
        far.flow = Some("../elsewhere.flow.json".into());
        assert!(v(&far).unwrap_err().starts_with("flow:"));
        far.flow = Some("~/.stretto/shop.flow.json".into());
        assert!(v(&far).is_ok());
        far.threshold = Some(0.5);
        assert!(v(&far).is_ok());
        far.threshold = Some(1.5);
        assert!(v(&far).unwrap_err().contains("threshold"));
        let mut long = stdio(&["x"]);
        long.description = Some("x".repeat(1001));
        assert!(v(&long).unwrap_err().contains("longer than 1,000"));
        let args: Vec<String> = (0..257).map(|i| i.to_string()).collect();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        assert!(v(&stdio(&args))
            .unwrap_err()
            .contains("too many or too long"));
    }

    #[test]
    fn a_registry_that_cannot_be_read_says_why() {
        let root = std::env::temp_dir().join(format!(
            "stretto-console-registry-unread-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(FILE)).unwrap();
        let e = load(&root).unwrap_err();
        assert!(e.starts_with("reading servers.json: "), "{e}");
        std::fs::remove_dir(root.join(FILE)).unwrap();
        std::fs::write(root.join(FILE), r#"{"stretto_servers": 1, "servers": 5}"#).unwrap();
        let e = load(&root).unwrap_err();
        assert!(e.starts_with("servers.json: "), "{e}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_decider_is_named_as_the_flag_names_it() {
        use stretto_report::flow::Decider;
        for d in [Decider::Arbiter, Decider::Habit, Decider::Reach] {
            assert_eq!(DeciderName::of(d).decider(), d);
        }
    }

    #[test]
    fn credentials_are_refused_where_only_names_belong() {
        let at =
            |c: &[&str]| credential_argument(&c.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(at(&["server", "--api-key", "abc"]), Some(2));
        assert_eq!(at(&["server", "--token=abc"]), Some(1));
        assert_eq!(at(&["env", "GITHUB_TOKEN=ghp_x", "server"]), Some(1));
        assert_eq!(
            at(&["server", "--header", "Authorization: Bearer x"]),
            Some(2)
        );
        assert_eq!(at(&["server", "--api-key-file", "/run/key"]), None);
        assert_eq!(at(&["server", "--token", "--verbose"]), None);
        assert_eq!(
            at(&[
                "npx",
                "-y",
                "@modelcontextprotocol/server-filesystem",
                "/notes"
            ]),
            None
        );
        assert_eq!(at(&["server", "--token="]), None);
        let refused = validate(
            &stdio(&["server", "--api-key", "abc"]),
            Path::new("/data"),
            None,
        )
        .unwrap_err();
        assert!(
            refused.contains("argument 3") && !refused.contains("abc"),
            "{refused}"
        );
    }

    #[test]
    fn the_registry_is_written_whole_and_read_back() {
        let root =
            std::env::temp_dir().join(format!("stretto-console-registry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(load(&root).unwrap().servers, []);
        let registry = Registry {
            stretto_servers: VERSION,
            servers: vec![
                stdio(&["x"]).entry(1, 2),
                http("https://e.com/mcp").entry(3, 4),
            ],
        };
        save(&root, &registry).unwrap();
        assert_eq!(load(&root).unwrap(), registry);
        // Nothing is left beside it.
        let files: Vec<String> = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(files, [FILE]);
        let text = std::fs::read_to_string(root.join(FILE)).unwrap();
        assert!(text.contains("\"kind\": \"stdio\"") && text.contains("\"env\": \"SHOP_AUTH\""));
        assert!(text.ends_with("}\n"));
        std::fs::write(root.join(FILE), "{\"stretto_servers\": 2, \"servers\": []}").unwrap();
        assert!(load(&root).unwrap_err().contains("format 2"));
        std::fs::write(root.join(FILE), "not json").unwrap();
        assert!(load(&root).unwrap_err().contains("not a server registry"));
        // Under a file, there is nowhere to write it.
        let e = save(&root.join(FILE).join("data"), &registry).unwrap_err();
        assert!(e.starts_with("writing servers.json: "), "{e}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn blank_optional_fields_are_none() {
        let mut i = stdio(&["x"]);
        i.description = Some("  ".into());
        i.flow = Some("".into());
        let e = i.entry(1, 2);
        assert_eq!((e.description, e.flow), (None, None));
        assert_eq!((e.created_unix_ms, e.updated_unix_ms), (1, 2));
    }

    #[test]
    fn the_fixture_registry_reads() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/home");
        let r = load(&root).unwrap();
        let names: Vec<&str> = r.servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["shop", "orders"]);
        for s in &r.servers {
            let input = ServerInput {
                name: s.name.clone(),
                description: s.description.clone(),
                upstream: s.upstream.clone(),
                mode: s.mode,
                flow: s.flow.clone(),
                record_dir: s.record_dir.clone(),
                decider: s.decider,
                threshold: s.threshold,
            };
            validate(&input, &root, None).unwrap();
        }
    }
}
