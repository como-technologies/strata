//! The run: which rules apply, how each one runs, and what the report holds. A run reads the
//! catalog once, keeps the rules of the engine, the release, and the tiers asked for, runs each
//! check, and assembles the report with the confidence of each rule from the ledger.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use strata::api::{Palette, PaletteApi, cluster_events_path, cluster_summary_path, subject_path};
use strata::closed_set::ClosedSet;
use strata::cluster_state::ClusterState;
use strata::json::{arr, str_at, text, text_or};
use strata::snapshot::SubjectKind;
use strata::term::color::Paint;
use strata::term::progress::Progress;
use strata::text::cut;

use crate::confidence::{Confidence, Ledger};
use crate::error::{Error, Result};
use crate::jq::Jq;
use crate::probe::{self, Backend, Outcome, Verdict};
use crate::report::label::LabelKind;
use crate::report::{self, Assembly, Options, Report, Subject, Tool};
use crate::rule::{Catalog, Check, Level, Rule, Tier};
use crate::world::{World, unreadable};

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
        .filter(|r| selection.ids.is_empty() || selection.ids.iter().any(|id| id == &r.id))
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
pub fn run(inputs: RunInputs<'_>) -> Result<Report> {
    let jq = Jq::new();
    let world_value = inputs.world.to_value();
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
            every: Duration::from_secs(30),
            limit: Duration::from_secs(3600),
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
        Err(strata::Error::Api { status, .. }) if unreadable(status) => Value::Null,
        Err(e) => return Err(e.into()),
    };
    let events = match palette.cluster_events(uid) {
        Ok(e) => Value::Array(e),
        Err(strata::Error::Api { status, .. }) if unreadable(status) => Value::Array(vec![]),
        Err(e) => return Err(e.into()),
    };
    Ok(json!({
        "cluster": cluster,
        "summary": summary,
        "events": events,
        "elapsed": elapsed.as_secs(),
    }))
}

/// The lines of `--verbose` for one check: one line for each request of a check with `each`, then
/// the line of the check.
fn verbose_lines(rule: &Rule, o: &Outcome, paint: &Paint) -> Vec<String> {
    let tier = rule.effective_tier().as_str();
    let mut lines: Vec<String> = o
        .ran
        .requests()
        .iter()
        .map(|c| format!("{tier}: {}: {}", rule.id, paint.dim(&c.line())))
        .collect();
    lines.push(verbose_line(rule, o, paint));
    lines
}

/// What a check ran, for its line of `--verbose`: its request or command, the count of the requests
/// of a check with `each`, or the program alone.
fn what_ran(o: &Outcome) -> String {
    match (o.ran.text(), o.ran.requests().len()) {
        (Some(ran), _) => ran,
        (None, 0) => "jq over the world".to_string(),
        (None, 1) => "1 request".to_string(),
        (None, n) => format!("{n} requests"),
    }
}

/// One line of `--verbose`: what the check ran, its answer, the time, and the verdict, with the
/// first message when it did not pass.
fn verbose_line(rule: &Rule, o: &Outcome, paint: &Paint) -> String {
    let mut line = format!(
        "{}: {}: {} {}",
        rule.effective_tier().as_str(),
        rule.id,
        paint.dim(&format!(
            "{} -> {}{} ms,",
            what_ran(o),
            o.ran.answer().map(|a| format!("{a}, ")).unwrap_or_default(),
            o.elapsed.as_millis()
        )),
        label(rule, o, paint)
    );
    if o.verdict != Verdict::Passed
        && let Some(m) = o.messages.first()
    {
        line.push_str(&format!(": {m}"));
    }
    line
}

/// `state` in the color of a cluster state: green when it runs, yellow while it builds, red when
/// it is in error or goes. A state that strata does not know has no color.
fn paint_state(paint: &Paint, state: &str) -> String {
    match ClusterState::parse(state) {
        Some(ClusterState::Running) => paint.green(state),
        Some(ClusterState::Provisioning | ClusterState::Pending | ClusterState::Importing) => {
            paint.yellow(state)
        }
        Some(ClusterState::Error | ClusterState::Deleting | ClusterState::Deleted) => {
            paint.red(state)
        }
        None => state.to_string(),
    }
}

/// What a line of `--verbose` shows for a field that the reading does not have.
const UNKNOWN: &str = "?";

/// The lines of `--verbose` for one reading: the three requests it made and what they gave, then
/// each condition that changed since the reading before, then each event new since it, the
/// newest 15 at most.
fn reading_lines(
    uid: &str,
    read: &Value,
    seen_conditions: &mut BTreeMap<String, String>,
    seen_events: &mut BTreeSet<String>,
    paint: &Paint,
) -> Vec<String> {
    let mut out = Vec::new();
    let events = arr(read, "/events");
    let state = text_or(read, "/cluster/status/state", UNKNOWN);
    out.push(format!(
        "{} state {}, health {}, {} events",
        paint.dim(&format!(
            "watch: read GET {}, GET {}, GET {}:",
            subject_path(SubjectKind::Cluster, uid),
            cluster_summary_path(uid),
            cluster_events_path(uid)
        )),
        paint_state(paint, state),
        text_or(read, "/summary/status/health/state", UNKNOWN),
        events.len()
    ));
    for c in arr(read, "/cluster/status/conditions") {
        let kind = text_or(c, "/type", UNKNOWN);
        let status = text_or(c, "/status", UNKNOWN);
        let reason = text_or(c, "/reason", UNKNOWN);
        let now = format!("{status} {reason}");
        if seen_conditions
            .insert(kind.to_string(), now.clone())
            .as_deref()
            != Some(now.as_str())
        {
            let message = text_or(c, "/message", UNKNOWN);
            let status_text = match status {
                "True" => paint.green(status),
                "False" => paint.yellow(status),
                _ => status.to_string(),
            };
            out.push(format!(
                "watch: condition {kind}={status_text} {reason}{}",
                if message == UNKNOWN || message.is_empty() {
                    String::new()
                } else {
                    paint.dim(&format!(": {}", cut(message, 160)))
                }
            ));
        }
    }
    let mut fresh: Vec<(String, String)> = events
        .iter()
        .map(|e| {
            let at = text_or(e, "/metadata/creationTimestamp", UNKNOWN);
            let reason = str_at(e, "/reason");
            let message = text_or(e, "/message", UNKNOWN);
            let key = format!("{at} {reason} {message}");
            let line = format!(
                "watch: event {}{}: {}",
                paint.cyan(at),
                if reason.is_empty() {
                    String::new()
                } else {
                    format!(" {reason}")
                },
                cut(&probe::one_line(message.to_string()), 200)
            );
            (key, line)
        })
        .filter(|(key, _)| seen_events.insert(key.clone()))
        .collect();
    fresh.sort();
    if fresh.len() > 15 {
        let older = fresh.len() - 15;
        fresh.drain(..older);
        out.push(paint.dim(&format!(
            "watch: {older} older event(s) not shown; the report holds them"
        )));
    }
    out.extend(fresh.into_iter().map(|(_, line)| line));
    out
}

/// One line of `--verbose` at a reading of the watch: the rule and its verdict, with the first
/// message when it did not pass. The time of a watch rule is the time of the reading, which the
/// line above it names, so no time here.
fn watch_line(rule: &Rule, o: &Outcome, paint: &Paint) -> String {
    let mut line = format!("watch: {}: {}", rule.id, label(rule, o, paint));
    if o.verdict != Verdict::Passed
        && let Some(m) = o.messages.first()
    {
        line.push_str(&format!(": {m}"));
    }
    line
}

/// The label of an outcome, in its color.
fn label(rule: &Rule, o: &Outcome, paint: &Paint) -> String {
    let kind = LabelKind::of(o.verdict, rule.level, rule.severity);
    kind.paint(paint, &kind.word())
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
    /// Print, at each reading, the verdict of each rule.
    pub verbose: bool,
    /// The color of the lines of `verbose`, which go to standard error.
    pub paint: Paint,
}

/// Watch a build: every `every`, read the cluster, and judge each watch rule. The watch ends at
/// the first counted failure (severity `fail` at the preflight level, or any failed probe with
/// `--strict`), when the cluster is `Running` with each counted rule passed, or at the limit. A
/// failed probe or a warning is in the report, and the watch reads on. The second value is the
/// world with the last reading in it, for `--out world.json`.
pub fn watch<A: PaletteApi>(inputs: WatchInputs<'_, A>) -> Result<(Report, Value)> {
    let WatchInputs {
        catalog,
        world,
        palette,
        ledger,
        selection,
        options,
        now,
        progress,
        sleep,
        out_dir,
        verbose,
        paint,
    } = inputs;
    let uid = world.cluster_uid.clone().ok_or_else(|| Error::Usage {
        message:
            "--watch needs cluster:UID: the watch reads the conditions and the events of a cluster"
                .to_string(),
    })?;
    let jq = Jq::new();
    let base = world.to_value();
    let rules: Vec<&Rule> = catalog
        .rules()
        .filter(|r| r.applies_to(&world.cloud) && r.applies_to_release(world.release.as_deref()))
        .filter(|r| selection.ids.is_empty() || selection.ids.iter().any(|id| id == &r.id))
        .filter(|r| matches!(r.check, Some(Check::Watch { .. })))
        .filter(|r| r.when.as_deref().is_none_or(|w| holds(&jq, r, w, &base)))
        .collect();
    let started = Instant::now();
    let mut last: BTreeMap<String, Outcome> = BTreeMap::new();
    let mut readings = 0u32;
    let final_state: String;
    let mut last_reading: Value;
    // for --verbose: the conditions and the events seen at the reading before, so each reading
    // prints what changed
    let mut seen_conditions: BTreeMap<String, String> = BTreeMap::new();
    let mut seen_events: BTreeSet<String> = BTreeSet::new();
    if verbose {
        for rule in &rules {
            if let Some(Check::Watch { expr }) = &rule.check {
                progress.note(&paint.dim(&format!(
                    "watch: {} reads the cluster, its summary, and its events with: {}",
                    rule.id,
                    cut(&probe::one_line(expr.clone()), 160)
                )));
            }
        }
    }
    loop {
        let elapsed = started.elapsed();
        let read = reading(palette, &uid, elapsed)?;
        readings += 1;
        let state = text_or(&read, "/cluster/status/state", UNKNOWN).to_string();
        if verbose {
            for line in reading_lines(&uid, &read, &mut seen_conditions, &mut seen_events, &paint) {
                progress.note(&line);
            }
        }
        let mut with = base.clone();
        for (k, v) in read.as_object().into_iter().flatten() {
            with[k] = v.clone();
        }
        let mut failed = false;
        for rule in &rules {
            let mut o = probe::judge_watch(rule, &with, &jq);
            o.elapsed = elapsed;
            // a failed rule ends the watch when its failure counts, as the exit code counts it
            if report::counts_as_failure(rule, o.verdict, selection.strict) {
                failed = true;
            }
            last.insert(rule.id.clone(), o);
        }
        last_reading = with;
        progress.note(&format!(
            "{} {} after {}s, reading {readings}{}",
            paint.bold("watch:"),
            paint_state(&paint, &state),
            elapsed.as_secs(),
            if failed {
                paint.red(", a rule failed")
            } else {
                String::new()
            }
        ));
        if verbose {
            for rule in &rules {
                if let Some(o) = last.get(&rule.id) {
                    progress.note(&watch_line(rule, o, &paint));
                }
            }
        }
        if failed && options.stop_at_first {
            final_state = state;
            break;
        }
        // a cluster that runs ends the watch: a failure it read on past is in the report
        if ClusterState::Running.is(&state) {
            final_state = state;
            break;
        }
        if ClusterState::Deleted.is(&state) || ClusterState::Error.is(&state) {
            final_state = state;
            break;
        }
        if elapsed >= options.limit {
            for o in last.values_mut() {
                if o.verdict == Verdict::Passed {
                    *o = Outcome::not_run(
                        Verdict::Error,
                        format!(
                            "the watch reached its limit of {}s with the cluster {state}",
                            options.limit.as_secs()
                        ),
                    );
                }
            }
            final_state = state;
            break;
        }
        sleep(options.every);
    }
    let outcomes: Vec<(&Rule, Outcome)> = rules
        .iter()
        .map(|r| {
            (
                *r,
                last.remove(&r.id)
                    .unwrap_or_else(|| Outcome::not_run(Verdict::Skipped, "no reading")),
            )
        })
        .collect();
    let confidence = confidence_of(catalog, ledger, world, &outcomes);
    let mut subject = subject_of(world);
    subject.source = format!(
        "{} ({final_state} after {}s, {readings} reading(s))",
        subject.source,
        started.elapsed().as_secs()
    );
    let report = report::assemble(
        Assembly {
            catalog,
            outcomes,
            confidence,
            world: &base,
            subject,
            options: Options {
                tiers: vec![Tier::Watch],
                strict: selection.strict,
                ready: None,
                out_dir: out_dir.map(str::to_string),
            },
            generated_at: now.to_string(),
        },
        &jq,
    );
    Ok((report, last_reading))
}

/// The rules as `--rules` lists them: each rule that applies to the engine, when one is given.
pub fn list_rules(
    catalog: &Catalog,
    cloud: Option<&str>,
    release: Option<&str>,
    tiers: &[Tier],
) -> Vec<Rule> {
    catalog
        .rules()
        .filter(|r| cloud.is_none_or(|c| r.applies_to(c)))
        .filter(|r| r.applies_to_release(release))
        .filter(|r| tiers.is_empty() || tiers.contains(&r.effective_tier()))
        .cloned()
        .collect()
}

/// The schema id of the `--rules` listing as JSON.
pub const RULES_SCHEMA: &str = "strata-preflight-rules/1";

/// The `--rules` listing as a document: the sections, and the `rules` that
/// [`list_rules`] gave for `cloud` and `release`.
pub fn rules_document(
    catalog: &Catalog,
    cloud: Option<&str>,
    release: Option<&str>,
    rules: &[Rule],
) -> Value {
    json!({
        "schema": RULES_SCHEMA,
        "tool": Tool::current(),
        "cloud": cloud,
        "release": release,
        "sections": catalog
            .sections
            .iter()
            .map(|s| json!({"section": s.section, "title": s.title, "about": s.about}))
            .collect::<Vec<_>>(),
        "rules": rules,
    })
}

/// The `--rules` listing as text: one line for each rule.
pub fn rules_text(catalog: &Catalog, rules: &[Rule]) -> String {
    let mut out = String::new();
    let mut section = "";
    for r in rules {
        if r.section != section {
            section = &r.section;
            let title = catalog.section_of(r).map_or(section, |s| s.title.as_str());
            out.push_str(&format!("\n{title}\n"));
        }
        out.push_str(&format!(
            "  {:<32} {:<8} {:<10} {:<9} {:<9} {:<22} {}\n",
            r.id,
            r.effective_tier().as_str(),
            r.level.as_str(),
            r.severity.as_str(),
            r.status.as_str(),
            r.engines.join(","),
            r.title
        ));
    }
    let probes = rules.iter().filter(|r| r.level == Level::Probe).count();
    let checks = rules.iter().filter(|r| r.check.is_some()).count();
    out.push_str(&format!(
        "\n{} rules: {} with a check, {} preflight checks, {probes} probes\n",
        rules.len(),
        checks,
        rules.len() - probes
    ));
    out
}

#[cfg(test)]
mod tests;
