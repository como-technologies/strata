//! The run: which rules apply, how each one runs, and what the report holds. A run reads the
//! catalog once, keeps the rules of the engine, the release, and the tiers asked for, runs each
//! check, and assembles the report with the confidence of each rule from the ledger.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use strata::api::{Palette, PaletteApi, cluster_events_path, cluster_summary_path, subject_path};
use strata::json::{arr, str_at, text, text_or};
use strata::ops::{DELETED_STATE, ERROR_STATE, RUNNING_STATE};
use strata::snapshot::SubjectKind;
use strata::term::progress::Progress;
use strata::text::cut;

use crate::confidence::{Confidence, Ledger};
use crate::error::{Error, Result};
use crate::jq::Jq;
use crate::probe::{self, Backend, Outcome, Verdict};
use crate::report::{self, Assembly, Options, Report, Subject, paint};
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
                        format!("tier {} was not asked for", r.effective_tier().name()),
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
    /// Color the lines of `verbose`: stderr is a terminal, and nothing turned color off.
    pub color: bool,
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
                    .note(&format!("{}: {}", rule.effective_tier().name(), rule.id));
                let outcome = probe::run_check(rule, &world_value, inputs.backend, &jq);
                if inputs.verbose {
                    for line in verbose_lines(rule, &outcome, inputs.color) {
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
fn verbose_lines(rule: &Rule, o: &Outcome, color: bool) -> Vec<String> {
    let tier = rule.effective_tier().name();
    let mut lines: Vec<String> = o
        .ran
        .requests()
        .iter()
        .map(|c| format!("{tier}: {}: {}", rule.id, paint(color, "2", &c.line())))
        .collect();
    lines.push(verbose_line(rule, o, color));
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
fn verbose_line(rule: &Rule, o: &Outcome, color: bool) -> String {
    let mut line = format!(
        "{}: {}: {} {}",
        rule.effective_tier().name(),
        rule.id,
        paint(
            color,
            "2",
            &format!(
                "{} -> {}{} ms,",
                what_ran(o),
                o.ran.answer().map(|a| format!("{a}, ")).unwrap_or_default(),
                o.elapsed.as_millis()
            )
        ),
        label(rule, o, color)
    );
    if o.verdict != Verdict::Passed
        && let Some(m) = o.messages.first()
    {
        line.push_str(&format!(": {m}"));
    }
    line
}

/// The ANSI color of a cluster state: green when it runs, yellow while it builds, red when it
/// is in error or goes.
fn state_color(state: &str) -> &'static str {
    match state {
        RUNNING_STATE => "32",
        "Provisioning" | "Pending" | "Importing" => "33",
        ERROR_STATE | "Deleting" | DELETED_STATE => "31",
        _ => "0",
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
    color: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    let events = arr(read, "/events");
    let state = text_or(read, "/cluster/status/state", UNKNOWN);
    out.push(format!(
        "{} state {}, health {}, {} events",
        paint(
            color,
            "2",
            &format!(
                "watch: read GET {}, GET {}, GET {}:",
                subject_path(SubjectKind::Cluster, uid),
                cluster_summary_path(uid),
                cluster_events_path(uid)
            )
        ),
        paint(color, state_color(state), state),
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
            let status_color = match status {
                "True" => "32",
                "False" => "33",
                _ => "0",
            };
            out.push(format!(
                "watch: condition {kind}={} {reason}{}",
                paint(color, status_color, status),
                if message == UNKNOWN || message.is_empty() {
                    String::new()
                } else {
                    paint(color, "2", &format!(": {}", cut(message, 160)))
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
                paint(color, "36", at),
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
        out.push(paint(
            color,
            "2",
            &format!("watch: {older} older event(s) not shown; the report holds them"),
        ));
    }
    out.extend(fresh.into_iter().map(|(_, line)| line));
    out
}

/// One line of `--verbose` at a reading of the watch: the rule and its verdict, with the first
/// message when it did not pass. The time of a watch rule is the time of the reading, which the
/// line above it names, so no time here.
fn watch_line(rule: &Rule, o: &Outcome, color: bool) -> String {
    let mut line = format!("watch: {}: {}", rule.id, label(rule, o, color));
    if o.verdict != Verdict::Passed
        && let Some(m) = o.messages.first()
    {
        line.push_str(&format!(": {m}"));
    }
    line
}

/// The label of an outcome, in its color.
fn label(rule: &Rule, o: &Outcome, color: bool) -> String {
    paint(
        color,
        report::label_color_for(o.verdict, rule.severity, rule.level),
        &report::label_for(o.verdict, rule.level, rule.severity),
    )
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
    /// Color the lines of `verbose`.
    pub color: bool,
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
        color,
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
                progress.note(&paint(
                    color,
                    "2",
                    &format!(
                        "watch: {} reads the cluster, its summary, and its events with: {}",
                        rule.id,
                        cut(&probe::one_line(expr.clone()), 160)
                    ),
                ));
            }
        }
    }
    loop {
        let elapsed = started.elapsed();
        let read = reading(palette, &uid, elapsed)?;
        readings += 1;
        let state = text_or(&read, "/cluster/status/state", UNKNOWN).to_string();
        if verbose {
            for line in reading_lines(&uid, &read, &mut seen_conditions, &mut seen_events, color) {
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
            paint(color, "1", "watch:"),
            paint(color, state_color(&state), &state),
            elapsed.as_secs(),
            if failed {
                paint(color, "31", ", a rule failed")
            } else {
                String::new()
            }
        ));
        if verbose {
            for rule in &rules {
                if let Some(o) = last.get(&rule.id) {
                    progress.note(&watch_line(rule, o, color));
                }
            }
        }
        if failed && options.stop_at_first {
            final_state = state;
            break;
        }
        // a cluster that runs ends the watch: a failure it read on past is in the report
        if state == RUNNING_STATE {
            final_state = state;
            break;
        }
        if matches!(state.as_str(), DELETED_STATE | ERROR_STATE) {
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
            r.effective_tier().name(),
            r.level.name(),
            r.severity.name(),
            r.status.name(),
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
mod tests {
    use super::*;
    use crate::probe::FixtureBackend;
    use std::path::Path;

    const SECTION: &str = r#"
section: compute
title: Compute and pools
rules:
  - id: gke-pool-multiple-3
    title: A GKE worker pool size is a multiple of 3
    engines: [gke]
    tier: static
    status: seen
    what: w
    check:
      kind: static
      expr: '[.entity.spec.machinepoolconfig[] | select(.poolConfig.isControlPlane | not) | select(.poolConfig.size % 3 != 0) | "pool \(.poolConfig.name) has size \(.poolConfig.size)"]'
  - id: eks-one-zone
    title: EKS needs two zones
    engines: [eks]
    tier: static
    status: source
    what: w
    check: {kind: static, expr: "true"}
  - id: gke-api-off
    title: The Kubernetes Engine API is off
    engines: [gke]
    tier: cloud
    status: seen
    what: w
    reproduce: ["gcloud services list --enabled --project {{ .entity.spec.cloudConfig.project }}"]
  - id: old-rule
    title: A rule of an old release
    engines: [all]
    tier: static
    status: source
    releases: "<4.0"
    what: w
    check: {kind: static, expr: "false"}
  - id: provider-thing
    title: A rule of the provider
    engines: [all]
    tier: tool
    status: seen
    what: w
  - id: on-change-only
    title: A rule on a change
    engines: [all]
    tier: static
    status: seen
    what: w
    when: ".before != null"
    check: {kind: static, expr: "false"}
"#;

    fn catalog() -> Catalog {
        let mut c = Catalog::default();
        c.push(
            Catalog::parse_section(SECTION, Path::new("c.yaml")).unwrap(),
            Path::new("c.yaml"),
        )
        .unwrap();
        c
    }

    fn world(size: u64) -> World {
        World {
            cloud: "gke".into(),
            release: Some("4.10".into()),
            source: "c.json".into(),
            entity: json!({"metadata": {"name": "fred"}, "spec": {"cloudConfig": {"project": "p1"},
                "machinepoolconfig": [{"poolConfig": {"name": "worker-pool", "size": size, "isControlPlane": false}}]}}),
            ..World::default()
        }
    }

    #[test]
    fn a_run_keeps_the_rules_of_the_engine_and_says_why_the_others_do_not_run() {
        let c = catalog();
        let (backend, _) = FixtureBackend::split(json!({}));
        let report = run(RunInputs {
            catalog: &c,
            world: &world(4),
            backend: &backend,
            ledger: &Ledger::default(),
            selection: &Selection::default(),
            now: "2026-09-30T00:00:00Z",
            out_dir: None,
            progress: &Progress::silent(),
            verbose: false,
            color: false,
        })
        .unwrap();
        let by_id = |id: &str| report.checks.iter().find(|c| c.id == id).unwrap();
        assert!(
            report.checks.iter().all(|c| c.id != "eks-one-zone"),
            "another engine is not in the report"
        );
        assert_eq!(by_id("gke-pool-multiple-3").verdict, Verdict::Failed);
        assert_eq!(
            by_id("gke-pool-multiple-3").messages,
            vec!["pool worker-pool has size 4"]
        );
        assert_eq!(by_id("gke-api-off").verdict, Verdict::NotChecked);
        assert_eq!(
            by_id("gke-api-off").reproduce,
            vec!["gcloud services list --enabled --project p1"]
        );
        assert_eq!(by_id("old-rule").verdict, Verdict::Skipped);
        assert_eq!(by_id("provider-thing").verdict, Verdict::Skipped);
        assert_eq!(
            by_id("on-change-only").verdict,
            Verdict::Skipped,
            "no change in this world"
        );
        assert!(
            by_id("on-change-only").messages[0].contains("does not apply here"),
            "{:?}",
            by_id("on-change-only").messages
        );
        assert_eq!(
            report.summary.exit_code, 0,
            "a failed probe does not change the exit code"
        );
        assert_eq!(report.summary.probe_failed, 1);
        assert_eq!(report.subject.name, "fred");
    }

    #[test]
    fn strict_counts_a_failed_probe_and_a_clean_entity_passes() {
        let c = catalog();
        let (backend, _) = FixtureBackend::split(json!({}));
        let strict = Selection {
            strict: true,
            ..Selection::default()
        };
        let inputs = |w: &World| {
            run(RunInputs {
                catalog: &c,
                world: w,
                backend: &backend,
                ledger: &Ledger::default(),
                selection: &strict,
                now: "t",
                out_dir: None,
                progress: &Progress::silent(),
                verbose: false,
                color: false,
            })
            .unwrap()
        };
        assert_eq!(inputs(&world(4)).summary.exit_code, 1);
        let clean = inputs(&world(3));
        assert_eq!(clean.summary.exit_code, 0);
        assert_eq!(clean.summary.passed, 1);
    }

    #[test]
    fn a_tier_not_asked_for_is_not_run_and_ids_select() {
        let c = catalog();
        let (backend, _) = FixtureBackend::split(json!({}));
        let sel = Selection {
            tiers: vec![Tier::Palette],
            ..Selection::default()
        };
        let report = run(RunInputs {
            catalog: &c,
            world: &world(4),
            backend: &backend,
            ledger: &Ledger::default(),
            selection: &sel,
            now: "t",
            out_dir: None,
            progress: &Progress::silent(),
            verbose: false,
            color: false,
        })
        .unwrap();
        let pool = report
            .checks
            .iter()
            .find(|c| c.id == "gke-pool-multiple-3")
            .unwrap();
        assert_eq!(pool.verdict, Verdict::NotRun);
        assert!(pool.messages[0].contains("tier static was not asked for"));
        let sel = Selection {
            ids: vec!["gke-api-off".into()],
            ..Selection::default()
        };
        let report = run(RunInputs {
            catalog: &c,
            world: &world(4),
            backend: &backend,
            ledger: &Ledger::default(),
            selection: &sel,
            now: "t",
            out_dir: None,
            progress: &Progress::silent(),
            verbose: false,
            color: false,
        })
        .unwrap();
        assert_eq!(report.checks.len(), 1);
    }

    #[test]
    fn the_rules_listing_names_each_rule_of_the_engine() {
        let c = catalog();
        let rules = list_rules(&c, Some("eks"), None, &[]);
        assert_eq!(
            rules.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec![
                "eks-one-zone",
                "old-rule",
                "provider-thing",
                "on-change-only"
            ]
        );
        let text = rules_text(&c, &rules);
        assert!(text.contains("eks-one-zone"), "{text}");
        assert!(text.contains("4 rules: 3 with a check"), "{text}");
        assert_eq!(list_rules(&c, None, Some("4.10"), &[Tier::Static]).len(), 3);
    }

    #[test]
    fn a_watch_stops_at_the_first_counted_failure_with_the_evidence() {
        // the rule is a probe: it counts, and ends the watch, with --strict
        let section = r#"
section: platform
title: The Palette platform
rules:
  - id: status-not-cloud
    title: The status shows no error while the cloud refuses the build
    engines: [gke]
    tier: watch
    status: seen
    what: w
    check:
      kind: watch
      expr: '[.events[] | select(.reason == "ReconcileError" or (.message | test("InvalidArgument"))) | "event \(.reason): \(.message)"]'
"#;
        let mut c = Catalog::default();
        c.push(
            Catalog::parse_section(section, Path::new("p.yaml")).unwrap(),
            Path::new("p.yaml"),
        )
        .unwrap();
        let api = strata::api::MemoryApi::new()
            .with("/v1/spectroclusters/c1", json!({"metadata": {"uid": "c1"}, "spec": {"cloudType": "gke"}, "status": {"state": "Provisioning"}}))
            .with("/v1/events/components/spectrocluster/c1?limit=50", json!({"items": [{"reason": "GCPManagedControlPlaneReconcile", "message": "InvalidArgument: no release channel"}]}));
        let mut w = world(3);
        w.cluster_uid = Some("c1".into());
        let slept = std::cell::Cell::new(0);
        let (report, _) = watch(WatchInputs {
            catalog: &c,
            world: &w,
            palette: &Palette::new(&api),
            ledger: &Ledger::default(),
            selection: &Selection {
                strict: true,
                ..Selection::default()
            },
            options: &WatchOptions::default(),
            now: "t",
            progress: &Progress::silent(),
            verbose: false,
            color: false,
            sleep: &|_| slept.set(slept.get() + 1),
            out_dir: None,
        })
        .unwrap();
        assert_eq!(report.checks[0].verdict, Verdict::Failed);
        assert!(
            report.checks[0].messages[0].contains("InvalidArgument"),
            "{:?}",
            report.checks[0].messages
        );
        assert_eq!(
            slept.get(),
            0,
            "the first reading failed, so the watch did not wait"
        );
        assert!(
            report.subject.source.contains("Provisioning after"),
            "{}",
            report.subject.source
        );
        assert_eq!(report.options.tiers, vec![Tier::Watch]);
    }

    #[test]
    fn a_reading_has_no_summary_for_a_404_and_stops_at_a_401_or_a_429() {
        let cluster = json!({"metadata": {"uid": "c1"}, "status": {"state": "Provisioning"}});
        let summary = "/v1/dashboard/spectroclusters/c1";
        let events = "/v1/events/components/spectrocluster/c1?limit=50";
        let gone = strata::api::MemoryApi::new()
            .with("/v1/spectroclusters/c1", cluster.clone())
            .failing(events, 403, "{}");
        let read = reading(&Palette::new(&gone), "c1", Duration::from_secs(1)).unwrap();
        assert_eq!(read["summary"], Value::Null);
        assert_eq!(read["events"], json!([]));
        for status in [401, 429] {
            let refused = strata::api::MemoryApi::new()
                .with("/v1/spectroclusters/c1", cluster.clone())
                .failing(summary, status, "{}");
            let err = reading(&Palette::new(&refused), "c1", Duration::from_secs(1)).unwrap_err();
            assert!(err.to_string().contains(&format!("HTTP {status}")), "{err}");
        }
    }

    #[test]
    fn a_reading_of_a_cluster_with_no_event_has_an_empty_list_of_events() {
        let cluster = json!({"metadata": {"uid": "c1"}, "status": {"state": "Provisioning"}});
        // Palette gives `items: null` when the cluster has no event
        let api = strata::api::MemoryApi::new()
            .with("/v1/spectroclusters/c1", cluster)
            .with(
                "/v1/events/components/spectrocluster/c1?limit=50",
                json!({"items": null}),
            );
        let read = reading(&Palette::new(&api), "c1", Duration::from_secs(1)).unwrap();
        assert_eq!(read["events"], json!([]));
    }

    #[test]
    fn a_watch_ends_when_the_cluster_runs() {
        let section = r#"
section: platform
title: P
rules:
  - id: agent-egress
    title: t
    engines: [all]
    tier: watch
    status: source
    what: w
    check: {kind: watch, expr: "true"}
"#;
        let mut c = Catalog::default();
        c.push(
            Catalog::parse_section(section, Path::new("p.yaml")).unwrap(),
            Path::new("p.yaml"),
        )
        .unwrap();
        let api = strata::api::MemoryApi::new().with(
            "/v1/spectroclusters/c1",
            json!({"metadata": {"uid": "c1"}, "status": {"state": "Running"}}),
        );
        let mut w = world(3);
        w.cluster_uid = Some("c1".into());
        let (report, _) = watch(WatchInputs {
            catalog: &c,
            world: &w,
            palette: &Palette::new(&api),
            ledger: &Ledger::default(),
            selection: &Selection::default(),
            options: &WatchOptions::default(),
            now: "t",
            progress: &Progress::silent(),
            verbose: false,
            color: false,
            sleep: &|_| {},
            out_dir: None,
        })
        .unwrap();
        assert_eq!(report.checks[0].verdict, Verdict::Passed);
        assert_eq!(report.summary.exit_code, 0);
    }

    #[test]
    fn a_watch_that_reads_on_past_a_counted_failure_ends_when_the_cluster_runs() {
        let section = r#"
section: platform
title: P
rules:
  - id: agent-egress
    title: t
    engines: [all]
    tier: watch
    status: source
    what: w
    check: {kind: watch, expr: "false"}
"#;
        let mut c = Catalog::default();
        c.push(
            Catalog::parse_section(section, Path::new("p.yaml")).unwrap(),
            Path::new("p.yaml"),
        )
        .unwrap();
        let api = strata::api::MemoryApi::new().with(
            "/v1/spectroclusters/c1",
            json!({"metadata": {"uid": "c1"}, "status": {"state": "Running"}}),
        );
        let mut w = world(3);
        w.cluster_uid = Some("c1".into());
        let readings = std::cell::Cell::new(0);
        let (report, _) = watch(WatchInputs {
            catalog: &c,
            world: &w,
            palette: &Palette::new(&api),
            ledger: &Ledger::default(),
            selection: &Selection {
                strict: true,
                ..Selection::default()
            },
            options: &WatchOptions {
                stop_at_first: false,
                ..WatchOptions::default()
            },
            now: "t",
            progress: &Progress::silent(),
            verbose: false,
            color: false,
            sleep: &|_| readings.set(readings.get() + 1),
            out_dir: None,
        })
        .unwrap();
        assert_eq!(report.checks[0].verdict, Verdict::Failed);
        assert!(report.checks[0].counts_as_failure);
        assert_eq!(report.summary.exit_code, 1);
        assert_eq!(
            readings.get(),
            0,
            "the first reading saw Running, and the watch ended there"
        );
    }

    #[test]
    fn a_failed_probe_does_not_end_the_watch_and_is_in_the_report() {
        let section = r#"
section: platform
title: P
rules:
  - id: webhook-call-failed
    title: t
    engines: [all]
    tier: watch
    severity: warn
    status: seen
    what: w
    check: {kind: watch, expr: '[.events[] | select(.message | test("failed calling webhook")) | .message]'}
"#;
        let mut c = Catalog::default();
        c.push(
            Catalog::parse_section(section, Path::new("p.yaml")).unwrap(),
            Path::new("p.yaml"),
        )
        .unwrap();
        let api = strata::api::MemoryApi::new()
            .with(
                "/v1/spectroclusters/c1",
                json!({"metadata": {"uid": "c1"}, "status": {"state": "Running"}}),
            )
            .with(
                "/v1/events/components/spectrocluster/c1?limit=50",
                json!({"items": [{"reason": "Log", "message": "failed calling webhook \"vspectrocluster.kb.io\": connection refused", "metadata": {"creationTimestamp": "2026-09-29T17:59:22.272Z"}}]}),
            );
        let mut w = world(3);
        w.cluster_uid = Some("c1".into());
        let (report, reading) = watch(WatchInputs {
            catalog: &c,
            world: &w,
            palette: &Palette::new(&api),
            ledger: &Ledger::default(),
            selection: &Selection::default(),
            options: &WatchOptions::default(),
            now: "t",
            progress: &Progress::silent(),
            verbose: false,
            color: false,
            sleep: &|_| {},
            out_dir: None,
        })
        .unwrap();
        // the warning is in the report, the exit is 0, and the watch ended at Running
        assert_eq!(report.checks[0].verdict, Verdict::Failed);
        assert_eq!(report.summary.exit_code, 0);
        assert!(
            report.subject.source.contains("Running after"),
            "{}",
            report.subject.source
        );
        // the reading the rules saw is the world of --out
        assert_eq!(reading["cluster"]["status"]["state"], "Running");
        assert_eq!(reading["events"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn a_check_with_each_prints_a_line_for_each_request_and_the_count_of_requests_on_its_line() {
        let section = r#"
section: compute
title: Compute and pools
rules:
  - id: edge-host-in-use
    title: Each edge host is free
    engines: [edge]
    tier: palette
    status: source
    what: w
    check:
      kind: palette
      each: '[.hosts[]]'
      path: /v1/edgehosts/{{ $item }}
      expr: '[.responses[] | select(.response.ok | not) | "edge host \(.item) is not there"]'
"#;
        let mut c = Catalog::default();
        c.push(
            Catalog::parse_section(section, Path::new("e.yaml")).unwrap(),
            Path::new("e.yaml"),
        )
        .unwrap();
        let (backend, world) = FixtureBackend::split(json!({"hosts": ["h1", "h2"],
            "_responses": {"GET /v1/edgehosts/h1": {"ok": true, "status": 200, "body": {}},
                "GET /v1/edgehosts/h2": {"ok": false, "status": 404, "body": {}}}}));
        let rule = c.rule("edge-host-in-use").unwrap();
        let outcome = probe::run_check(rule, &world, &backend, &Jq::new());
        let lines = verbose_lines(rule, &outcome, false);
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert_eq!(
            lines[..2],
            [
                "palette: edge-host-in-use: GET /v1/edgehosts/h1 -> HTTP 200",
                "palette: edge-host-in-use: GET /v1/edgehosts/h2 -> HTTP 404",
            ]
        );
        assert!(
            lines[2].starts_with("palette: edge-host-in-use: 2 requests -> ")
                && lines[2].ends_with(" ms, PROBE FAIL: edge host h2 is not there"),
            "{}",
            lines[2]
        );
    }
}
