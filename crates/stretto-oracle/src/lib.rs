//! The System-One oracle interface.
//!
//! An [`Oracle`] takes a [`Request`] (state plus typed questions) and returns
//! typed [`Answer`]s with probabilities. The wire format is TypeSafe's
//! `POST /v1/systemone`, so [`jev::JevClient`] is a thin HTTP client, and any
//! other oracle (a mock, a small local classifier, an LLM restricted to fixed
//! choices) implements the same trait.
//!
//! [`ReplayCache`] stores every answer on disk, keyed by a hash of the exact
//! request. Experiments therefore pay for each distinct question once, and
//! re-running them is deterministic and free.

#[cfg(feature = "http")]
pub mod jev;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A typed question.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Question {
    /// Yes/no; the answer is the probability of "yes".
    Noul {
        instructions: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        criteria: Option<NoulCriteria>,
    },
    /// Pick one option; `criteria` maps each option to its rubric (≤ 255).
    Choice {
        instructions: String,
        criteria: BTreeMap<String, String>,
    },
    /// Rate against 2–10 ordered level descriptions.
    Score {
        instructions: String,
        criteria: Vec<String>,
    },
}

/// What "true" and "false" mean for a [`Question::Noul`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoulCriteria {
    /// Description of the "yes" case.
    #[serde(rename = "true")]
    pub yes: String,
    /// Description of the "no" case.
    #[serde(rename = "false")]
    pub no: String,
}

/// State plus questions, evaluated together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// Model id, e.g. `jev-latest`.
    pub model: String,
    /// What the questions are about: a string, object or array.
    pub state: serde_json::Value,
    /// Questions by id.
    pub questions: BTreeMap<String, Question>,
}

/// A typed answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Answer {
    /// Probability that the statement is true.
    Noul { noul: f64 },
    /// The chosen option, the distribution over options, and a confidence.
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
    /// The probability-weighted mean level, the distribution, and a confidence.
    Score {
        score: f64,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
        #[serde(default)]
        legend: serde_json::Value,
    },
}

/// Token usage reported by the service.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Input tokens billed.
    pub input_tokens: u64,
    /// Output tokens (not billed by TypeSafe).
    pub output_tokens: u64,
}

/// Answers to a [`Request`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// The exact model version that answered.
    pub model: String,
    /// Answers by question id.
    pub answers: BTreeMap<String, Answer>,
    /// Token usage.
    #[serde(default)]
    pub usage: Usage,
}

/// Anything that answers typed questions about a state.
pub trait Oracle {
    /// Answer every question in `request`.
    fn ask(&self, request: &Request) -> Result<Response>;
}

/// Hex SHA-256 of the request's canonical JSON. Maps are ordered, so equal
/// requests hash equally.
pub fn request_key(request: &Request) -> String {
    let bytes = serde_json::to_vec(request).expect("requests always serialize");
    hex::encode(Sha256::digest(bytes))
}

/// An on-disk, content-addressed cache in front of another oracle.
///
/// With no inner oracle it is replay-only: a cache miss is an error, which is
/// what reproducing a published experiment needs.
pub struct ReplayCache<O> {
    dir: PathBuf,
    inner: Option<O>,
}

impl<O: Oracle> ReplayCache<O> {
    /// Cache in `dir`, asking `inner` on a miss.
    pub fn new(dir: impl Into<PathBuf>, inner: Option<O>) -> Self {
        Self {
            dir: dir.into(),
            inner,
        }
    }

    fn read(path: &Path) -> Result<Option<Response>> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Some(serde_json::from_str(&text)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

impl<O> ReplayCache<O> {
    /// Every cached `(key, response)`, sorted by key. Responses carry only
    /// the oracle's answers and usage, never the request's state.
    pub fn entries(&self) -> Result<Vec<(String, Response)>> {
        let mut out = Vec::new();
        let shards = match std::fs::read_dir(&self.dir) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        for shard in shards {
            let shard = shard?.path();
            if !shard.is_dir() {
                continue;
            }
            for file in std::fs::read_dir(&shard)? {
                let path = file?.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let key = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading {}", path.display()))?;
                out.push((key, serde_json::from_str(&text)?));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    /// Store `response` under `key`, as if it had been asked.
    pub fn insert(&self, key: &str, response: &Response) -> Result<()> {
        if key.len() < 3 || !key.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("not a request key: {key:?}");
        }
        let path = self.path(key);
        let parent = path.parent().expect("cache paths have a parent");
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
        static WRITES: AtomicU64 = AtomicU64::new(0);
        let n = WRITES.fetch_add(1, Ordering::Relaxed);
        let tmp = path.with_extension(format!("tmp{}-{n}", std::process::id()));
        std::fs::write(&tmp, serde_json::to_vec_pretty(response)?)
            .with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir.join(&key[..2]).join(format!("{key}.json"))
    }
}

impl<O: Oracle> Oracle for ReplayCache<O> {
    fn ask(&self, request: &Request) -> Result<Response> {
        let key = request_key(request);
        let path = self.path(&key);
        if let Some(hit) = Self::read(&path)? {
            return Ok(hit);
        }
        let Some(inner) = &self.inner else {
            bail!("replay cache miss for request {key}");
        };
        let response = inner.ask(request)?;
        // Written then renamed, so a concurrent reader never sees half a file.
        self.insert(&key, &response)?;
        Ok(response)
    }
}

/// A deterministic oracle for tests and dry runs: every `Choice` picks its
/// first option with probability `confidence`, every `Noul` returns `noul`,
/// every `Score` returns the lowest level.
pub struct MockOracle {
    /// Probability put on the first option of every `Choice`.
    pub confidence: f64,
    /// Probability returned for every `Noul`.
    pub noul: f64,
}

impl Oracle for MockOracle {
    fn ask(&self, request: &Request) -> Result<Response> {
        let answers = request
            .questions
            .iter()
            .map(|(id, q)| {
                let a = match q {
                    Question::Noul { .. } => Answer::Noul { noul: self.noul },
                    Question::Choice { criteria, .. } => {
                        let rest = if criteria.len() > 1 {
                            (1.0 - self.confidence) / (criteria.len() - 1) as f64
                        } else {
                            0.0
                        };
                        let first = criteria.keys().next().cloned().unwrap_or_default();
                        let probabilities = criteria
                            .keys()
                            .map(|k| (k.clone(), if *k == first { self.confidence } else { rest }))
                            .collect();
                        Answer::Choice {
                            choice: first,
                            probabilities,
                            confidence: self.confidence,
                        }
                    }
                    Question::Score { criteria, .. } => Answer::Score {
                        score: 0.0,
                        probabilities: criteria
                            .iter()
                            .enumerate()
                            .map(|(i, _)| (i.to_string(), if i == 0 { 1.0 } else { 0.0 }))
                            .collect(),
                        confidence: 1.0,
                        legend: serde_json::Value::Null,
                    },
                };
                (id.clone(), a)
            })
            .collect();
        Ok(Response {
            model: "mock".to_string(),
            answers,
            usage: Usage::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn documented_example_round_trips() {
        // The request and response from TypeSafe's API reference.
        let req: Request = serde_json::from_str(
            r#"{
              "state": "Help! My payouts have been failing for 3 days.",
              "model": "jev-latest",
              "questions": {
                "is_urgent": {
                  "type": "noul",
                  "instructions": "Does this convey urgency?",
                  "criteria": {"true": "Explicitly time-sensitive", "false": "No urgency expressed"}
                }
              }
            }"#,
        )
        .unwrap();
        assert!(matches!(req.questions["is_urgent"], Question::Noul { .. }));
        let back: Request = serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        assert_eq!(back, req);

        let resp: Response = serde_json::from_str(
            r#"{
              "model": "jev-1.13.0",
              "answers": {"is_urgent": {"type": "noul", "noul": 0.95}},
              "usage": {"input_tokens": 296, "output_tokens": 20}
            }"#,
        )
        .unwrap();
        assert_eq!(resp.answers["is_urgent"], Answer::Noul { noul: 0.95 });
        assert_eq!(resp.usage.input_tokens, 296);
    }

    #[test]
    fn choice_and_score_answers_parse() {
        let resp: Response = serde_json::from_str(
            r#"{
              "model": "jev-1.13.0",
              "answers": {
                "dept": {"type": "choice", "choice": "billing",
                         "probabilities": {"billing": 0.9, "sales": 0.1}, "confidence": 0.8},
                "sev": {"type": "score", "score": 1.2, "legend": {"0": "Cosmetic"},
                        "probabilities": {"0": 0.1, "1": 0.6, "2": 0.3}, "confidence": 0.5}
              }
            }"#,
        )
        .unwrap();
        match &resp.answers["dept"] {
            Answer::Choice { choice, .. } => assert_eq!(choice, "billing"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(resp.answers["sev"], Answer::Score { .. }));
        assert_eq!(resp.usage, Usage::default());
    }

    struct Counting {
        calls: Cell<usize>,
    }

    impl Oracle for Counting {
        fn ask(&self, request: &Request) -> Result<Response> {
            self.calls.set(self.calls.get() + 1);
            MockOracle {
                confidence: 0.7,
                noul: 0.5,
            }
            .ask(request)
        }
    }

    #[test]
    fn replay_cache_pays_once_and_replays_offline() {
        let dir = std::env::temp_dir().join(format!("stretto-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let req = Request {
            model: "jev-latest".into(),
            state: serde_json::json!({"ticket": "refund please"}),
            questions: BTreeMap::from([(
                "route".to_string(),
                Question::Choice {
                    instructions: "Which team?".into(),
                    criteria: BTreeMap::from([
                        ("billing".to_string(), "Money".to_string()),
                        ("tech".to_string(), "Bugs".to_string()),
                    ]),
                },
            )]),
        };

        let live = ReplayCache::new(
            &dir,
            Some(Counting {
                calls: Cell::new(0),
            }),
        );
        let first = live.ask(&req).unwrap();
        let second = live.ask(&req).unwrap();
        assert_eq!(first, second);
        assert_eq!(live.inner.as_ref().unwrap().calls.get(), 1);

        let offline: ReplayCache<MockOracle> = ReplayCache::new(&dir, None);
        assert_eq!(offline.ask(&req).unwrap(), first);
        let mut other = req.clone();
        other.state = serde_json::json!("something else");
        assert!(offline.ask(&other).is_err());

        // Entries export the answers alone, and import into another cache.
        let entries = offline.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, request_key(&req));
        let copy_dir = dir.with_extension("copy");
        let _ = std::fs::remove_dir_all(&copy_dir);
        let copy: ReplayCache<MockOracle> = ReplayCache::new(&copy_dir, None);
        copy.insert(&entries[0].0, &entries[0].1).unwrap();
        assert_eq!(copy.ask(&req).unwrap(), first);
        assert!(copy.insert("../escape", &first).is_err());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&copy_dir);
    }
}
