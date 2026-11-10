//! The watch: every few seconds, read a cluster that builds, and judge each watch rule on the
//! reading.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use strata::api::{Palette, PaletteApi};
use strata::json::{set_at, text_or, value_at};
use strata::model::cluster_state::ClusterState;
use strata::render::{count_of, paint_state};
use strata::snapshot::SubjectKind;
use strata::term::color::Paint;
use strata::term::progress::Progress;

use super::verbose::{Seen, UNKNOWN, program_lines, reading_lines, watch_line};
use super::{Selection, confidence_of, holds, subject_of};
use crate::confidence::Ledger;
use crate::error::{Error, Result};
use crate::jq::Jq;
use crate::probe::{self, Outcome, Reader, Verdict, is_answer};
use crate::report::{self, Assembly, Options, Report};
use crate::rule::{Catalog, Check, Rule, Tier};
use crate::world::World;

/// The seconds between two readings of a watch, when the run gives none.
pub const DEFAULT_EVERY_SECS: u64 = 30;

/// The longest watch in seconds, when the run gives none.
pub const DEFAULT_LIMIT_SECS: u64 = 3600;

/// The settings of a watch.
#[derive(Debug, Clone)]
pub struct WatchOptions {
    /// Seconds between two readings.
    pub every: Duration,
    /// The longest watch. The watch ends with `ERROR` at the limit, not with a verdict.
    pub limit: Duration,
    /// Stop at the first counted failure, or read on until the cluster is `Running` or the limit.
    pub stop_at_first: bool,
}

impl Default for WatchOptions {
    fn default() -> Self {
        WatchOptions {
            every: Duration::from_secs(DEFAULT_EVERY_SECS),
            limit: Duration::from_secs(DEFAULT_LIMIT_SECS),
            stop_at_first: true,
        }
    }
}

/// One reading of a cluster during a build.
///
/// The summary is `null`, and the events are empty, after each 4xx but 401 and 429. A 401 or a 429
/// stops the watch with the error that names it, as a read of the world does.
///
/// # Errors
///
/// [`Error::Palette`] with the error of the request for the cluster document, also a 404. The error
/// of the summary or of the events, when it is a 401, a 429, or not a 4xx.
pub fn reading<A: PaletteApi>(palette: &Palette<A>, uid: &str, elapsed: Duration) -> Result<Value> {
    let cluster = palette.subject_document(SubjectKind::Cluster, uid)?;
    let summary = match palette.fetch_cluster_summary(uid) {
        Ok(s) => s,
        Err(strata::Error::Api { status, .. }) if is_answer(status, Reader::World) => Value::Null,
        Err(e) => return Err(e.into()),
    };
    let events = match palette.cluster_events(uid) {
        Ok(e) => Value::Array(e),
        Err(strata::Error::Api { status, .. }) if is_answer(status, Reader::World) => {
            Value::Array(vec![])
        }
        Err(e) => return Err(e.into()),
    };
    Ok(json!({
        "cluster": cluster,
        "summary": summary,
        "events": events,
        "elapsed": elapsed.as_secs(),
    }))
}

/// The inputs of a watch.
pub struct WatchInputs<'a, A: PaletteApi> {
    /// The catalog, with any probes file.
    pub catalog: &'a Catalog,
    /// The world of the cluster.
    pub world: &'a World,
    /// The Palette client.
    pub palette: &'a Palette<A>,
    /// The ledger, for the confidence of each rule.
    pub ledger: &'a Ledger,
    /// What to run.
    pub selection: &'a Selection,
    /// How often, and how long.
    pub options: &'a WatchOptions,
    /// The time of the report.
    pub now: &'a str,
    /// Progress notes.
    pub progress: &'a Progress,
    /// Waits between two readings. A test passes one that does not wait.
    pub sleep: &'a dyn Fn(Duration),
    /// The directory of `--out`, for the reproduce lines and the report.
    pub out_dir: Option<&'a str>,
    /// True when `out_dir` gets `world.json` and `entity.json` too: `--world`.
    pub out_world: bool,
    /// Print, at each reading, the verdict of each rule.
    pub verbose: bool,
    /// The color of the lines of `verbose`, which go to standard error.
    pub paint: Paint,
}

/// What a watch kept when it ended.
struct Ended {
    /// The last outcome of each watch rule, by id.
    last: BTreeMap<String, Outcome>,
    /// How many readings the watch made.
    readings: u32,
    /// The state of the cluster at the last reading.
    state: String,
    /// True when the last reading shows a cluster that Palette deleted.
    gone: bool,
    /// The world with the last reading in it.
    world: Value,
}

/// Watch a build: every `every`, read the cluster, and judge each watch rule.
///
/// The watch ends at the first counted failure (severity `fail` at the preflight level, or any
/// failed probe with `--strict`). It also ends when the cluster is `Running` with each counted
/// rule passed, or at the limit. A failed probe or a warning is in the report, and the watch reads
/// on. The second value is the world with the last reading in it, for `world.json` of `--world`.
///
/// # Errors
///
/// [`Error::Usage`] when the world names no cluster. [`Error::Input`] when the world cannot give its
/// document: see [`World::to_value`]. The error of the first reading that fails: see
/// [`Error::Palette`].
pub fn watch<A: PaletteApi>(inputs: WatchInputs<'_, A>) -> Result<(Report, Value)> {
    let uid =
        inputs.world.cluster_uid.clone().ok_or_else(|| {
            Error::Usage {
        message:
            "--watch needs cluster:UID: the watch reads the conditions and the events of a cluster"
                .to_string(),
    }
        })?;
    let jq = Jq::new();
    let base = inputs.world.to_value()?;
    let rules = watch_rules(&inputs, &jq, &base);
    let started = Instant::now();
    if inputs.verbose {
        for line in program_lines(&rules, &inputs.paint) {
            inputs.progress.note(&line);
        }
    }
    let mut ended = read_until_ended(&inputs, &uid, &rules, &jq, &base, started)?;
    let outcomes: Vec<(&Rule, Outcome)> = rules
        .iter()
        .map(|r| {
            (
                *r,
                ended
                    .last
                    .remove(&r.id)
                    .unwrap_or_else(|| Outcome::not_run(Verdict::Skipped, "no reading")),
            )
        })
        .collect();
    let confidence = confidence_of(inputs.catalog, inputs.ledger, inputs.world, &outcomes);
    let mut subject = subject_of(inputs.world);
    subject.source = format!(
        "{} ({} after {}s, {})",
        subject.source,
        ended.state,
        started.elapsed().as_secs(),
        count_of(ended.readings as usize, "reading")
    );
    let report = report::assemble(
        Assembly {
            catalog: inputs.catalog,
            outcomes,
            confidence,
            world: &base,
            subject,
            options: Options {
                tiers: vec![Tier::Watch],
                strict: inputs.selection.strict,
                offline: false,
                ready: None,
                out_dir: inputs.out_dir.map(str::to_string),
                out_world: inputs.out_world,
            },
            cluster_gone: ended.gone,
            generated_at: inputs.now.to_string(),
        },
        &jq,
    );
    Ok((report, ended.world))
}

/// The watch rules that apply to the cluster: the rules of its engine and release that `--rule`
/// keeps and whose `when` holds on `base`.
fn watch_rules<'a, A: PaletteApi>(
    inputs: &WatchInputs<'a, A>,
    jq: &Jq,
    base: &Value,
) -> Vec<&'a Rule> {
    let world = inputs.world;
    inputs
        .catalog
        .rules()
        .filter(|r| r.applies_to(&world.cloud) && r.applies_to_release(world.release.as_deref()))
        .filter(|r| inputs.selection.keeps(&r.id))
        .filter(|r| matches!(r.check, Some(Check::Watch { .. })))
        .filter(|r| r.when.as_deref().is_none_or(|w| holds(jq, r, w, base)))
        .collect()
}

/// Read the cluster, judge the rules on each reading, and wait, until the watch ends: see
/// [`watch`]. `started` is the time of the first reading.
fn read_until_ended<A: PaletteApi>(
    inputs: &WatchInputs<'_, A>,
    uid: &str,
    rules: &[&Rule],
    jq: &Jq,
    base: &Value,
    started: Instant,
) -> Result<Ended> {
    let options = inputs.options;
    let mut last: BTreeMap<String, Outcome> = BTreeMap::new();
    let mut readings = 0u32;
    let mut seen = Seen::default();
    loop {
        let elapsed = started.elapsed();
        let read = reading(inputs.palette, uid, elapsed)?;
        readings += 1;
        let state = text_or(&read, "/cluster/status/state", UNKNOWN).to_string();
        if inputs.verbose {
            for line in reading_lines(uid, &read, &mut seen, &inputs.paint) {
                inputs.progress.note(&line);
            }
        }
        let mut world = base.clone();
        for (k, v) in read.as_object().into_iter().flatten() {
            set_at(&mut world, &[k.as_str()], v.clone());
        }
        let failed = judge_rules(rules, &world, jq, elapsed, inputs.selection, &mut last);
        note_reading(inputs, &state, elapsed, readings, failed);
        if inputs.verbose {
            for rule in rules {
                if let Some(o) = last.get(&rule.id) {
                    inputs.progress.note(&watch_line(rule, o, &inputs.paint));
                }
            }
        }
        // a cluster that runs ends the watch: a failure it read on past is in the report
        let gone = ClusterState::is_gone(value_at(&read, "/cluster"));
        let ends = (failed && options.stop_at_first)
            || ClusterState::Running.is(&state)
            || gone
            || ClusterState::Error.is(&state);
        let at_limit = !ends && elapsed >= options.limit;
        if at_limit {
            end_at_limit(&mut last, &state, options.limit);
        }
        if ends || at_limit {
            return Ok(Ended {
                last,
                readings,
                state,
                gone,
                world,
            });
        }
        (inputs.sleep)(options.every);
    }
}

/// Judge each watch rule on `world`, and keep the outcome of each in `last`. True when a rule
/// failed in a way that counts, as the exit code counts it.
fn judge_rules(
    rules: &[&Rule],
    world: &Value,
    jq: &Jq,
    elapsed: Duration,
    selection: &Selection,
    last: &mut BTreeMap<String, Outcome>,
) -> bool {
    let mut failed = false;
    for rule in rules {
        let mut o = probe::judge_watch(rule, world, jq);
        o.elapsed = elapsed;
        if report::counts_as_failure(rule, o.verdict, selection.strict) {
            failed = true;
        }
        last.insert(rule.id.clone(), o);
    }
    failed
}

/// Tell the state of the cluster at a reading, and whether a rule failed.
fn note_reading<A: PaletteApi>(
    inputs: &WatchInputs<'_, A>,
    state: &str,
    elapsed: Duration,
    readings: u32,
    failed: bool,
) {
    let paint = &inputs.paint;
    inputs.progress.note(&format!(
        "{} {} after {}s, reading {readings}{}",
        paint.bold("watch:"),
        paint_state(paint, state),
        elapsed.as_secs(),
        if failed {
            paint.red(", a rule failed")
        } else {
            String::new()
        }
    ));
}

/// A rule that passed becomes an error when the watch reaches its limit: the cluster did not
/// settle.
fn end_at_limit(last: &mut BTreeMap<String, Outcome>, state: &str, limit: Duration) {
    for o in last.values_mut() {
        if o.verdict == Verdict::Passed {
            *o = Outcome::not_run(
                Verdict::Error,
                format!(
                    "the watch reached its limit of {}s with the cluster {state}",
                    limit.as_secs()
                ),
            );
        }
    }
}
