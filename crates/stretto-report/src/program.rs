//! A flow's run as a fugue program (RFC-001 §3.2 and §3.5).
//!
//! After one of the agent's calls returns, a flow may make a lookup, then
//! decide again, until it hands back. That loop is a fugue program, in
//! fugue's serializable program format, and the flow IR holds it
//! ([`Flow::program`]). When a flow loads, its program is checked against
//! the flow's own distributions, and each run interprets it into a fugue
//! `Model` ([`FlowProgram`]).
//!
//! `stretto compile` and `stretto learn` write the standard program
//! ([`standard`], [`PROGRAM`] in the format's text). Step `i` has a decision
//! site, `decide#i`: a categorical over handing back ([`HAND_BACK`]) and
//! every lookup the flow could make, which puts weight only on the lookups
//! offered after the call that just returned. Unless it hands back, an
//! outcome site, `outcome#i`, says whether the lookup succeeded. The next
//! decision is made after that lookup, and the program stops after
//! `max_lookups` lookups.
//!
//! A program draws only on the flow's statistics ([`RunModel`]), registered
//! as two distributions: `Decide`, what the agent did next after each call
//! in training, and `Outcome`, how often each tool's calls succeeded. Each
//! carries its site as metadata ([`DecideSite`], [`OutcomeSite`]), so a
//! handler knows which decision, or which call, it is at.
//!
//! One program has three interpreters:
//! - **Simulate.** fugue's `PriorHandler` runs the flow on its statistics
//!   alone: the lookups it would make if the agent did as in training.
//! - **Execute.** `stretto-proxy` runs it with `run_async` and a handler that
//!   makes each decision with the flow's arbiter ([`Flow::next_with`]) and
//!   each lookup against the server, scoring the server's answer at the
//!   outcome site.
//! - **Audit.** `ScoreGivenTrace` scores a recorded run: how surprised the
//!   flow's statistics are by it.

use crate::flow::Flow;
use crate::shadow::{Sites, RESPOND};
use fugue::program::{
    Arity, CompiledProgram, Data, HostDist, Program, ProgramError, Registry, SiteType, Value,
};
use fugue::{beta_posterior, dirichlet_predictive, Bernoulli, Categorical, Model, WithMeta};
use std::collections::HashMap;
use std::sync::Arc;
use stretto_model::world::decode;
use stretto_model::{Action, Vocab};

/// Option 0 at every decision site: hand back.
pub const HAND_BACK: usize = 0;

/// The standard run, in the text of fugue's program format. `call` is the
/// id of the tool whose call just returned ([`RunModel::tools`]),
/// `call_failed` whether it failed, and `max_lookups` the most lookups to
/// make. Each decision's value is an option's id, 0 to hand back. The run's
/// value is the id of the last tool called.
pub const PROGRAM: &str = "let prev = call;
let failed = call_failed;
for i in 0..max_lookups {
    let d <- sample(addr!(\"decide\", i), Decide(prev, failed));
    if d == 0 {
        break;
    }
    let ok <- sample(addr!(\"outcome\", i), Outcome(d));
    prev = d;
    failed = !ok;
}
pure(prev)";

/// The standard run ([`PROGRAM`]): the program `stretto compile` and
/// `stretto learn` write, and the one a flow written before flows held
/// their program runs.
pub fn standard() -> Program {
    Program::parse(PROGRAM).expect("the standard program parses")
}

/// A decision site's metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct DecideSite {
    /// The site: the tool that just returned, marked when it failed, as
    /// [`Sites::name`] writes it.
    pub site: String,
    /// What the decision chooses between: [`RESPOND`] (hand back), then
    /// every lookup the flow could make, by name. The same at every site;
    /// the site's distribution puts weight only on the lookups it offers.
    pub options: Vec<String>,
}

/// An outcome site's metadata: the lookup whose result it scores.
#[derive(Clone, Debug, PartialEq)]
pub struct OutcomeSite {
    /// The tool called.
    pub tool: String,
}

/// A flow's statistics, as its run draws on them.
#[derive(Clone, Debug)]
pub struct RunModel {
    sites: Sites,
    vocab: Vocab,
    /// Every tool by id: the options, then the flow's other tools.
    tools: Vec<String>,
    /// What followed each step `(action id, failed)` in training: counts by
    /// action id, and their total.
    followed: HashMap<(u32, bool), (HashMap<u32, f64>, f64)>,
}

impl RunModel {
    /// The statistics of `flow`.
    pub fn new(flow: &Flow) -> Self {
        let mut followed: HashMap<(u32, bool), (HashMap<u32, f64>, f64)> = HashMap::new();
        let base = flow.habit.base();
        for id in 0..flow.vocab.len() as u32 {
            for failed in [false, true] {
                let outcome = u32::from(failed);
                let counts = base
                    .followed(|s| matches!(decode(s), Some((a, o, _)) if a == id && o == outcome));
                if counts.1 > 0.0 {
                    followed.insert((id, failed), counts);
                }
            }
        }
        let options: Vec<String> = std::iter::once(RESPOND.to_string())
            .chain(flow.sites.reads().cloned())
            .collect();
        let others: Vec<String> = flow
            .manifest
            .tools
            .keys()
            .filter(|t| !options.contains(t))
            .cloned()
            .collect();
        Self {
            sites: flow.sites.clone(),
            vocab: flow.vocab.clone(),
            tools: options.into_iter().chain(others).collect(),
            followed,
        }
    }

    /// Every tool, by id: the options first, [`RESPOND`] at [`HAND_BACK`]
    /// and then every lookup the flow could make, so a decision's value is
    /// its lookup's id; then the flow's other tools. A run after a call to
    /// a tool the flow does not know gives it the id one past the last.
    pub fn tools(&self) -> &[String] {
        &self.tools
    }

    /// A tool's id (see [`Self::tools`]).
    pub fn id(&self, tool: &str) -> usize {
        self.tools
            .iter()
            .position(|t| t == tool)
            .unwrap_or(self.tools.len())
    }

    /// The tool with id `id`, if the flow knows one.
    pub fn tool(&self, id: usize) -> Option<&str> {
        self.tools.get(id).map(String::as_str)
    }

    fn counts(&self, tool: &str, failed: bool) -> Option<&(HashMap<u32, f64>, f64)> {
        let id = self.vocab.id(&Action::Tool(tool.to_string()));
        self.followed.get(&(id, failed))
    }

    /// Every decision's options: [`RESPOND`] (hand back), then every lookup
    /// the flow could make.
    pub fn options(&self) -> Vec<String> {
        self.tools[..=self.sites.reads().count()].to_vec()
    }

    /// The lookups the flow offers after a call to `tool` (failed or not),
    /// the options its arbiter weighs there.
    pub fn offered(&self, tool: &str, failed: bool) -> Vec<String> {
        self.sites.options(tool, failed)
    }

    /// The decision after a call to `tool` (failed or not). Handing back and
    /// each lookup offered there get the posterior predictive of a flat
    /// Dirichlet over them, given what the agent did next there in training;
    /// every step but the lookups offered, a write or a message included,
    /// counts as handing back. The lookups not offered there get 0.
    pub fn decide(&self, tool: &str, failed: bool) -> WithMeta<Categorical, DecideSite> {
        let options = self.options();
        let offered = self.offered(tool, failed);
        let (by_action, total) = self
            .counts(tool, failed)
            .map(|(c, t)| (Some(c), *t))
            .unwrap_or((None, 0.0));
        let seen = |lookup: &String| {
            let id = self.vocab.id(&Action::Tool(lookup.clone()));
            by_action.and_then(|c| c.get(&id)).copied().unwrap_or(0.0)
        };
        // Handing back, then each lookup offered: its count in training.
        let looked: f64 = offered.iter().map(seen).sum();
        let counts: Vec<u64> = std::iter::once((total - looked).max(0.0))
            .chain(offered.iter().map(seen))
            .map(|n| n.round() as u64)
            .collect();
        let predictive = dirichlet_predictive(&vec![1.0; counts.len()], &counts)
            .expect("a flat Dirichlet and counts make a predictive");
        let probs: Vec<f64> = options
            .iter()
            .enumerate()
            .map(|(i, option)| match i {
                HAND_BACK => predictive[0],
                _ => offered
                    .iter()
                    .position(|o| o == option)
                    .map_or(0.0, |j| predictive[j + 1]),
            })
            .collect();
        WithMeta::new(
            Categorical::new(probs).expect("predictive probabilities are a distribution"),
            DecideSite {
                site: Sites::name(tool, failed),
                options,
            },
        )
    }

    /// The outcome of a call to `tool`: whether it succeeds, with the
    /// posterior predictive of a flat Beta given how often its calls
    /// succeeded and failed in training.
    pub fn outcome(&self, tool: &str) -> WithMeta<Bernoulli, OutcomeSite> {
        let n = |failed| {
            self.counts(tool, failed)
                .map_or(0, |(_, total)| total.round() as u64)
        };
        let (a, b) = beta_posterior(1.0, 1.0, n(false), n(true))
            .expect("a flat Beta and counts make a posterior");
        WithMeta::new(
            Bernoulli::new(a / (a + b)).expect("a predictive probability is in (0, 1)"),
            OutcomeSite {
                tool: tool.to_string(),
            },
        )
    }
}

/// A flow's program with the flow's distributions: `Decide(prev, failed)`,
/// the decision after the tool with id `prev` ([`RunModel::decide`]), and
/// `Outcome(d)`, whether the lookup with id `d` succeeds
/// ([`RunModel::outcome`]). Nothing else is registered, so a program draws
/// only on the flow's statistics. Its data are `call`, `call_failed` and
/// `max_lookups`, as in [`PROGRAM`].
#[derive(Clone, Debug)]
pub struct FlowProgram {
    program: Program,
    model: Arc<RunModel>,
}

impl FlowProgram {
    /// `flow`'s program. Fails when the program does not compile against
    /// the flow's distributions and data: a name it does not know, or a
    /// distribution given the wrong number of arguments.
    pub fn new(flow: &Flow) -> anyhow::Result<Self> {
        let program = Self {
            program: flow.program().clone(),
            model: Arc::new(RunModel::new(flow)),
        };
        program
            .compile(RESPOND, false, 0)
            .map_err(|e| anyhow::anyhow!("the flow's program: {e}"))?;
        Ok(program)
    }

    /// The program.
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// The statistics its distributions draw on.
    pub fn model(&self) -> &Arc<RunModel> {
        &self.model
    }

    /// The flow's run after a call to `tool` (failed or not), making at most
    /// `max_lookups` lookups.
    pub fn run(&self, tool: &str, failed: bool, max_lookups: usize) -> Model<Value> {
        self.compile(tool, failed, max_lookups)
            .expect("the program compiled when the flow loaded")
            .build()
    }

    fn compile(
        &self,
        tool: &str,
        failed: bool,
        max_lookups: usize,
    ) -> Result<CompiledProgram, ProgramError> {
        // A tool the flow does not know gets the next id, so that its
        // decision still names it.
        let mut names = self.model.tools.clone();
        let call = self.model.id(tool);
        if call == names.len() {
            names.push(tool.to_string());
        }
        let data = Data::new()
            .with("call", call)
            .with("call_failed", failed)
            .with("max_lookups", max_lookups);
        self.program
            .compile(&registry(&self.model, Arc::new(names)), &data)
    }
}

/// `Decide` and `Outcome`, over the tools `names` by id.
fn registry(model: &Arc<RunModel>, names: Arc<Vec<String>>) -> Registry {
    let mut registry = Registry::empty();
    let (m, n) = (model.clone(), names.clone());
    registry.register_distribution("Decide", SiteType::Usize, Arity::Exact(2), move |args| {
        let failed = args[1]
            .as_bool()
            .ok_or("`Decide` takes whether the call failed")?;
        Ok(HostDist::Usize(Box::new(
            m.decide(tool(&n, &args[0])?, failed),
        )))
    });
    let m = model.clone();
    registry.register_distribution("Outcome", SiteType::Bool, Arity::Exact(1), move |args| {
        Ok(HostDist::Bool(Box::new(m.outcome(tool(&names, &args[0])?))))
    });
    registry
}

/// The tool with id `id` among `names`.
fn tool<'a>(names: &'a [String], id: &Value) -> Result<&'a str, String> {
    id.as_usize()
        .and_then(|i| names.get(i))
        .map(String::as_str)
        .ok_or_else(|| format!("no tool has the id {id:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fugue::runtime::handler::run as interpret;
    use fugue::{addr, pure, sample, ModelExt, PriorHandler, ScoreGivenTrace, Trace};
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn flow() -> Flow {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/examples/retail-10-sessions.flow.json");
        Flow::load(&path).unwrap()
    }

    /// A run's lookups, each with whether it succeeded, read from its trace.
    fn lookups(model: &RunModel, trace: &Trace) -> Vec<(String, bool)> {
        let mut steps = Vec::new();
        for i in 0.. {
            match trace.get_usize(&addr!("decide", i)) {
                Some(d) if d != HAND_BACK => {
                    let ok = trace.get_bool(&addr!("outcome", i)).unwrap();
                    steps.push((model.tool(d).unwrap().to_string(), ok));
                }
                _ => break,
            }
        }
        steps
    }

    /// The standard run written in Rust, as it was before flows held their
    /// program: its lookups, each with whether it succeeded.
    fn native(
        model: Arc<RunModel>,
        i: usize,
        tool: String,
        failed: bool,
        max_lookups: usize,
    ) -> Model<Vec<(String, bool)>> {
        if i >= max_lookups {
            return pure(Vec::new());
        }
        let decide = model.decide(&tool, failed);
        let options = decide.meta().options.clone();
        sample(addr!("decide", i), decide).bind(move |choice| {
            if choice == HAND_BACK {
                return pure(Vec::new());
            }
            let lookup = options[choice].clone();
            let outcome = model.outcome(&lookup);
            sample(addr!("outcome", i), outcome).bind(move |ok| {
                native(model, i + 1, lookup.clone(), !ok, max_lookups).map(move |rest| {
                    let mut steps = vec![(lookup, ok)];
                    steps.extend(rest);
                    steps
                })
            })
        })
    }

    #[test]
    fn a_decision_offers_handing_back_and_the_sites_lookups() {
        let model = RunModel::new(&flow());
        let d = model.decide("get_user_details", false);
        assert_eq!(d.meta().site, "get_user_details");
        assert_eq!(d.meta().options, model.options());
        assert_eq!(d.meta().options[HAND_BACK], RESPOND);
        let probs = d.dist().probs();
        assert!((probs.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        // Only the lookups offered there, and handing back, carry weight.
        let offered = model.offered("get_user_details", false);
        assert!(offered.iter().any(|o| o == "get_order_details"));
        for (option, p) in d.meta().options.iter().zip(probs).skip(1) {
            assert_eq!(*p > 0.0, offered.contains(option), "{option}");
        }
        // In training the agent read an order after the user's details.
        let order = d
            .meta()
            .options
            .iter()
            .position(|o| o == "get_order_details")
            .unwrap();
        assert!(probs[order] > probs[HAND_BACK], "{:?}", d.meta());
        // A site never seen offers only handing back.
        let unseen = model.decide("no_such_tool", true);
        assert_eq!(unseen.dist().probs()[HAND_BACK], 1.0);
        assert_eq!(unseen.dist().probs().iter().sum::<f64>(), 1.0);
    }

    #[test]
    fn simulating_makes_offered_lookups_at_addressed_sites() {
        let program = FlowProgram::new(&flow()).unwrap();
        let model = program.model().clone();
        let start = "find_user_id_by_name_zip";
        for seed in 0..20 {
            let mut rng = StdRng::seed_from_u64(seed);
            let (last, trace) = interpret(
                PriorHandler {
                    rng: &mut rng,
                    trace: Trace::default(),
                },
                program.run(start, false, 4),
            );
            let steps = lookups(&model, &trace);
            assert!(steps.len() <= 4);
            let (mut tool, mut failed) = (start.to_string(), false);
            for (lookup, ok) in &steps {
                assert!(model.offered(&tool, failed).contains(lookup));
                (tool, failed) = (lookup.clone(), !ok);
            }
            // The run's value is the last tool called.
            assert_eq!(last.as_usize(), Some(model.id(&tool)));
            // It ends by handing back, or after its last lookup, and has no
            // sites but its decisions and their outcomes.
            let end = trace.get_usize(&addr!("decide", steps.len()));
            assert!(steps.len() == 4 || end == Some(HAND_BACK));
            let sites = 2 * steps.len() + usize::from(end.is_some());
            assert_eq!(trace.choices.len(), sites);
            // Replayed under the same program, the run scores the same.
            let (_, scored) = interpret(
                ScoreGivenTrace {
                    base: trace.clone(),
                    trace: Trace::default(),
                },
                program.run(start, false, 4),
            );
            assert!((scored.total_log_weight() - trace.total_log_weight()).abs() < 1e-12);
        }
    }

    #[test]
    fn the_program_draws_what_the_native_run_drew() {
        // Interpreted from the program format, the standard run draws the
        // same sites, values and scores as the run written in Rust, bit for
        // bit, after every call the flow knows and one it does not.
        let program = FlowProgram::new(&flow()).unwrap();
        let model = program.model().clone();
        let mut starts: Vec<String> = model.tools().to_vec();
        starts.push("no_such_tool".to_string());
        let mut lookups_made = 0;
        for tool in &starts {
            for failed in [false, true] {
                for seed in 0..25 {
                    let mut rng = StdRng::seed_from_u64(seed);
                    let (steps, expected) = interpret(
                        PriorHandler {
                            rng: &mut rng,
                            trace: Trace::default(),
                        },
                        native(model.clone(), 0, tool.clone(), failed, 8),
                    );
                    let mut rng = StdRng::seed_from_u64(seed);
                    let (_, trace) = interpret(
                        PriorHandler {
                            rng: &mut rng,
                            trace: Trace::default(),
                        },
                        program.run(tool, failed, 8),
                    );
                    assert_eq!(trace.choices.len(), expected.choices.len());
                    for (address, choice) in &expected.choices {
                        let got = &trace.choices[address];
                        assert_eq!(got.value, choice.value, "{tool} {failed} {seed} {address}");
                        assert_eq!(got.logp.to_bits(), choice.logp.to_bits());
                    }
                    assert_eq!(trace.log_prior.to_bits(), expected.log_prior.to_bits());
                    assert_eq!(lookups(&model, &trace), steps);
                    lookups_made += steps.len();
                }
            }
        }
        // The runs compared made lookups, not only decisions to hand back.
        assert!(lookups_made > 100, "{lookups_made}");
    }

    #[test]
    fn a_flow_holds_its_program() {
        let mut flow = flow();
        // A flow written before flows held their program runs the standard
        // one, which prints as it is written here.
        assert_eq!(*flow.program(), standard());
        assert_eq!(standard().to_string(), PROGRAM);
        // The program is written with the flow, in fugue's format, and read
        // back.
        let json = serde_json::to_value(&flow).unwrap();
        assert_eq!(json["program"]["fugue_program"], 1);
        let back = Flow::from_json(&json.to_string()).unwrap();
        assert_eq!(*back.program(), standard());
        // Another program, read from the flow: this one stops after a lookup
        // fails.
        let stops = PROGRAM.replace("    prev = d;\n", "    if !ok { break; }\n    prev = d;\n");
        assert_ne!(stops, PROGRAM);
        flow.program = Program::parse(&stops).unwrap();
        let back = Flow::from_json(&serde_json::to_string(&flow).unwrap()).unwrap();
        assert_ne!(*back.program(), standard());
        let program = FlowProgram::new(&back).unwrap();
        let mut stopped = 0;
        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let (_, trace) = interpret(
                PriorHandler {
                    rng: &mut rng,
                    trace: Trace::default(),
                },
                program.run("find_user_id_by_name_zip", false, 8),
            );
            let steps = lookups(program.model(), &trace);
            for (i, (_, ok)) in steps.iter().enumerate() {
                assert!(*ok || i + 1 == steps.len(), "{steps:?}");
            }
            stopped += usize::from(steps.last().is_some_and(|(_, ok)| !ok));
        }
        assert!(stopped > 0);
    }

    #[test]
    fn a_program_that_does_not_compile_does_not_load() {
        let mut flow = flow();
        // Only the flow's distributions are registered.
        flow.program = Program::parse(
            "let x <- sample(addr!(\"x\"), Normal(0.0, 1.0));
             pure(x)",
        )
        .unwrap();
        let e = FlowProgram::new(&flow).unwrap_err().to_string();
        assert!(e.contains("Normal"), "{e}");
        let e = Flow::from_json(&serde_json::to_string(&flow).unwrap())
            .unwrap_err()
            .to_string();
        assert!(e.contains("Normal"), "{e}");
        // `Decide` takes two arguments.
        flow.program =
            Program::parse(&PROGRAM.replace("Decide(prev, failed)", "Decide(prev)")).unwrap();
        let e = FlowProgram::new(&flow).unwrap_err().to_string();
        assert!(e.contains("Decide"), "{e}");
    }
}
