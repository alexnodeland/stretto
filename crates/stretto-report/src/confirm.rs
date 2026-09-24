//! Judging a customer's confirmation with a System-One model.
//!
//! τ²-bench's policies ask the agent to describe a change and get the
//! customer's explicit "yes" before making it. The guards check that with a
//! word list (`retail.confirmed`, `airline.confirmed`), which misses about as
//! often in successful episodes as in failed ones, so it is only logged (see
//! [`crate::guards`]). Here the System-One model is asked one yes/no question
//! per write instead. It sees what the agent last said before the customer's
//! last message, that message, and the call about to be made, and is asked
//! whether the customer explicitly agreed to this change. The audit sorts
//! its answers the way the guard audit sorts verdicts, next to the word
//! list's: accepted writes in successful and in failed episodes, and writes
//! the tool refused.

use crate::guards::{Guards, Verdict};
use crate::shadow::{self, Decision, Kind, ShadowConfig};
use anyhow::Result;
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use stretto_oracle::{request_key, Answer, NoulCriteria, Oracle, Question, Request};
use stretto_trace::{Episode, Event};

/// The question's id.
const QUESTION: &str = "confirmed";
/// Characters kept from the end of the agent's last message.
const MAX_AGENT: usize = 1500;
/// Characters kept from the start of the customer's last message.
const MAX_CUSTOMER: usize = 600;
/// Characters of the customer's message shown in an example.
const SHOWN: usize = 160;

/// One write, judged two ways.
#[derive(Clone, Debug, Serialize)]
pub struct Judged {
    /// The τ²-bench task.
    pub task_id: String,
    /// The episode.
    pub episode: String,
    /// The write.
    pub tool: String,
    /// Whether the episode passed τ²-bench's check.
    pub success: bool,
    /// Whether the tool refused the write.
    pub refused: bool,
    /// Whether the word list passes it: the customer's last message has a
    /// confirming word.
    pub word_list: bool,
    /// The System-One model's probability that the customer explicitly
    /// confirmed this change.
    pub p_yes: Option<f64>,
    /// The customer's last message.
    pub customer: String,
    /// The question's key in the replay cache.
    pub key: String,
    #[serde(skip)]
    request: Request,
}

/// Every write in `episodes` that `guards` checks for a confirmation, with
/// the question to ask `model` about it.
pub fn writes(guards: &Guards, episodes: &[&Episode], model: &str) -> Vec<Judged> {
    let mut out = Vec::new();
    for ep in episodes {
        let refused: HashMap<&str, bool> = ep
            .events
            .iter()
            .filter_map(|e| match e {
                Event::ToolResult { call_id, error, .. } => Some((call_id.as_str(), *error)),
                _ => None,
            })
            .collect();
        // What the agent said last, and what it had said when the customer
        // last spoke: the proposal the customer answered.
        let (mut agent_last, mut proposal, mut customer) =
            (String::new(), String::new(), String::new());
        for (i, e) in ep.events.iter().enumerate() {
            match e {
                Event::User { text } => {
                    proposal = agent_last.clone();
                    customer = text.clone();
                }
                Event::Assistant { text, calls, .. } => {
                    let before = Episode {
                        events: ep.events[..i].to_vec(),
                        ..(*ep).clone()
                    };
                    for call in calls {
                        let Some(word_list) = guards
                            .check(&before, call)
                            .into_iter()
                            .find(|(id, _)| id.ends_with(".confirmed"))
                            .map(|(_, v)| matches!(v, Verdict::Pass))
                        else {
                            continue;
                        };
                        let request = Request {
                            model: model.to_string(),
                            state: json!({
                                "agent_said_last": tail(&proposal, MAX_AGENT),
                                "customer_replied": head(&customer, MAX_CUSTOMER),
                                "call_about_to_be_made": {
                                    "tool": call.name,
                                    "arguments": call.arguments,
                                },
                            }),
                            questions: BTreeMap::from([(QUESTION.to_string(), question())]),
                        };
                        out.push(Judged {
                            key: request_key(&request),
                            task_id: ep.task_id.clone(),
                            episode: ep.id.clone(),
                            tool: call.name.clone(),
                            success: ep.succeeded(),
                            refused: refused.get(call.id.as_str()).copied().unwrap_or(false),
                            word_list,
                            p_yes: None,
                            customer: customer.clone(),
                            request,
                        });
                    }
                    if let Some(t) = text.as_deref().filter(|t| !t.trim().is_empty()) {
                        agent_last = t.to_string();
                    }
                }
                Event::ToolResult { .. } => {}
            }
        }
    }
    out
}

/// The yes/no question asked about every write.
fn question() -> Question {
    Question::Noul {
        instructions: "An agent in a customer-service chat is about to make the change in \
                       call_about_to_be_made. The policy requires the agent to describe a change \
                       and get the customer's explicit yes before making it. Did the customer's \
                       reply explicitly agree to this change?"
            .to_string(),
        criteria: Some(NoulCriteria {
            yes: "The reply clearly agrees to go ahead with this change as the agent described \
                  it, for example 'yes', 'go ahead' or 'please do'."
                .to_string(),
            no: "The reply does not agree to this change: it asks a question, gives \
                 information, declines, agrees to something different, or the agent never \
                 described this change."
                .to_string(),
        }),
    }
}

fn tail(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    chars[chars.len().saturating_sub(n)..].iter().collect()
}

fn head(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Ask `oracle` about every write in `items` (each distinct question once),
/// and fill in its answers.
pub fn judge(
    oracle: &(dyn Oracle + Sync),
    items: &mut [Judged],
    config: &ShadowConfig,
    domain: &str,
) -> Result<()> {
    let decisions: Vec<Decision> = items
        .iter()
        .enumerate()
        .map(|(i, j)| Decision {
            episode: i,
            step: 0,
            kind: Kind::Next,
            tool: None,
            actual: String::new(),
            agent: String::new(),
            request: Some(j.request.clone()),
            fixed: None,
        })
        .collect();
    let asked = shadow::ask(oracle, &decisions, config, domain)?;
    for j in items.iter_mut() {
        j.p_yes = asked.responses.get(&request_key(&j.request)).and_then(|r| {
            match r.answers.get(QUESTION) {
                Some(Answer::Noul { noul }) => Some(*noul),
                _ => None,
            }
        });
    }
    Ok(())
}

/// Counts for one group of writes.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Counts {
    /// Writes.
    pub writes: usize,
    /// Writes the word list fails.
    pub word_list: usize,
    /// Writes the System-One judge fails.
    pub judge: usize,
    /// Writes both fail.
    pub both: usize,
    /// Writes the judge did not answer.
    pub unanswered: usize,
}

/// A write the two judges disagree on.
#[derive(Clone, Debug, Serialize)]
pub struct Example {
    /// The τ²-bench task.
    pub task_id: String,
    /// The write.
    pub tool: String,
    /// The judge's probability of an explicit yes.
    pub p_yes: f64,
    /// Whether the episode passed.
    pub success: bool,
    /// The start of the customer's last message.
    pub customer: String,
}

/// The confirmation audit of one domain.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ConfirmAudit {
    /// The domain.
    pub domain: String,
    /// The judge fails a write below this probability of an explicit yes.
    pub threshold: f64,
    /// Episodes: successful, failed.
    pub episodes: [usize; 2],
    /// Accepted writes in successful episodes, in failed ones, and writes
    /// the tool refused.
    pub groups: [Counts; 3],
    /// Episodes (successful, failed) with an accepted write the word list
    /// fails.
    pub episodes_word_list: [usize; 2],
    /// Episodes (successful, failed) with an accepted write the judge fails.
    pub episodes_judge: [usize; 2],
    /// Accepted writes the judge fails and the word list passes.
    pub judge_only: Vec<Example>,
    /// Accepted writes the word list fails and the judge passes.
    pub word_list_only: Vec<Example>,
}

/// Sort `items`, all of `domain`, into the audit; show up to `examples` of
/// each disagreement.
pub fn audit(
    domain: &str,
    items: &[Judged],
    episodes: [usize; 2],
    threshold: f64,
    examples: usize,
) -> ConfirmAudit {
    let mut a = ConfirmAudit {
        domain: domain.to_string(),
        threshold,
        episodes,
        ..Default::default()
    };
    let mut flagged: HashMap<&str, [bool; 2]> = HashMap::new();
    for j in items {
        let g = if j.refused {
            2
        } else if j.success {
            0
        } else {
            1
        };
        let judge_fails = j.p_yes.is_some_and(|p| p < threshold);
        let c = &mut a.groups[g];
        c.writes += 1;
        c.word_list += usize::from(!j.word_list);
        c.judge += usize::from(judge_fails);
        c.both += usize::from(!j.word_list && judge_fails);
        c.unanswered += usize::from(j.p_yes.is_none());
        if j.refused {
            continue;
        }
        let e = flagged.entry(j.episode.as_str()).or_insert([false; 2]);
        e[0] |= !j.word_list;
        e[1] |= judge_fails;
        let example = || Example {
            task_id: j.task_id.clone(),
            tool: j.tool.clone(),
            p_yes: j.p_yes.unwrap_or(f64::NAN),
            success: j.success,
            customer: head(&j.customer.replace('\n', " "), SHOWN),
        };
        if judge_fails && j.word_list && a.judge_only.len() < examples {
            a.judge_only.push(example());
        }
        if !j.word_list
            && j.p_yes.is_some_and(|p| p >= threshold)
            && a.word_list_only.len() < examples
        {
            a.word_list_only.push(example());
        }
    }
    let success: HashMap<&str, bool> = items
        .iter()
        .map(|j| (j.episode.as_str(), j.success))
        .collect();
    for (ep, [word, judge]) in &flagged {
        let side = usize::from(!success[ep]);
        a.episodes_word_list[side] += usize::from(*word);
        a.episodes_judge[side] += usize::from(*judge);
    }
    a
}

/// The audits as a report.
pub fn markdown(audits: &[ConfirmAudit]) -> String {
    let mut s = String::from("# Confirmation judged by a System-One model\n\n");
    let _ = writeln!(
        s,
        "Every write that τ²-bench's policy says needs the customer's explicit yes is judged two \
         ways: by the guards' word list (the customer's last message has a confirming word), and \
         by a System-One model asked whether the customer explicitly agreed to this change, given \
         what the agent said before the customer's last message, that message, and the call. The \
         judge fails a write when its probability of a yes is below the threshold. A failure on \
         an accepted write of a successful episode is a false alarm, or a confirmation the agent \
         skipped and the database check did not catch.\n"
    );
    let pct = |n: usize, d: usize| {
        if d == 0 {
            "–".to_string()
        } else {
            format!("{n} ({:.1}%)", 100.0 * n as f64 / d as f64)
        }
    };
    for a in audits {
        let _ = writeln!(s, "## {}\n", a.domain);
        let _ = writeln!(
            s,
            "{} episodes ({} successful, {} failed). The judge fails a write below P(yes) = {}.\n",
            a.episodes[0] + a.episodes[1],
            a.episodes[0],
            a.episodes[1],
            a.threshold
        );
        let _ = writeln!(
            s,
            "| Writes | Checked | Word list fails | Judge fails | Both fail | Unanswered |"
        );
        let _ = writeln!(s, "|---|---|---|---|---|---|");
        for (name, c) in [
            "Accepted, successful episodes",
            "Accepted, failed episodes",
            "Refused by the tool",
        ]
        .iter()
        .zip(&a.groups)
        {
            let _ = writeln!(
                s,
                "| {name} | {} | {} | {} | {} | {} |",
                c.writes,
                pct(c.word_list, c.writes),
                pct(c.judge, c.writes),
                pct(c.both, c.writes),
                c.unanswered
            );
        }
        let _ = writeln!(
            s,
            "\nEpisodes with an accepted write that fails: the word list, {} successful and {} \
             failed; the judge, {} successful and {} failed.\n",
            pct(a.episodes_word_list[0], a.episodes[0]),
            pct(a.episodes_word_list[1], a.episodes[1]),
            pct(a.episodes_judge[0], a.episodes[0]),
            pct(a.episodes_judge[1], a.episodes[1]),
        );
        for (title, list) in [
            ("The judge fails, the word list passes", &a.judge_only),
            ("The word list fails, the judge passes", &a.word_list_only),
        ] {
            if list.is_empty() {
                continue;
            }
            let _ = writeln!(s, "{title} (accepted writes, up to {}):\n", list.len());
            for e in list {
                let _ = writeln!(
                    s,
                    "- task {}, `{}`, P(yes) = {:.2}, {} episode: \"{}\"",
                    e.task_id,
                    e.tool,
                    e.p_yes,
                    if e.success { "successful" } else { "failed" },
                    e.customer.replace('"', "'")
                );
            }
            s.push('\n');
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use stretto_trace::ToolCall;

    fn episode(customer_last: &str) -> Episode {
        let call = |id: &str, name: &str, args: serde_json::Value| Event::Assistant {
            text: None,
            calls: vec![ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                arguments: args,
            }],
            usage: None,
        };
        let result = |id: &str, name: &str, content: &str| Event::ToolResult {
            call_id: id.to_string(),
            name: name.to_string(),
            error: false,
            content: content.to_string(),
        };
        Episode {
            id: "e1".to_string(),
            task_id: "7".to_string(),
            trial: 0,
            domain: "retail".to_string(),
            agent_model: "m".to_string(),
            reward: 1.0,
            events: vec![
                Event::User {
                    text: "I'm ann@x.com, cancel my order #W1".to_string(),
                },
                call("a", "find_user_id_by_email", json!({"email": "ann@x.com"})),
                result("a", "find_user_id_by_email", "ann_1"),
                Event::Assistant {
                    text: Some(
                        "I'll cancel order #W1 because you no longer need it. Shall I go ahead?"
                            .to_string(),
                    ),
                    calls: vec![],
                    usage: None,
                },
                Event::User {
                    text: customer_last.to_string(),
                },
                call(
                    "b",
                    "cancel_pending_order",
                    json!({"order_id": "#W1", "reason": "no longer needed"}),
                ),
                result("b", "cancel_pending_order", "{\"status\": \"cancelled\"}"),
            ],
        }
    }

    #[test]
    fn each_write_is_put_with_the_proposal_it_answers() {
        let guards = Guards::for_domain("retail").unwrap();
        let ep = episode("Yes, please.");
        let items = writes(&guards, &[&ep], "jev-test");
        assert_eq!(items.len(), 1);
        let j = &items[0];
        assert_eq!(j.tool, "cancel_pending_order");
        assert!(j.word_list && !j.refused && j.success);
        let state = &j.request.state;
        assert!(state["agent_said_last"]
            .as_str()
            .unwrap()
            .contains("Shall I go ahead?"));
        assert_eq!(state["customer_replied"], "Yes, please.");
        assert_eq!(
            state["call_about_to_be_made"]["tool"],
            "cancel_pending_order"
        );

        // A reply with no confirming word fails the word list; the judge's
        // answer is what the oracle says.
        let ep = episode("Hmm, what is the refund method?");
        let mut items = writes(&guards, &[&ep], "jev-test");
        assert!(!items[0].word_list);
        items[0].p_yes = Some(0.1);
        let a = audit("retail", &items, [1, 0], 0.5, 3);
        assert_eq!(a.groups[0].writes, 1);
        assert_eq!(
            (a.groups[0].word_list, a.groups[0].judge, a.groups[0].both),
            (1, 1, 1)
        );
        assert_eq!(a.episodes_judge, [1, 0]);
        assert!(markdown(&[a]).contains("Accepted, successful episodes | 1 | 1 (100.0%)"));
    }
}
