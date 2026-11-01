//! Confidence is a count of runs, never an estimate.
//!
//! For each rule, per engine and Palette release, the ledger holds the runs that made the fault on
//! purpose and the runs of a clean build, and what the check answered in each. From the counts
//! come two rates and their 95% lower bounds (Clopper-Pearson, one side):
//!
//! - **catch rate**: of the runs that made the fault, the runs in which the check found it;
//! - **clean-pass rate**: of the runs of a clean build, the runs in which the check passed.
//!
//! A probe becomes a preflight check when both bounds reach the bar, `0.8` today. With no wrong
//! result, that is 14 correct runs of each kind, because `0.05^(1/14) = 0.807`.
//!
//! When a report names a Palette release, only the runs of that release count. A run of an older
//! release, and a run with no release, count for nothing, and the report says how many there are.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strata::model::cloud::CLOUD_ALL;
use strata::reference::version;

use crate::error::{Error, Result};
use crate::report::percent;
use crate::rule::{Catalog, Level, Rule, Status, Tier};
use crate::world::Engine;

/// The schema id of a ledger file.
pub const LEDGER_SCHEMA: &str = "strata-preflight-ledger/1";

/// What a form of the report writes where no run of the ledger gives a rate, a bound, or a
/// confidence.
pub const NO_RUNS: &str = "no runs";

/// What one check answered in one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RunResult {
    /// The run made the fault, and the check found it.
    Caught,
    /// The run made the fault, and the check passed. A wrong result.
    Missed,
    /// A clean build, and the check passed.
    Passed,
    /// A clean build, and the check failed. A wrong result.
    FalseFail,
}

/// One rule in one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuleRun {
    /// The rule id.
    pub rule: String,
    /// What the check answered.
    pub result: RunResult,
    /// A note: what the run made, or why the result is what it is.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// One run of the test plan: one engine, one Palette release, and each rule it ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Run {
    /// The run id: the UTC time of the run directory, `20260930T120000Z`.
    pub run: String,
    /// The engine the run built on.
    pub engine: String,
    /// The Palette release, as `4.10`, or empty when unknown.
    #[serde(default)]
    pub release: String,
    /// Each rule the run answered.
    pub results: Vec<RuleRun>,
}

/// The ledger: every run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Ledger {
    /// The schema id.
    #[serde(default = "ledger_schema")]
    pub schema: String,
    /// The runs, oldest first.
    #[serde(default)]
    pub runs: Vec<Run>,
}

fn ledger_schema() -> String {
    LEDGER_SCHEMA.to_string()
}

impl Ledger {
    /// Parse a ledger.
    ///
    /// # Errors
    ///
    /// [`Error::Ledger`] when `text` is not a ledger, or its schema is not the schema that this
    /// tool reads. `path` names the file in the message.
    pub fn parse(text: &str, path: &Path) -> Result<Ledger> {
        let ledger: Ledger = serde_yaml::from_str(text).map_err(|e| Error::Ledger {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        if ledger.schema != LEDGER_SCHEMA {
            return Err(Error::Ledger {
                path: path.to_path_buf(),
                message: format!(
                    "schema is '{}', and this tool reads {LEDGER_SCHEMA}",
                    ledger.schema
                ),
            });
        }
        Ok(ledger)
    }

    /// Read a ledger file.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when strata-preflight cannot read the file. The errors of [`Ledger::parse`].
    pub fn load(path: &Path) -> Result<Ledger> {
        let text = std::fs::read_to_string(path).map_err(|e| crate::error::io(path, e))?;
        Self::parse(&text, path)
    }

    /// Add the runs of another ledger.
    pub fn extend(&mut self, other: Ledger) {
        self.runs.extend(other.runs);
    }

    /// The counts of one rule, per engine and release.
    pub fn counts(&self, rule: &str) -> BTreeMap<(String, String), Counts> {
        let mut out: BTreeMap<(String, String), Counts> = BTreeMap::new();
        for run in &self.runs {
            for r in run.results.iter().filter(|r| r.rule == rule) {
                let c = out
                    .entry((run.engine.clone(), run.release.clone()))
                    .or_default();
                c.runs.push(run.run.clone());
                match r.result {
                    RunResult::Caught => c.caught += 1,
                    RunResult::Missed => c.missed += 1,
                    RunResult::Passed => c.passed += 1,
                    RunResult::FalseFail => c.false_fail += 1,
                }
            }
        }
        out
    }

    /// The counts of one rule on one engine. With `release`, only the runs of that release count.
    /// With none, every run counts.
    ///
    /// A run of an older release, and a run with no release, are in the `older` runs of the
    /// counts. A run of a later release is in no count.
    pub fn counts_for(&self, rule: &str, engine: &str, release: Option<&str>) -> Counts {
        let mut total = Counts::default();
        for ((e, r), c) in self.counts(rule) {
            if e != engine {
                continue;
            }
            match release.map(|want| standing(&r, want)) {
                None | Some(Standing::Same) => {
                    total.caught += c.caught;
                    total.missed += c.missed;
                    total.passed += c.passed;
                    total.false_fail += c.false_fail;
                    total.runs.extend(c.runs);
                }
                Some(Standing::Older) => total.older.extend(c.runs),
                Some(Standing::Later) => {}
            }
        }
        for ids in [&mut total.runs, &mut total.older] {
            ids.sort();
            ids.dedup();
        }
        total
    }
}

/// Where the release of a run stands against the release that a report names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Standing {
    /// The same release: a patch does not make another release.
    Same,
    /// An older release, or no release.
    Older,
    /// A later release.
    Later,
}

/// The release line of `v`: its first two numbers, so `4.10` for `4.10.3-rc.1`. None when `v` does
/// not parse as a version.
fn line_of(v: &str) -> Option<String> {
    let numbers = version::numbers(v)?;
    let [major, minor] = [0, 1].map(|i| numbers.get(i).copied().unwrap_or(0));
    Some(format!("{major}.{minor}"))
}

/// Place `run`, the release of a run, against `want`, the release that a report names.
///
/// A version that parses names a release by its line, so `4.10.3` and `4.10` are one release, and
/// so are `4` and `4.0`. [`version::compare`] orders two lines. A text that does not parse is the
/// same release only as the same text. A run with no release, and a text that no order places,
/// are older: they prove nothing for the release that the report names.
fn standing(run: &str, want: &str) -> Standing {
    match (line_of(run), line_of(want)) {
        (Some(r), Some(w)) => match version::compare(&r, &w) {
            Ordering::Less => Standing::Older,
            Ordering::Equal => Standing::Same,
            Ordering::Greater => Standing::Later,
        },
        (None, None) if run == want => Standing::Same,
        _ => Standing::Older,
    }
}

/// The counts of one rule.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    /// Runs that made the fault, and the check found it.
    pub caught: u32,
    /// Runs that made the fault, and the check passed.
    pub missed: u32,
    /// Clean runs in which the check passed.
    pub passed: u32,
    /// Clean runs in which the check failed.
    pub false_fail: u32,
    /// The run ids.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<String>,
    /// The ids of the runs that the counts leave out because their release is older than the
    /// release asked, or because they have no release. Empty when no release is asked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub older: Vec<String>,
}

/// One rate with its counts and its bound.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Rate {
    /// The correct runs.
    pub correct: u32,
    /// All the runs.
    pub total: u32,
    /// `correct / total`, or none with no run.
    pub rate: Option<f64>,
    /// The 95% lower bound of the rate, or none with no run.
    pub lower_bound: Option<f64>,
}

impl Rate {
    /// The rate of `correct` in `total`.
    pub fn of(correct: u32, total: u32) -> Rate {
        Rate {
            correct,
            total,
            rate: (total > 0).then(|| f64::from(correct) / f64::from(total)),
            lower_bound: lower_bound(correct, total, 0.95),
        }
    }

    /// `14/14 (81%)`, or `no runs`.
    pub fn text(&self) -> String {
        match self.lower_bound {
            Some(b) => format!("{}/{} ({})", self.correct, self.total, percent(b)),
            None => NO_RUNS.to_string(),
        }
    }
}

/// The confidence of one rule on one engine and release.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Confidence {
    /// The engine the counts are for.
    pub engine: String,
    /// The release the counts are for, or empty for every release.
    pub release: String,
    /// The catch rate: the fault made on purpose, and found.
    pub catch: Rate,
    /// The clean-pass rate: a clean build, and the check passed.
    pub clean: Rate,
    /// The bar the bounds must reach for a preflight check.
    pub bar: f64,
    /// True when both bounds reach the bar.
    pub meets_bar: bool,
    /// The run ids behind the counts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<String>,
    /// The runs of older releases, and the runs with no release, that the counts leave out. It is
    /// `0` when no release is asked.
    #[serde(default)]
    pub older_runs: u32,
}

impl Confidence {
    /// `catch 14/14 (81%), clean 9/9 (72%)`: the two rates, as a report shows them. With no run
    /// of this release, it is `no runs`. With runs of older releases, it ends with `older runs 3`.
    pub fn text(&self) -> String {
        let rates = if self.has_runs() {
            format!("catch {}, clean {}", self.catch.text(), self.clean.text())
        } else {
            NO_RUNS.to_string()
        };
        match self.older_runs {
            0 => rates,
            n => format!("{rates}, older runs {n}"),
        }
    }

    /// From the counts.
    pub fn of(engine: &str, release: &str, counts: &Counts, bar: f64) -> Confidence {
        let catch = Rate::of(counts.caught, counts.caught + counts.missed);
        let clean = Rate::of(counts.passed, counts.passed + counts.false_fail);
        let meets_bar = catch.lower_bound.is_some_and(|b| b >= bar)
            && clean.lower_bound.is_some_and(|b| b >= bar);
        Confidence {
            engine: engine.to_string(),
            release: release.to_string(),
            catch,
            clean,
            bar,
            meets_bar,
            runs: counts.runs.clone(),
            older_runs: counts.older.len() as u32,
        }
    }

    /// The weakest of the two bounds, or none with no run of one kind.
    pub fn weakest_bound(&self) -> Option<f64> {
        match (self.catch.lower_bound, self.clean.lower_bound) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        }
    }

    /// True when the ledger holds a run of the rule on this engine, of either kind.
    pub fn has_runs(&self) -> bool {
        self.catch.total + self.clean.total > 0
    }
}

/// The one-sided Clopper-Pearson lower bound of a rate at `confidence`.
///
/// It is the `p` at which `P(X >= correct | n, p) = 1 - confidence`. With every run correct it
/// is `(1 - confidence)^(1/n)`. None with no run.
pub fn lower_bound(correct: u32, total: u32, confidence: f64) -> Option<f64> {
    if total == 0 {
        return None;
    }
    if correct == 0 {
        return Some(0.0);
    }
    let alpha = 1.0 - confidence;
    let n = f64::from(total);
    let k = f64::from(correct);
    if correct == total {
        return Some(alpha.powf(1.0 / n));
    }
    // the lower bound is the alpha quantile of Beta(k, n - k + 1): find p with I_p(k, n-k+1) = alpha
    let (a, b) = (k, n - k + 1.0);
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        if regularized_beta(mid, a, b) < alpha {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some((lo + hi) / 2.0)
}

/// The regularized incomplete beta function `I_x(a, b)`, by the continued fraction of
/// Numerical Recipes (`betacf`), with the symmetry that keeps the fraction converging.
fn regularized_beta(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let ln_beta = ln_gamma(a) + ln_gamma(b) - ln_gamma(a + b);
    let front = (a * x.ln() + b * (1.0 - x).ln() - ln_beta).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        front * beta_cf(x, a, b) / a
    } else {
        1.0 - front * beta_cf(1.0 - x, b, a) / b
    }
}

fn beta_cf(x: f64, a: f64, b: f64) -> f64 {
    const TINY: f64 = 1e-300;
    let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < TINY {
        d = TINY;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=300 {
        let m = f64::from(m);
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-14 {
            break;
        }
    }
    h
}

/// `ln Γ(x)` by the Lanczos approximation.
fn ln_gamma(x: f64) -> f64 {
    const COEF: [f64; 6] = [
        76.180_091_729_471_46,
        -86.505_320_329_416_77,
        24.014_098_240_830_91,
        -1.231_739_572_450_155,
        0.120_865_097_386_617_9e-2,
        -0.539_523_938_495_3e-5,
    ];
    let y = x;
    let tmp = x + 5.5;
    let tmp = tmp - (x + 0.5) * tmp.ln();
    let mut ser = 1.000_000_000_190_015;
    let mut y = y;
    for c in COEF {
        y += 1.0;
        ser += c / y;
    }
    -tmp + (2.506_628_274_631_000_5 * ser / x).ln()
}

/// What the ledger says about the level of each rule: which probes meet the bar, and which
/// preflight checks do not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromotionReport {
    /// The schema id.
    pub schema: String,
    /// The release whose runs count. With none, every run counts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    /// One entry for each rule with a check. With a release, a rule that does not apply to it has
    /// no entry.
    pub rules: Vec<PromotionEntry>,
    /// The rules whose level disagrees with the ledger.
    pub disagreements: u32,
}

/// The schema id of the promotion report.
pub const PROMOTION_SCHEMA: &str = "strata-preflight-promotion/1";

/// One rule in the promotion report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromotionEntry {
    /// The rule id.
    pub id: String,
    /// Its section.
    pub section: String,
    /// The tier of its check.
    pub tier: Tier,
    /// The level the rule file says.
    pub level: Level,
    /// The confidence on each engine and release the ledger has runs for.
    pub confidence: Vec<Confidence>,
    /// What the ledger says the level must be. A static rule needs one run that made the fault
    /// and one clean run; a live or watch rule needs the bar on each engine it names.
    pub earned: Level,
    /// `promote`, `demote`, or empty when the level and the ledger agree.
    pub advice: String,
}

impl PromotionReport {
    /// Exit `1` when a level disagrees with the ledger.
    pub fn exit_code(&self) -> u8 {
        if self.disagreements == 0 { 0 } else { 1 }
    }
}

/// Compare the level of each rule with what the ledger proves.
///
/// With `release`, only the runs of that release count, so a rule that no run proved on that
/// release is a probe. A rule that does not apply to the release has no entry. With none, every
/// run counts.
pub fn promotion(catalog: &Catalog, ledger: &Ledger, release: Option<&str>) -> PromotionReport {
    let mut rules = Vec::new();
    let mut disagreements = 0;
    let checks = catalog
        .rules()
        .filter(|r| r.check.is_some() && r.applies_to_release(release));
    for rule in checks {
        let bar = rule.promotion.as_ref().map_or(0.8, |p| p.bar);
        let confidence = rule_confidence(ledger, &rule.id, release, bar);
        let engines = named_engines(rule, &confidence);
        let earned = earned_level(rule, ledger, &engines, release, bar);
        let advice = match (rule.level, earned) {
            (Level::Probe, Level::Preflight) => "promote",
            (Level::Preflight, Level::Probe) => "demote",
            _ => "",
        };
        if !advice.is_empty() {
            disagreements += 1;
        }
        rules.push(PromotionEntry {
            id: rule.id.clone(),
            section: rule.section.clone(),
            tier: rule.effective_tier(),
            level: rule.level,
            confidence,
            earned,
            advice: advice.to_string(),
        });
    }
    PromotionReport {
        schema: PROMOTION_SCHEMA.to_string(),
        release: release.map(str::to_string),
        rules,
        disagreements,
    }
}

/// The confidence of one rule, in the order of the engine and the release. With `release`, one
/// entry for each engine that has a run of the rule, on that release. With none, one entry for
/// each engine and release that has a run.
fn rule_confidence(
    ledger: &Ledger,
    rule: &str,
    release: Option<&str>,
    bar: f64,
) -> Vec<Confidence> {
    let counts = ledger.counts(rule);
    let Some(want) = release else {
        return counts
            .iter()
            .map(|((e, r), c)| Confidence::of(e, r, c, bar))
            .collect();
    };
    let engines: BTreeSet<&String> = counts.keys().map(|(e, _)| e).collect();
    engines
        .into_iter()
        .map(|e| Confidence::of(e, want, &ledger.counts_for(rule, e, Some(want)), bar))
        .collect()
}

/// The engines the rule names: `all` is each engine that the ledger has a run of, in
/// `confidence`.
fn named_engines(rule: &Rule, confidence: &[Confidence]) -> Vec<String> {
    if rule.engines.iter().any(|e| e == CLOUD_ALL) {
        confidence
            .iter()
            .map(|c| c.engine.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    } else {
        // the clouds alone: `lab` marks a check of the lab, and no run records it
        rule.engines
            .iter()
            .filter(|e| Engine::parse(e).is_some())
            .cloned()
            .collect()
    }
}

/// The level that the ledger proves for `rule` on `engines`, from the runs of `release` when it is
/// given: a preflight check, or a probe.
fn earned_level(
    rule: &Rule,
    ledger: &Ledger,
    engines: &[String],
    release: Option<&str>,
    bar: f64,
) -> Level {
    // the first rule of a promotion: a live run shows the fault. A rule from the source or a
    // reading stays a probe, whatever the plan runs count, until a build shows the fault
    if rule.status != Status::Seen {
        return Level::Probe;
    }
    let static_rule = rule.effective_tier() == Tier::Static;
    let proven = !engines.is_empty()
        && engines.iter().all(|e| {
            let c = ledger.counts_for(&rule.id, e, release);
            if static_rule {
                // a static rule is deterministic: one run that made the fault and one clean run
                // prove it
                c.caught >= 1 && c.missed == 0 && c.passed >= 1 && c.false_fail == 0
            } else {
                Confidence::of(e, release.unwrap_or(""), &c, bar).meets_bar
            }
        });
    if proven {
        Level::Preflight
    } else {
        Level::Probe
    }
}

#[cfg(test)]
mod tests;
