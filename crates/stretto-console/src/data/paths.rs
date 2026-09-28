//! Paths, kept inside the data directory.
//!
//! A path a request names ([`resolve`]) is relative to the data directory,
//! or starts with `~/` for one in the home directory, or is absolute inside
//! the data directory. `..` may not climb out of either, and a symbolic link
//! may not lead out: the deepest part of the path that exists must resolve,
//! links followed, inside the directory it started from. The scan ([`keep`])
//! follows a link only where it stays inside the data directory.

use std::path::{Component, Path, PathBuf};

/// Whether the scan of `root` (whose canonical form is `canonical`) takes
/// `path`: not the console's own directory, and not a symbolic link that
/// resolves outside the data directory, or not at all.
pub fn keep(root: &Path, canonical: &Path, path: &Path) -> bool {
    if path.parent() == Some(root) && path.file_name().is_some_and(|n| n == super::CONSOLE_DIR) {
        return false;
    }
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => {
            std::fs::canonicalize(path).is_ok_and(|target| target.starts_with(canonical))
        }
        Ok(_) => true,
        Err(_) => false,
    }
}

/// `path` relative to `root`, with `/` between its parts; the whole path
/// when it is not under `root`.
pub fn rel(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rest) => rest
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.display().to_string(),
    }
}

/// `path` as a person would type it: `~/…` under `home`, else as it is.
pub fn display(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rel(Path::new(""), rest)),
        None => path.display().to_string(),
    }
}

/// Resolve a path a request names: relative to `data_dir`, `~/…` inside
/// `home`, or absolute inside `data_dir`. The error says why not, without
/// saying what is there.
pub fn resolve(data_dir: &Path, home: Option<&Path>, input: &str) -> Result<PathBuf, String> {
    if input.trim().is_empty() {
        return Err("the path is empty".to_string());
    }
    if input.contains('\0') {
        return Err("the path holds a NUL character".to_string());
    }
    let (base, rest) = if input == "~" || input.starts_with("~/") {
        let Some(home) = home else {
            return Err(format!(
                "{input}: there is no home directory to resolve ~ in"
            ));
        };
        (home, input.trim_start_matches('~').trim_start_matches('/'))
    } else if input.starts_with('~') {
        return Err(format!(
            "{input}: only ~/ (this user's home directory) is expanded"
        ));
    } else if Path::new(input).is_absolute() {
        let normal = normalize(Path::new(input))
            .ok_or_else(|| format!("{input}: .. climbs out of the root"))?;
        let rest = normal
            .strip_prefix(data_dir)
            .map_err(|_| {
                format!(
                    "{input} is outside the data directory ({}): name a path inside it, or \
                     one that starts with ~/",
                    data_dir.display()
                )
            })?
            .to_path_buf();
        return inside(data_dir, &rest, input);
    } else {
        (data_dir, input)
    };
    inside(base, Path::new(rest), input)
}

/// `base` joined with `rest`, which must stay inside `base`, links followed.
fn inside(base: &Path, rest: &Path, input: &str) -> Result<PathBuf, String> {
    if rest.is_absolute() {
        return Err(format!("{input}: expected a relative path"));
    }
    let joined =
        normalize(&base.join(rest)).ok_or_else(|| format!("{input}: .. climbs out of the root"))?;
    if !joined.starts_with(base) {
        return Err(format!("{input}: .. climbs out of {}", base.display()));
    }
    // The deepest part that exists, with its links followed, must still be
    // inside the base.
    let canonical_base = std::fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    let mut existing = joined.as_path();
    while existing.starts_with(base) {
        if std::fs::symlink_metadata(existing).is_ok() {
            match std::fs::canonicalize(existing) {
                Ok(real) if real.starts_with(&canonical_base) => break,
                Ok(_) => {
                    return Err(format!(
                        "{input}: a symbolic link leads out of {}",
                        base.display()
                    ))
                }
                // A dangling link: where it leads is unknown, so no.
                Err(_) => {
                    return Err(format!("{input}: a symbolic link leads nowhere"));
                }
            }
        }
        match existing.parent() {
            Some(parent) => existing = parent,
            None => break,
        }
    }
    Ok(joined)
}

/// `path` with `.` dropped and each `..` taking the part before it away;
/// `None` if a `..` would climb above the root.
pub fn normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "stretto-console-paths-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::canonicalize(&dir).unwrap()
    }

    #[test]
    fn a_path_stays_in_the_data_directory_or_the_home_directory() {
        let root = temp("resolve");
        let data = root.join("data");
        let home = root.join("home");
        std::fs::create_dir_all(data.join("logs/shop")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let r = |p: &str| resolve(&data, Some(&home), p);
        assert_eq!(r("logs/shop").unwrap(), data.join("logs/shop"));
        assert_eq!(r("./logs/../logs/shop/").unwrap(), data.join("logs/shop"));
        assert_eq!(
            r("new/dir/f.flow.json").unwrap(),
            data.join("new/dir/f.flow.json")
        );
        assert_eq!(r("~/x/y").unwrap(), home.join("x/y"));
        assert_eq!(r("~").unwrap(), home);
        assert_eq!(
            r(&data.join("logs").display().to_string()).unwrap(),
            data.join("logs")
        );
        for bad in [
            "",
            "  ",
            "../outside",
            "logs/../../outside",
            "~/../etc",
            "~root/x",
            "/etc/passwd",
            "a\0b",
        ] {
            assert!(r(bad).is_err(), "{bad:?}");
        }
        let outside = r("/etc/passwd").unwrap_err();
        assert!(outside.contains("outside the data directory"), "{outside}");
        assert!(resolve(&data, None, "~/x").is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_may_not_lead_out() {
        let root = temp("links");
        let data = root.join("data");
        std::fs::create_dir_all(data.join("logs")).unwrap();
        std::fs::create_dir_all(root.join("secret")).unwrap();
        std::os::unix::fs::symlink(root.join("secret"), data.join("out")).unwrap();
        std::os::unix::fs::symlink(data.join("logs"), data.join("in")).unwrap();
        std::os::unix::fs::symlink(root.join("gone"), data.join("dangling")).unwrap();
        let r = |p: &str| resolve(&data, None, p);
        assert!(r("out").unwrap_err().contains("symbolic link"));
        assert!(r("out/new/f.json").unwrap_err().contains("symbolic link"));
        assert!(r("dangling/x").is_err());
        assert_eq!(r("in/x").unwrap(), data.join("in/x"));
        // The scan does not take the link out either.
        assert!(!keep(&data, &data, &data.join("out")));
        assert!(keep(&data, &data, &data.join("in")));
        assert!(!keep(&data, &data, &data.join("dangling")));
        assert!(!keep(&data, &data, &data.join("console")));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn paths_are_shown_from_the_data_and_home_directories() {
        let root = Path::new("/home/me/.stretto");
        assert_eq!(
            rel(root, &root.join("logs/shop/s.jsonl")),
            "logs/shop/s.jsonl"
        );
        assert_eq!(rel(root, Path::new("/elsewhere/f")), "/elsewhere/f");
        let home = Path::new("/home/me");
        assert_eq!(
            display(&root.join("shop.flow.json"), Some(home)),
            "~/.stretto/shop.flow.json"
        );
        assert_eq!(display(Path::new("/srv/x"), Some(home)), "/srv/x");
        assert_eq!(
            normalize(Path::new("/a/./b/../c")),
            Some(PathBuf::from("/a/c"))
        );
        assert_eq!(normalize(Path::new("/..")), None);
    }
}
