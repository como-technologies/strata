//! The run: which rules apply, how each one runs, and what the report holds.
//!
//! A run reads the catalog once, keeps the rules of the engine, the release, and the tiers asked
//! for, runs each check, and assembles the report with the confidence of each rule from the
//! ledger.

use std::collections::BTreeMap;

use serde_json::Value;
use strata::closed_set::ClosedSet;
use strata::json::text;
use strata::term::color::Paint;
use strata::term::progress::Progress;

use crate::confidence::{Confidence, Ledger};
use crate::error::{Error, Result};
use crate::jq::Jq;
use crate::probe::{self, Backend, Outcome, Verdict};
use crate::report::{self, Assembly, Options, Report, Subject};
use crate::rule::{Catalog, Rule, Tier};
use crate::world::World;

mod rules;
mod verbose;
mod watch;

use verbose::verbose_lines;

pub use rules::{RULES_SCHEMA, list_rules, rules_document, rules_text};
pub use watch::{WatchInputs, WatchOptions, reading, watch};

/// What a run keeps of the catalog.
#[derive(Debug, Clone)]
pub struct Selection {
    /// The tiers to run. A rule of another tier is `SKIP`.
    pub tiers: Vec<Tier>,
    /// Only these rules, by id, when not empty.
    pub ids: Vec<String>,
    /// A failed probe counts as a failed check.
    pub strict: bool,
    /// The readiness the run needs, as a share.
    pub ready: Option<f64>,
}

impl Default for Selection {
    fn default() -> Self {
        Selection {
            tiers: vec![Tier::Static, Tier::Palette, Tier::Cloud],
            ids: vec![],
            strict: false,
            ready: None,
        }
    }
}

impl Selection {
    /// True when the rule `id` is one that the run keeps: all of them when `ids` is empty.
    fn keeps(&self, id: &str) -> bool {
        self.ids.is_empty() || self.ids.iter().any(|kept| kept == id)
    }

    /// Makes sure that each id of `ids` names a rule of `catalog`.
    ///
    /// # Errors
    ///
    /// [`Error::Usage`] that names each id that the catalog does not have, and tells how to list
    /// the ids. A rule that left the catalog is one of them.
    pub fn require_known(&self, catalog: &Catalog) -> Result<()> {
        let unknown: Vec<&str> = self
            .ids
            .iter()
            .map(String::as_str)
            .filter(|id| catalog.rules().all(|r| r.id != *id))
            .collect();
        if unknown.is_empty() {
            return Ok(());
        }
        Err(Error::Usage {
            message: format!(
                "--rule: no rule has the id {}. Run strata-preflight --rules to list the ids",
                unknown.join(", ")
            ),
        })
    }
}

/// The rules of the catalog that apply to `world` under `selection`, in the order of the
/// catalog, each with why it does not run when it does not. `jq` runs the `when` of each rule
/// over `world_value`.
pub fn applicable<'a>(
    catalog: &'a Catalog,
    world: &World,
    world_value: &Value,
    selection: &Selection,
    jq: &Jq,
) -> Vec<(&'a Rule, Option<Outcome>)> {
    catalog
        .rules()
        .filter(|r| r.applies_to(&world.cloud))
        .filter(|r| selection.keeps(&r.id))
        .map(|r| {
            let skip = if !r.applies_to_release(world.release.as_deref()) {
                Some(Outcome::not_run(
                    Verdict::Skipped,
                    format!(
                        "the rule is for Palette {}, and this run is {}",
                        r.releases.as_deref().unwrap_or("?"),
                        world.release.as_deref().unwrap_or("?")
                    ),
                ))
            } else if let Some(when) = &r.when
                && !holds(jq, r, when, world_value)
            {
                Some(Outcome::not_run(
                    Verdict::Skipped,
                    format!("does not apply here: `{}` is not true", when.trim()),
                ))
            } else if r.check.is_none() {
                match r.tier {
                    // a row with no check: the report says what to run by hand
                    Tier::Cloud | Tier::Static | Tier::Palette | Tier::Watch => Some(Outcome::not_run(
                        Verdict::NotChecked,
                        "no check in strata-preflight; the reproduce lines say what to run by hand",
                    )),
                    Tier::Race => Some(Outcome::not_run(
                        Verdict::NotChecked,
                        "a race: no check before the build finds it; the watch finds its effect",
                    )),
                    Tier::Tool => Some(Outcome::not_run(
                        Verdict::Skipped,
                        "a rule of the tool that makes the entity, out of scope here",
                    )),
                }
            } else if !selection.tiers.contains(&r.effective_tier()) {
                if r.effective_tier() == Tier::Watch {
                    // a watch rule is of the build, not of this run: it does not apply here
                    Some(Outcome::not_run(
                        Verdict::Skipped,
                        "a watch rule runs with --watch cluster:UID, during the build",
                    ))
                } else {
                    // the check applies and did not run: it counts against the readiness
                    Some(Outcome::not_run(
                        Verdict::NotRun,
                        format!("tier {} was not asked for", r.effective_tier().as_str()),
                    ))
                }
            } else {
                None
            };
            (r, skip)
        })
        .collect()
}

/// True when the `when` program of a rule gives `true` on the world. A program that fails does
/// not hold, so a rule with a broken `when` is `SKIP` and not a wrong verdict.
fn holds(jq: &Jq, rule: &Rule, when: &str, world: &Value) -> bool {
    matches!(jq.first(&rule.id, when, world, None), Ok(Value::Bool(true)))
}

/// The inputs of a run.
pub struct RunInputs<'a> {
    /// The catalog, with any probes file.
    pub catalog: &'a Catalog,
    /// The world.
    pub world: &'a World,
    /// Where the answers come from.
    pub backend: &'a dyn Backend,
    /// The ledger, for the confidence of each rule.
    pub ledger: &'a Ledger,
    /// What to run.
    pub selection: &'a Selection,
    /// The time of the report.
    pub now: &'a str,
    /// The directory the report goes to, or none.
    pub out_dir: Option<&'a str>,
    /// Progress notes.
    pub progress: &'a Progress,
    /// Print, after each check, what it ran, its answer, the time, and the verdict.
    pub verbose: bool,
    /// The color of the lines of `verbose`, which go to standard error.
    pub paint: Paint,
}

/// Run every applicable check, and assemble the report.
///
/// # Errors
///
/// [`Error::Input`] when the world has a `clusterUid`, and its `entity` or the `metadata` of its
/// `entity` is neither an object nor `null`: see [`World::to_value`]. A check that fails or cannot
/// run is a result in the report, and not an error.
pub fn run(inputs: RunInputs<'_>) -> Result<Report> {
    let jq = Jq::new();
    let world_value = inputs.world.to_value()?;
    let rules = applicable(
        inputs.catalog,
        inputs.world,
        &world_value,
        inputs.selection,
        &jq,
    );
    let mut outcomes = Vec::with_capacity(rules.len());
    for (rule, skip) in rules {
        let outcome = match skip {
            Some(o) => o,
            None => {
                inputs
                    .progress
                    .note(&format!("{}: {}", rule.effective_tier().as_str(), rule.id));
                let outcome = probe::run_check(rule, &world_value, inputs.backend, &jq);
                if inputs.verbose {
                    for line in verbose_lines(rule, &outcome, &inputs.paint) {
                        inputs.progress.note(&line);
                    }
                }
                outcome
            }
        };
        outcomes.push((rule, outcome));
    }
    let confidence = confidence_of(inputs.catalog, inputs.ledger, inputs.world, &outcomes);
    Ok(report::assemble(
        Assembly {
            catalog: inputs.catalog,
            outcomes,
            confidence,
            world: &world_value,
            subject: subject_of(inputs.world),
            options: Options {
                tiers: inputs.selection.tiers.clone(),
                strict: inputs.selection.strict,
                ready: inputs.selection.ready,
                out_dir: inputs.out_dir.map(str::to_string),
            },
            generated_at: inputs.now.to_string(),
        },
        &jq,
    ))
}

fn subject_of(world: &World) -> Subject {
    Subject {
        cloud: world.cloud.clone(),
        release: world.release.clone(),
        source: world.source.clone(),
        name: text(&world.entity, "/metadata/name"),
        cluster_uid: world.cluster_uid.clone(),
    }
}

/// The confidence of each rule with a check, on this engine and release, from the ledger.
fn confidence_of(
    catalog: &Catalog,
    ledger: &Ledger,
    world: &World,
    outcomes: &[(&Rule, Outcome)],
) -> BTreeMap<String, Confidence> {
    let mut out = BTreeMap::new();
    for (rule, _) in outcomes {
        if rule.check.is_none() || catalog.rule(&rule.id).is_none() {
            continue;
        }
        let counts = ledger.counts_for(&rule.id, &world.cloud, world.release.as_deref());
        if counts.caught + counts.missed + counts.passed + counts.false_fail == 0 {
            continue;
        }
        let bar = rule.promotion.as_ref().map_or(0.8, |p| p.bar);
        out.insert(
            rule.id.clone(),
            Confidence::of(
                &world.cloud,
                world.release.as_deref().unwrap_or(""),
                &counts,
                bar,
            ),
        );
    }
    out
}

#[cfg(test)]
mod tests;
