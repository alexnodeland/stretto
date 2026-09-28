//! Keys: how the API names a session or a flow in a URL.
//!
//! A key is the file's stem (`20260928T014620.569Z-654`, `shop`), with each
//! character a URL would have to escape replaced by `_`. Where two files
//! share a stem, or a stem is a word a route takes for itself, each of them
//! gets `~` and the first 8 hex digits of the SHA-256 of its path, relative
//! to the data directory, appended. A plain key never holds `~`, so the two
//! kinds cannot meet, and keys stay the same across restarts as long as the
//! files do.

use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// The keys of `items`, each `(stem, path relative to the data directory)`,
/// in order. A stem in `reserved` always gets its hash.
pub fn assign(items: &[(String, String)], reserved: &[&str]) -> Vec<String> {
    let bases: Vec<String> = items.iter().map(|(stem, _)| base(stem)).collect();
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for b in &bases {
        *seen.entry(b.as_str()).or_default() += 1;
    }
    let mut keys: Vec<String> = bases
        .iter()
        .zip(items)
        .map(|(b, (_, rel))| {
            if seen[b.as_str()] > 1 || reserved.contains(&b.as_str()) {
                format!("{b}~{}", &hash(rel)[..8])
            } else {
                b.clone()
            }
        })
        .collect();
    // Eight hex digits of two paths may still agree; then the whole hash.
    let mut taken: HashMap<String, usize> = HashMap::new();
    for k in &keys {
        *taken.entry(k.clone()).or_default() += 1;
    }
    let clashes: HashSet<String> = taken
        .into_iter()
        .filter(|(_, n)| *n > 1)
        .map(|(k, _)| k)
        .collect();
    for (k, (b, (_, rel))) in keys.iter_mut().zip(bases.iter().zip(items)) {
        if clashes.contains(k) {
            *k = format!("{b}~{}", hash(rel));
        }
    }
    keys
}

/// The stem with every character a URL path segment would escape replaced
/// by `_`; a stem of dots alone, which a path would read as `.` or `..`,
/// becomes `_` too.
fn base(stem: &str) -> String {
    let b: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    if b.is_empty() || b.chars().all(|c| c == '.') {
        "_".to_string()
    } else {
        b
    }
}

fn hash(rel: &str) -> String {
    hex::encode(Sha256::digest(rel.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(s, r)| (s.to_string(), r.to_string()))
            .collect()
    }

    #[test]
    fn a_key_is_the_stem_unless_it_is_shared() {
        let keys = assign(
            &items(&[
                (
                    "20260928T014620.569Z-654",
                    "logs/shop/20260928T014620.569Z-654.jsonl",
                ),
                ("shop", "shop.flow.json"),
                ("shop", "old/shop.flow.json"),
            ]),
            &[],
        );
        assert_eq!(keys[0], "20260928T014620.569Z-654");
        // Each shared stem gets its own path's hash.
        assert_eq!(keys[1], format!("shop~{}", &hash("shop.flow.json")[..8]));
        assert_eq!(
            keys[2],
            format!("shop~{}", &hash("old/shop.flow.json")[..8])
        );
        assert_ne!(keys[1], keys[2]);
        assert_eq!(keys[1].len(), "shop~".len() + 8);
    }

    #[test]
    fn keys_are_url_safe() {
        let keys = assign(
            &items(&[
                ("my flow#1", "my flow#1.flow.json"),
                ("..", "...flow.json"),
                ("", ".jsonl"),
                ("é", "é.jsonl"),
                ("a/b", "x"),
            ]),
            &[],
        );
        assert_eq!(keys[0], "my_flow_1");
        // `..`, the empty stem and `é` are all `_`, so each gets its hash.
        assert_eq!(keys[1], "_~".to_string() + &hash("...flow.json")[..8]);
        assert_eq!(keys[2], "_~".to_string() + &hash(".jsonl")[..8]);
        assert_eq!(keys[3], "_~".to_string() + &hash("é.jsonl")[..8]);
        assert_eq!(keys[4], "a_b");
        for k in &keys {
            assert!(
                k.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-~".contains(c)),
                "{k}"
            );
        }
    }

    #[test]
    fn a_reserved_stem_gets_its_hash_and_a_sanitized_clash_does_too() {
        let keys = assign(&items(&[("diff", "diff.flow.json")]), &["diff"]);
        assert!(keys[0].starts_with("diff~"));
        // `a b` and `a_b` meet once sanitized.
        let keys = assign(&items(&[("a b", "a b.jsonl"), ("a_b", "a_b.jsonl")]), &[]);
        assert!(keys[0].starts_with("a_b~") && keys[1].starts_with("a_b~"));
        assert_ne!(keys[0], keys[1]);
        // A plain key never has `~`, so it cannot be another's hashed key.
        let keys = assign(&items(&[("x~12345678", "x~12345678.jsonl")]), &[]);
        assert_eq!(keys[0], "x_12345678");
    }
}
