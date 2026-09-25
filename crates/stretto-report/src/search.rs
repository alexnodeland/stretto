//! Flow search (RFC-001 §3.10, Phase 3): NSGA-II, from fugue-evo, over a
//! flow's settings, each setting scored by replaying recorded episodes.
//!
//! A setting is a threshold for each searched site (or the site switched
//! off) and the decider: the flow's arbiter, or the habit alone. Its two
//! objectives are the LLM turns a replay saves, to raise, and the detours it
//! makes, to lower. A [`Replay`] scores a setting. [`CommandReplay`] runs any
//! command that takes a flow file and a decider and prints a `CHECK` line of
//! totals, as `pilot/check_flow.py` does, so the search runs on whatever
//! episodes that command replays. Thresholds live on a grid, and each
//! distinct setting is replayed once.

use crate::flow::Flow;
use anyhow::{bail, Context, Result};
use fugue_evo::algorithms::nsga2::{
    fast_non_dominated_sort, recompute_crowding_distance_per_front, MultiObjectiveFitness, Nsga2,
    Nsga2Individual,
};
use fugue_evo::genome::bounds::{Bounds, MultiBounds};
use fugue_evo::genome::real_vector::RealVector;
use fugue_evo::genome::traits::{EvolutionaryGenome, RealValuedGenome};
use fugue_evo::operators::crossover::SbxCrossover;
use fugue_evo::operators::mutation::PolynomialMutation;
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// The lowest threshold on the grid.
const LOW: f64 = 0.1;
/// The grid's step.
const STEP: f64 = 0.05;
/// Thresholds on the grid, from [`LOW`] to 0.95; one level more switches the
/// site off.
const LEVELS: usize = 18;
/// A threshold that switches a site off (the flow never acts above 1).
pub const OFF: f64 = 2.0;

/// Who decides at a site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decider {
    /// The flow's arbiter: the habit and the System-One model's answers.
    Arbiter,
    /// The habit alone, which asks nothing.
    Habit,
}

impl Decider {
    /// As `check_flow.py --flow-decider` names it.
    pub fn name(self) -> &'static str {
        match self {
            Decider::Arbiter => "arbiter",
            Decider::Habit => "habit",
        }
    }
}

/// One setting of a flow.
#[derive(Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Setting {
    /// Each searched site's threshold; [`OFF`] switches it off.
    pub thresholds: BTreeMap<String, f64>,
    /// Who decides.
    pub decider: Decider,
}

impl Setting {
    /// Every site at `threshold`, decided by `decider`: a hand-set flow.
    pub fn uniform(sites: &[String], threshold: f64, decider: Decider) -> Self {
        Setting {
            thresholds: sites.iter().map(|s| (s.clone(), threshold)).collect(),
            decider,
        }
    }

    /// A name for a hand-set setting, every site alike ("arbiter at
    /// 0.30"); else its [`Setting::key`].
    pub fn label(&self) -> String {
        let mut ts = self.thresholds.values();
        match ts.next() {
            Some(first) if ts.all(|t| t == first) => {
                if *first > 1.0 {
                    format!("{}, every site off", self.decider.name())
                } else {
                    format!("{} at {first:.2}", self.decider.name())
                }
            }
            _ => self.key(),
        }
    }

    /// A stable name for the setting, for the memo and the files.
    pub fn key(&self) -> String {
        let t: Vec<String> = self
            .thresholds
            .values()
            .map(|t| {
                if *t > 1.0 {
                    "off".to_string()
                } else {
                    format!("{t:.2}")
                }
            })
            .collect();
        format!("{}:{}", self.decider.name(), t.join(","))
    }

    /// The setting as NSGA-II's genes: one per site, then the decider.
    pub fn genes(&self) -> Vec<f64> {
        let mut g: Vec<f64> = self
            .thresholds
            .values()
            .map(|&t| {
                let level = if t > 1.0 {
                    LEVELS as f64
                } else {
                    ((t - LOW) / STEP).round().clamp(0.0, (LEVELS - 1) as f64)
                };
                (level + 0.5) / (LEVELS + 1) as f64
            })
            .collect();
        g.push(match self.decider {
            Decider::Arbiter => 0.25,
            Decider::Habit => 0.75,
        });
        g
    }

    /// The setting a genome stands for, over `sites` (in the genes' order).
    pub fn of(sites: &[String], genes: &[f64]) -> Self {
        let thresholds = sites
            .iter()
            .zip(genes)
            .map(|(site, &x)| {
                let level = ((x.clamp(0.0, 1.0) * (LEVELS + 1) as f64) as usize).min(LEVELS);
                let t = if level == LEVELS {
                    OFF
                } else {
                    ((LOW + STEP * level as f64) * 100.0).round() / 100.0
                };
                (site.clone(), t)
            })
            .collect();
        let decider = if genes.get(sites.len()).copied().unwrap_or(0.0) < 0.5 {
            Decider::Arbiter
        } else {
            Decider::Habit
        };
        Setting {
            thresholds,
            decider,
        }
    }

    /// `flow` with this setting's thresholds.
    pub fn apply(&self, flow: &Flow) -> Flow {
        flow.clone().with_thresholds(self.thresholds.clone())
    }
}

/// A replay's totals, as `check_flow.py` prints them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Totals {
    /// LLM turns in the replayed episodes.
    pub turns: u64,
    /// Of those, the turns the flow saved.
    pub turns_saved: u64,
    /// Lookups the agent never made.
    pub detours: u64,
    /// Episodes with a detour.
    pub episodes_with_detour: u64,
    /// Episodes replayed.
    pub episodes: u64,
    /// Decisions the flow handed back because no answer was to be had
    /// (a question not in a replay cache).
    #[serde(default)]
    pub unanswered: u64,
}

impl Totals {
    /// NSGA-II's objectives, both minimized: turns saved, negated, and
    /// detours.
    pub fn objectives(&self) -> Vec<f64> {
        vec![-(self.turns_saved as f64), self.detours as f64]
    }

    /// Turns saved, as a share.
    pub fn share(&self) -> f64 {
        self.turns_saved as f64 / self.turns.max(1) as f64
    }
}

/// Scores a setting of a flow.
pub trait Replay: Sync {
    /// The totals of replaying `flow`, decided by `decider`.
    fn replay(&self, flow: &Flow, decider: Decider) -> Result<Totals>;
}

/// A [`Replay`] that runs a command: `program` with `--flow FILE
/// --flow-decider NAME --out DIR` added, whose standard output ends with a
/// `CHECK {…}` line of totals. Each setting's flow and replay go in a
/// directory of their own under `dir`.
pub struct CommandReplay {
    /// The command and its arguments.
    pub program: Vec<String>,
    /// Where each setting's flow and replay go.
    pub dir: PathBuf,
    count: AtomicUsize,
}

impl CommandReplay {
    /// A replay that runs `program`, working in `dir`.
    pub fn new(program: Vec<String>, dir: PathBuf) -> Self {
        CommandReplay {
            program,
            dir,
            count: AtomicUsize::new(0),
        }
    }
}

impl Replay for CommandReplay {
    fn replay(&self, flow: &Flow, decider: Decider) -> Result<Totals> {
        let n = self.count.fetch_add(1, Ordering::SeqCst);
        let out = self.dir.join(format!("setting-{n:04}"));
        std::fs::create_dir_all(&out)?;
        let path = out.join("flow.json");
        flow.save(&path)?;
        let (program, args) = self
            .program
            .split_first()
            .context("the replay command is empty")?;
        let run = std::process::Command::new(program)
            .args(args)
            .arg("--flow")
            .arg(&path)
            .arg("--flow-decider")
            .arg(decider.name())
            .arg("--out")
            .arg(&out)
            .stderr(std::process::Stdio::null())
            .output()
            .with_context(|| format!("running {program}"))?;
        if !run.status.success() {
            bail!(
                "{program} exited with {} (in {})",
                run.status,
                out.display()
            );
        }
        let stdout = String::from_utf8_lossy(&run.stdout);
        let line = stdout
            .lines()
            .rev()
            .find_map(|l| l.strip_prefix("CHECK "))
            .with_context(|| format!("no CHECK line from {program}"))?;
        let mut totals: Totals = serde_json::from_str(line).context("the CHECK line")?;
        totals.unanswered = unanswered(&out.join("flow.jsonl"));
        Ok(totals)
    }
}

/// Decisions in a flow log that the flow handed back because the
/// System-One model gave no answer.
fn unanswered(log: &Path) -> u64 {
    std::fs::read_to_string(log)
        .map(|t| {
            t.lines()
                .filter(|l| l.contains("the System-One model failed"))
                .count() as u64
        })
        .unwrap_or(0)
}

/// One setting and its totals.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scored {
    /// The setting.
    pub setting: Setting,
    /// Its replay's totals.
    pub totals: Totals,
}

/// NSGA-II's view of a replay: each distinct setting replayed once.
struct Fitness<'a> {
    flow: &'a Flow,
    sites: &'a [String],
    replay: &'a dyn Replay,
    memo: Mutex<BTreeMap<String, Scored>>,
    error: Mutex<Option<String>>,
}

impl Fitness<'_> {
    fn score(&self, setting: &Setting) -> Vec<f64> {
        let key = setting.key();
        if let Some(s) = self.memo.lock().expect("no panics hold the memo").get(&key) {
            return s.totals.objectives();
        }
        match self
            .replay
            .replay(&setting.apply(self.flow), setting.decider)
        {
            Ok(totals) => {
                let objectives = totals.objectives();
                eprintln!(
                    "stretto: {key}: {} turns saved, {} detours",
                    totals.turns_saved, totals.detours
                );
                self.memo.lock().expect("no panics hold the memo").insert(
                    key,
                    Scored {
                        setting: setting.clone(),
                        totals,
                    },
                );
                objectives
            }
            Err(e) => {
                self.error
                    .lock()
                    .expect("no panics hold the error")
                    .get_or_insert_with(|| format!("{e:#}"));
                vec![0.0, f64::MAX]
            }
        }
    }
}

impl MultiObjectiveFitness<RealVector> for Fitness<'_> {
    fn num_objectives(&self) -> usize {
        2
    }

    fn evaluate(&self, genome: &RealVector) -> Vec<f64> {
        self.score(&Setting::of(self.sites, genome.genes()))
    }
}

/// What a search found.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Searched {
    /// The sites searched.
    pub sites: Vec<String>,
    /// NSGA-II's population and generations.
    pub population: usize,
    /// See `population`.
    pub generations: usize,
    /// The seed.
    pub seed: u64,
    /// The hand-set settings the search started from.
    pub seeds: Vec<Scored>,
    /// Every distinct setting replayed.
    pub evaluated: Vec<Scored>,
    /// The front: the settings no other setting replayed beats on both
    /// turns saved and detours, by turns saved.
    pub front: Vec<Scored>,
}

/// The settings no other in `scored` beats on both objectives, by turns
/// saved (ties broken by fewer detours), one per distinct pair of totals.
pub fn front(scored: &[Scored]) -> Vec<Scored> {
    let beats = |a: &Totals, b: &Totals| {
        a.turns_saved >= b.turns_saved
            && a.detours <= b.detours
            && (a.turns_saved > b.turns_saved || a.detours < b.detours)
    };
    let mut out: Vec<Scored> = scored
        .iter()
        .filter(|s| !scored.iter().any(|o| beats(&o.totals, &s.totals)))
        .cloned()
        .collect();
    out.sort_by(|a, b| {
        b.totals
            .turns_saved
            .cmp(&a.totals.turns_saved)
            .then(a.totals.detours.cmp(&b.totals.detours))
            .then(a.setting.key().cmp(&b.setting.key()))
    });
    out.dedup_by(|a, b| {
        a.totals.turns_saved == b.totals.turns_saved && a.totals.detours == b.totals.detours
    });
    out
}

/// Search `flow`'s settings over `sites` with NSGA-II: `population`
/// settings, `seeds` first and the rest drawn at random, bred for
/// `generations` generations, each scored by `replay`.
pub fn search(
    flow: &Flow,
    sites: &[String],
    seeds: &[Setting],
    replay: &dyn Replay,
    population: usize,
    generations: usize,
    seed: u64,
) -> Result<Searched> {
    let fitness = Fitness {
        flow,
        sites,
        replay,
        memo: Mutex::new(BTreeMap::new()),
        error: Mutex::new(None),
    };
    let check = |fitness: &Fitness| -> Result<()> {
        match fitness
            .error
            .lock()
            .expect("no panics hold the error")
            .take()
        {
            Some(e) => bail!("a replay failed: {e}"),
            None => Ok(()),
        }
    };
    let bounds = MultiBounds::new(vec![Bounds::new(0.0, 1.0); sites.len() + 1]);
    let crossover = SbxCrossover::new(15.0);
    let mutation = PolynomialMutation::new(20.0);
    let nsga2: Nsga2<RealVector, Fitness, SbxCrossover, PolynomialMutation> =
        Nsga2::new(population.max(seeds.len()).max(2));
    let mut rng = StdRng::seed_from_u64(seed);
    let mut pop: Vec<Nsga2Individual<RealVector>> = (0..nsga2.population_size)
        .map(|i| {
            let genome = match seeds.get(i) {
                Some(s) => RealVector::new(s.genes()),
                None => RealVector::generate(&mut rng, &bounds),
            };
            let objectives = fitness.evaluate(&genome);
            Nsga2Individual::new(genome, objectives)
        })
        .collect();
    check(&fitness)?;
    fast_non_dominated_sort(&mut pop);
    recompute_crowding_distance_per_front(&mut pop);
    for g in 0..generations {
        nsga2.step_bounded(&mut pop, &fitness, &crossover, &mutation, &bounds, &mut rng);
        check(&fitness)?;
        eprintln!(
            "stretto: generation {} of {generations}: {} settings replayed",
            g + 1,
            fitness.memo.lock().expect("no panics hold the memo").len()
        );
    }
    let memo = fitness.memo.into_inner().expect("no panics hold the memo");
    let evaluated: Vec<Scored> = memo.into_values().collect();
    let seeds = seeds
        .iter()
        .filter_map(|s| {
            evaluated
                .iter()
                .find(|e| e.setting.key() == s.key())
                .cloned()
        })
        .collect();
    Ok(Searched {
        sites: sites.to_vec(),
        population: nsga2.population_size,
        generations,
        seed,
        seeds,
        front: front(&evaluated),
        evaluated,
    })
}

/// Replay each of `settings` on `flow` with `replay` (a held-out set).
pub fn rescore(flow: &Flow, settings: &[Setting], replay: &dyn Replay) -> Result<Vec<Scored>> {
    settings
        .iter()
        .map(|s| {
            let totals = replay.replay(&s.apply(flow), s.decider)?;
            eprintln!(
                "stretto: {}: {} turns saved, {} detours",
                s.key(),
                totals.turns_saved,
                totals.detours
            );
            Ok(Scored {
                setting: s.clone(),
                totals,
            })
        })
        .collect()
}

/// A table of scored settings, in Markdown: one row each, the sites'
/// thresholds in `sites`' order.
pub fn table(scored: &[Scored], sites: &[String], names: &[String]) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        "| | Decider | Turns saved | Share | Detours | Episodes with a detour | Unanswered |"
    );
    for site in sites {
        let _ = write!(s, " `{site}` |");
    }
    let _ = writeln!(s);
    let _ = write!(s, "|---|---|---|---|---|---|---|");
    for _ in sites {
        let _ = write!(s, "---|");
    }
    let _ = writeln!(s);
    for (i, sc) in scored.iter().enumerate() {
        let t = &sc.totals;
        let _ = write!(
            s,
            "| {} | {} | {} | {:.1}% | {} | {} | {} |",
            names
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("{}", i + 1)),
            sc.setting.decider.name(),
            t.turns_saved,
            100.0 * t.share(),
            t.detours,
            t.episodes_with_detour,
            t.unanswered
        );
        for site in sites {
            let cell = match sc.setting.thresholds.get(site) {
                Some(t) if *t > 1.0 => "off".to_string(),
                Some(t) => format!("{t:.2}"),
                None => "—".to_string(),
            };
            let _ = write!(s, " {cell} |");
        }
        let _ = writeln!(s);
    }
    s
}

/// A search's report, in Markdown.
pub fn markdown(r: &Searched) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# Flow search\n\nNSGA-II over each site's threshold (0.10 to 0.95, or off) and the decider, \
         {} settings for {} generations (seed {}): {} distinct settings replayed.\n",
        r.population,
        r.generations,
        r.seed,
        r.evaluated.len()
    );
    let _ = writeln!(s, "## The hand-set settings\n");
    let names: Vec<String> = r.seeds.iter().map(|x| x.setting.label()).collect();
    let _ = writeln!(s, "{}", table(&r.seeds, &r.sites, &names));
    let _ = writeln!(s, "## The front\n");
    let names: Vec<String> = (1..=r.front.len()).map(|i| format!("F{i}")).collect();
    let _ = writeln!(s, "{}", table(&r.front, &r.sites, &names));
    let unanswered = r.front.iter().filter(|f| f.totals.unanswered > 0).count();
    if unanswered > 0 {
        let _ = writeln!(
            s,
            "{unanswered} of the front's settings handed back decisions for want of an answer \
             (unanswered), so their totals are not what the flow would do. Replay them with the \
             System-One model answering before choosing one.\n"
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A replay whose totals follow the thresholds: each site on saves
    /// turns the lower its threshold, and costs detours below 0.3.
    struct Toy;

    impl Replay for Toy {
        fn replay(&self, flow: &Flow, decider: Decider) -> Result<Totals> {
            let (mut saved, mut detours) = (0u64, 0u64);
            for t in flow.thresholds().values().filter(|t| **t <= 1.0) {
                saved += ((1.0 - t) * 10.0).round() as u64;
                if *t < 0.3 {
                    detours += ((0.3 - t) * 40.0).round() as u64;
                }
            }
            if decider == Decider::Habit {
                detours += 5;
            }
            Ok(Totals {
                turns: 100,
                turns_saved: saved,
                detours,
                episodes: 10,
                ..Totals::default()
            })
        }
    }

    fn sites() -> Vec<String> {
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    }

    #[test]
    fn settings_round_trip_through_genes() {
        let s = Setting {
            thresholds: BTreeMap::from([
                ("a".to_string(), 0.1),
                ("b".to_string(), 0.55),
                ("c".to_string(), OFF),
            ]),
            decider: Decider::Habit,
        };
        assert_eq!(Setting::of(&sites(), &s.genes()), s);
        let d0 = Setting::uniform(&sites(), 0.3, Decider::Arbiter);
        assert_eq!(Setting::of(&sites(), &d0.genes()), d0);
        // Every gene maps onto the grid.
        for x in [0.0, 0.01, 0.33, 0.5, 0.94, 0.96, 1.0] {
            let t = Setting::of(&sites(), &[x, x, x, x]).thresholds["a"];
            assert!(t == OFF || (0.1..=0.95).contains(&t), "{x} -> {t}");
        }
    }

    #[test]
    fn the_search_finds_the_front_and_keeps_the_seeds() {
        let flow = crate::flow::tests::toy_flow();
        let seeds = vec![Setting::uniform(&sites(), 0.3, Decider::Arbiter)];
        let r = search(&flow, &sites(), &seeds, &Toy, 12, 8, 25).unwrap();
        assert_eq!(r.seeds.len(), 1);
        assert_eq!(r.seeds[0].totals.turns_saved, 21);
        assert_eq!(r.seeds[0].totals.detours, 0);
        // Nothing on the front is beaten by anything replayed.
        for f in &r.front {
            assert!(!r.evaluated.iter().any(|e| {
                e.totals.turns_saved >= f.totals.turns_saved
                    && e.totals.detours <= f.totals.detours
                    && (e.totals.turns_saved > f.totals.turns_saved
                        || e.totals.detours < f.totals.detours)
            }));
        }
        // With no detours, the arbiter at 0.3 everywhere is the best there is.
        let best_clean = r
            .front
            .iter()
            .filter(|f| f.totals.detours == 0)
            .map(|f| f.totals.turns_saved)
            .max()
            .unwrap();
        assert_eq!(best_clean, 21);
        // The front reaches past it, trading detours for turns.
        assert!(r.front.iter().any(|f| f.totals.turns_saved > 21));
        // The same seed searches the same way.
        let again = search(&flow, &sites(), &seeds, &Toy, 12, 8, 25).unwrap();
        assert_eq!(again.evaluated.len(), r.evaluated.len());
    }

    #[test]
    fn the_report_flags_a_front_the_replay_could_not_answer() {
        let scored = |unanswered| Scored {
            setting: Setting::uniform(&sites(), 0.3, Decider::Arbiter),
            totals: Totals {
                turns: 100,
                turns_saved: 10,
                unanswered,
                ..Totals::default()
            },
        };
        let searched = |front: Vec<Scored>| Searched {
            sites: sites(),
            population: 4,
            generations: 1,
            seed: 25,
            seeds: vec![],
            evaluated: front.clone(),
            front,
        };
        let warning = "handed back decisions for want of an answer";
        assert!(!markdown(&searched(vec![scored(0)])).contains(warning));
        let flagged = markdown(&searched(vec![scored(0), scored(3)]));
        assert!(flagged.contains(&format!("1 of the front's settings {warning}")));
    }

    #[test]
    fn a_setting_is_kept_in_the_flow_ir() {
        let flow = crate::flow::tests::toy_flow();
        let sites = flow.sites();
        let setting = Setting::of(&sites, &vec![0.97; sites.len() + 1]);
        let set = setting.apply(&flow);
        assert!(set.thresholds().values().all(|t| *t == OFF));
        let written = serde_json::to_string(&set).unwrap();
        let back = Flow::from_json(&written).unwrap();
        assert_eq!(back.thresholds(), set.thresholds());
        // With thresholds a flow is format 2, which a build that reads only
        // format 1 refuses; a version-1 file with thresholds is refused here.
        assert_eq!(serde_json::to_value(&set).unwrap()["stretto_flow"], 2);
        let lying = written.replacen("\"stretto_flow\":2", "\"stretto_flow\":1", 1);
        assert!(Flow::from_json(&lying).is_err());
        // A flow with none writes none, as format 1.
        let plain = serde_json::to_value(&flow).unwrap();
        assert!(plain.get("thresholds").is_none());
        assert_eq!(plain["stretto_flow"], 1);
        let cleared = set.with_thresholds(BTreeMap::new());
        assert_eq!(serde_json::to_value(&cleared).unwrap()["stretto_flow"], 1);
    }
}
