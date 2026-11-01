//! Confidence is a count of runs, never an estimate. For each rule, per engine and Palette
//! release, the ledger holds the runs that made the fault on purpose and the runs of a clean
//! build, and what the check answered in each. From the counts come two rates and their 95%
//! lower bounds (Clopper-Pearson, one side):
//!
//! - **catch rate**: of the runs that made the fault, the runs in which the check found it;
//! - **clean-pass rate**: of the runs of a clean build, the runs in which the check passed.
//!
//! A probe becomes a preflight check when both bounds reach the bar, `0.8` today. With no wrong
//! result, that is 14 correct runs of each kind, because `0.05^(1/14) = 0.807`.

use std::collections::BTreeMap;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strata::cloud::CLOUD_ALL;

use crate::error::{Error, Result};
use crate::report::percent;
use crate::rule::{Catalog, Level, Tier};

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

    /// The counts of one rule on one engine, over every release, and on one release when
    /// `release` is given.
    pub fn counts_for(&self, rule: &str, engine: &str, release: Option<&str>) -> Counts {
        let mut total = Counts::default();
        for ((e, r), c) in self.counts(rule) {
            if e != engine {
                continue;
            }
            if let Some(want) = release
                && !r.is_empty()
                && !same_minor(&r, want)
            {
                continue;
            }
            total.caught += c.caught;
            total.missed += c.missed;
            total.passed += c.passed;
            total.false_fail += c.false_fail;
            total.runs.extend(c.runs);
        }
        total.runs.sort();
        total.runs.dedup();
        total
    }
}

/// `4.10.3` and `4.10` are one release for the counts: a patch does not reset them.
fn same_minor(a: &str, b: &str) -> bool {
    let minor = |s: &str| {
        s.trim_start_matches('v')
            .split('.')
            .take(2)
            .map(|p| {
                p.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(".")
    };
    minor(a) == minor(b)
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
}

impl Confidence {
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

/// The one-sided Clopper-Pearson lower bound of a rate at `confidence`: the `p` at which
/// `P(X >= correct | n, p) = 1 - confidence`. With every run correct it is
/// `(1 - confidence)^(1/n)`. None with no run.
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
    /// One entry for each rule with a check.
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
pub fn promotion(catalog: &Catalog, ledger: &Ledger) -> PromotionReport {
    let mut rules = Vec::new();
    let mut disagreements = 0;
    for rule in catalog.rules().filter(|r| r.check.is_some()) {
        let bar = rule.promotion.as_ref().map_or(0.8, |p| p.bar);
        let mut confidence: Vec<Confidence> = ledger
            .counts(&rule.id)
            .iter()
            .map(|((e, r), c)| Confidence::of(e, r, c, bar))
            .collect();
        confidence.sort_by(|a, b| (&a.engine, &a.release).cmp(&(&b.engine, &b.release)));
        // the engines the rule names: `all` is each engine the ledger has a run of
        let engines: Vec<String> = if rule.engines.iter().any(|e| e == CLOUD_ALL) {
            confidence
                .iter()
                .map(|c| c.engine.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        } else {
            // the clouds alone: `lab` marks a check of the lab, and no run records it
            rule.engines
                .iter()
                .filter(|e| crate::world::Engine::parse(e).is_some())
                .cloned()
                .collect()
        };
        // the first rule of a promotion: a live run shows the fault. A rule from the source or a
        // reading stays a probe, whatever the plan runs count, until a build shows the fault
        let seen = rule.status == crate::rule::Status::Seen;
        let earned = match rule.effective_tier() {
            _ if !seen => Level::Probe,
            // a static rule is deterministic: one run that made the fault and one clean run prove it
            Tier::Static => {
                let proven = !engines.is_empty()
                    && engines.iter().all(|e| {
                        let c = ledger.counts_for(&rule.id, e, None);
                        c.caught >= 1 && c.missed == 0 && c.passed >= 1 && c.false_fail == 0
                    });
                if proven {
                    Level::Preflight
                } else {
                    Level::Probe
                }
            }
            _ => {
                let proven = !engines.is_empty()
                    && engines.iter().all(|e| {
                        let c = ledger.counts_for(&rule.id, e, None);
                        Confidence::of(e, "", &c, bar).meets_bar
                    });
                if proven {
                    Level::Preflight
                } else {
                    Level::Probe
                }
            }
        };
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
        rules,
        disagreements,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.005
    }

    #[test]
    fn the_bound_with_no_wrong_result_is_the_table_of_the_issue() {
        for (n, want) in [
            (3, 0.37),
            (5, 0.55),
            (10, 0.74),
            (14, 0.81),
            (20, 0.86),
            (30, 0.90),
        ] {
            let b = lower_bound(n, n, 0.95).unwrap();
            assert!(close(b, want), "{n} of {n}: {b}");
        }
        assert_eq!(lower_bound(0, 0, 0.95), None);
        assert_eq!(lower_bound(0, 5, 0.95), Some(0.0));
    }

    #[test]
    fn the_bound_with_a_miss_uses_the_beta_quantile() {
        // 13 of 14: the one-sided 95% Clopper-Pearson lower bound is 0.703, under the bar of 0.8
        let b = lower_bound(13, 14, 0.95).unwrap();
        assert!(close(b, 0.703), "{b}");
        // 9 of 10 is 0.606
        let b = lower_bound(9, 10, 0.95).unwrap();
        assert!(close(b, 0.606), "{b}");
        // 19 of 20 is about 0.78, so one miss in 20 runs is under the bar
        let b = lower_bound(19, 20, 0.95).unwrap();
        assert!(close(b, 0.784), "{b}");
        // 1 of 2
        let b = lower_bound(1, 2, 0.95).unwrap();
        assert!(close(b, 0.0253), "{b}");
    }

    #[test]
    fn the_regularized_beta_matches_known_values() {
        assert!(close(regularized_beta(0.5, 1.0, 1.0), 0.5));
        assert!(close(regularized_beta(0.5, 2.0, 2.0), 0.5));
        assert!(close(regularized_beta(0.2, 2.0, 3.0), 0.1808));
    }

    fn ledger(text: &str) -> Ledger {
        Ledger::parse(text, Path::new("runs.yaml")).unwrap()
    }

    const RUNS: &str = r#"
schema: strata-preflight-ledger/1
runs:
  - run: r1
    engine: gke
    release: "4.10"
    results:
      - {rule: gke-pool-multiple-3, result: caught}
      - {rule: gke-api-off, result: caught}
  - run: r2
    engine: gke
    release: "4.10.3"
    results:
      - {rule: gke-pool-multiple-3, result: passed}
      - {rule: gke-api-off, result: missed}
      - {rule: gke-no-gpu, result: caught}
      - {rule: gke-no-gpu, result: passed}
"#;

    #[test]
    fn the_counts_join_the_patches_of_one_release() {
        let l = ledger(RUNS);
        let c = l.counts_for("gke-pool-multiple-3", "gke", Some("4.10"));
        assert_eq!((c.caught, c.passed, c.missed), (1, 1, 0));
        assert_eq!(c.runs, vec!["r1", "r2"]);
        let c = l.counts_for("gke-pool-multiple-3", "gke", Some("4.11"));
        assert_eq!(c.caught + c.passed, 0, "another release has no run");
        let c = l.counts_for("gke-pool-multiple-3", "eks", None);
        assert_eq!(c.caught + c.passed, 0, "another engine has no run");
    }

    #[test]
    fn confidence_reads_the_bar() {
        let c = Counts {
            caught: 14,
            missed: 0,
            passed: 14,
            false_fail: 0,
            runs: vec![],
        };
        let conf = Confidence::of("gke", "4.10", &c, 0.8);
        assert!(conf.meets_bar);
        assert_eq!(conf.catch.text(), "14/14 (81%)");
        let c = Counts {
            caught: 14,
            missed: 0,
            passed: 13,
            false_fail: 1,
            runs: vec![],
        };
        assert!(!Confidence::of("gke", "4.10", &c, 0.8).meets_bar);
        let none = Confidence::of("gke", "4.10", &Counts::default(), 0.8);
        assert_eq!(none.catch.text(), NO_RUNS);
        assert_eq!(none.weakest_bound(), None);
    }

    #[test]
    fn a_ledger_of_another_schema_is_refused() {
        let err = Ledger::parse("schema: other/1\nruns: []\n", Path::new("l.yaml")).unwrap_err();
        assert!(err.to_string().contains("schema is 'other/1'"), "{err}");
    }

    #[test]
    fn a_promotion_counts_the_clouds_a_rule_names_and_not_the_lab() {
        let section = r#"
section: tools
title: T
rules:
  - id: cloud-cli-auth
    title: t
    engines: [aws, lab]
    tier: cloud
    status: seen
    what: w
    check: {kind: exec, argv: [aws], expr: "true"}
"#;
        let mut catalog = Catalog::default();
        catalog
            .push(
                Catalog::parse_section(section, Path::new("t.yaml")).unwrap(),
                Path::new("t.yaml"),
            )
            .unwrap();
        let mut runs = String::from("schema: strata-preflight-ledger/1\nruns:\n");
        for i in 0..14 {
            runs.push_str(&format!(
                "  - run: r{i}\n    engine: aws\n    release: \"\"\n    results:\n      - {{rule: cloud-cli-auth, result: caught, note: f}}\n      - {{rule: cloud-cli-auth, result: passed, note: c}}\n"
            ));
        }
        let report = promotion(&catalog, &ledger(&runs));
        let rule = report
            .rules
            .iter()
            .find(|r| r.id == "cloud-cli-auth")
            .unwrap();
        assert_eq!(
            rule.earned,
            Level::Preflight,
            "14 correct runs of each kind on aws, and no run of the lab is needed"
        );
        assert_eq!(rule.advice, "promote");
    }

    #[test]
    fn promotion_compares_the_level_with_the_ledger() {
        let section = r#"
section: compute
title: Compute
rules:
  - id: gke-pool-multiple-3
    title: t
    engines: [gke]
    tier: static
    status: seen
    what: w
    check: {kind: static, expr: "true"}
  - id: gke-api-off
    title: t
    engines: [gke]
    tier: cloud
    status: seen
    what: w
    check: {kind: exec, argv: [gcloud], expr: "true"}
  - id: gke-no-gpu
    title: t
    engines: [gke]
    tier: static
    status: source
    what: w
    check: {kind: static, expr: "true"}
"#;
        let mut catalog = Catalog::default();
        catalog
            .push(
                Catalog::parse_section(section, Path::new("c.yaml")).unwrap(),
                Path::new("c.yaml"),
            )
            .unwrap();
        let report = promotion(&catalog, &ledger(RUNS));
        let pool = report
            .rules
            .iter()
            .find(|r| r.id == "gke-pool-multiple-3")
            .unwrap();
        assert_eq!(
            pool.earned,
            Level::Preflight,
            "a static rule with one fault and one clean run is proven"
        );
        assert_eq!(pool.advice, "promote");
        let api = report.rules.iter().find(|r| r.id == "gke-api-off").unwrap();
        assert_eq!(api.earned, Level::Probe, "a miss, and no clean run");
        assert_eq!(api.advice, "");
        let gpu = report.rules.iter().find(|r| r.id == "gke-no-gpu").unwrap();
        assert_eq!(
            gpu.earned,
            Level::Probe,
            "the runs count, and no live run shows the fault: status source"
        );
        assert_eq!(report.disagreements, 1);
        assert_eq!(report.exit_code(), 1);
    }
}
