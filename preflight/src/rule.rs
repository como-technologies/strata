//! The rule format. A rule is data: one thing that can stop or break a cluster build, where a
//! check can find it, how much to trust the check, and the check itself as a jq program. The
//! catalog that the binary carries and a file of custom probes (`--probes FILE`) are the same
//! format, so a company reads and writes rules the way strata does.
//!
//! One file holds one section:
//!
//! ```yaml
//! section: networking
//! title: Networking
//! about: The subnets, the zones, the CIDRs, and the routes that a build needs.
//! rules:
//!   - id: eks-one-zone
//!     title: EKS needs subnets in two zones
//!     engines: [eks]
//!     tier: static
//!     level: probe
//!     severity: fail
//!     status: source
//!     what: |
//!       EKS needs subnets in two zones or more. Palette checks the count of the control plane
//!       subnets, not the count of distinct zones.
//!     check:
//!       kind: static
//!       expr: |
//!         [ .entity.spec.machinepoolconfig[]
//!           | select((.cloudConfig.azs // []) | unique | length < 2)
//!           | "pool \(.poolConfig.name) names \((.cloudConfig.azs // []) | unique | length) zone(s); EKS needs 2" ]
//!     fix:
//!       owner: palette
//!       text: Check the count of distinct zones in the EKS validate route.
//!     evidence:
//!       - kind: source
//!         ref: hubble services/service/spectrocluster/internal/service/spectrocluster/internal/eks/spectrocluster_eks.go
//!       - kind: report
//!         ref: palette-precreate-validation-gaps-findings-2026-09-28 F6
//!     tests:
//!       - fault: fixtures/eks/one-zone.json
//!         clean: fixtures/eks/clean.json
//! ```
//!
//! A check outputs one of three shapes, and the engine reads each the same way: `true` or
//! `false`; a list of messages, where an empty list passes and each message is one failure; or an
//! object `{pass, message}`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The engines a rule can name. `all` stands for every engine.
pub const ENGINES: &[&str] = &[
    "all",
    "aws",
    "eks",
    "gcp",
    "gke",
    "azure",
    "aks",
    "maas",
    "edge",
    "vsphere",
    "openstack",
    "lab",
];

/// Where a check can find the fault.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    /// From the entity of the create and the records alone. No key, no network.
    Static,
    /// A call to Palette.
    Palette,
    /// A call to the cloud, with the tool of the cloud. strata-preflight runs it when the tool is
    /// there and authenticated, and prints `NOT CHECKED` with the command when it is not.
    Cloud,
    /// During the build: the conditions and the events of the cluster.
    Watch,
    /// A race: no check before the build finds it. The watch finds its effect.
    Race,
    /// A rule of the tool that makes the entity, not of Palette. Out of scope here; the row stays
    /// so the catalog is complete.
    Tool,
}

impl Tier {
    /// The letter of the catalog of #91.
    pub fn letter(self) -> &'static str {
        match self {
            Tier::Static => "S",
            Tier::Palette => "P",
            Tier::Cloud => "C",
            Tier::Watch => "W",
            Tier::Race => "X",
            Tier::Tool => "T",
        }
    }

    /// The name, as the rule files spell it.
    pub fn name(self) -> &'static str {
        match self {
            Tier::Static => "static",
            Tier::Palette => "palette",
            Tier::Cloud => "cloud",
            Tier::Watch => "watch",
            Tier::Race => "race",
            Tier::Tool => "tool",
        }
    }

    /// Every tier, in the order of the report.
    pub const ALL: [Tier; 6] = [
        Tier::Static,
        Tier::Palette,
        Tier::Cloud,
        Tier::Watch,
        Tier::Race,
        Tier::Tool,
    ];

    /// Parse the name a person types.
    pub fn parse(text: &str) -> Option<Tier> {
        Tier::ALL
            .into_iter()
            .find(|t| t.name() == text || t.letter().eq_ignore_ascii_case(text))
    }
}

/// How much a person can trust the result of the check.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Default,
)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    /// strata-preflight knows the fault, but the runs do not yet prove the check. A failed probe
    /// does not change the exit code; `--strict` counts it.
    #[default]
    Probe,
    /// The runs prove the check, per engine and Palette release. A failed preflight check makes
    /// the exit code `1`.
    Preflight,
}

impl Level {
    /// The word of the report.
    pub fn name(self) -> &'static str {
        match self {
            Level::Probe => "probe",
            Level::Preflight => "preflight",
        }
    }
}

/// What a failed check means for the build.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Default,
)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// Palette cannot finish the build, or the build breaks the cluster.
    #[default]
    Fail,
    /// The build can finish, and something stays wrong or costs more than expected.
    Warn,
    /// A fact to know. A note never fails a run.
    Note,
}

impl Severity {
    /// The word of the report.
    pub fn name(self) -> &'static str {
        match self {
            Severity::Fail => "fail",
            Severity::Warn => "warn",
            Severity::Note => "note",
        }
    }
}

/// Where the knowledge of the fault comes from.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// A live run shows it.
    Seen,
    /// The source or the documentation of Palette shows it, and no run.
    Source,
    /// A reading, not confirmed.
    Inferred,
    /// A rule strata proposes; no run and no source names it.
    Proposed,
}

impl Status {
    /// The word of the report.
    pub fn name(self) -> &'static str {
        match self {
            Status::Seen => "seen",
            Status::Source => "source",
            Status::Inferred => "inferred",
            Status::Proposed => "proposed",
        }
    }
}

/// Who fixes the fault for good, and how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Fix {
    /// `palette`, `provider`, `lab`, `strata`, `operator`, or `none`.
    pub owner: String,
    /// The change.
    #[serde(default)]
    pub text: String,
}

/// One piece of evidence: a live run, a line of the source, a public page, or a findings report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Evidence {
    /// `run`, `source`, `doc`, or `report`.
    pub kind: String,
    /// The run id or issue, the file and the function, the page, or the report and its finding.
    #[serde(rename = "ref")]
    pub reference: String,
    /// What it shows, when the reference alone does not say.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// The runs that promoted a rule to a preflight check, and the bar they met.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Promotion {
    /// The 95% lower bound that the catch rate and the clean-pass rate must reach. `0.8` today,
    /// as a field so that it can go up.
    #[serde(default = "default_bar")]
    pub bar: f64,
    /// The run ids of the promotion, in the ledger.
    #[serde(default)]
    pub runs: Vec<String>,
}

fn default_bar() -> f64 {
    0.8
}

/// A pair of fixtures: one that holds the fault, and one that is clean. The test of the catalog
/// runs the check on each, and the rule fails its test when the fault passes or the clean one
/// fails. Paths are relative to the rule file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuleTest {
    /// A world document with the fault. The check must fail on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault: Option<PathBuf>,
    /// A world document with no fault. The check must pass on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clean: Option<PathBuf>,
}

/// The HTTP method of a Palette check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    /// Read.
    #[default]
    Get,
    /// Ask a question that changes nothing, such as a validate route.
    Post,
}

/// The check of a rule: how strata-preflight finds the fault. Each kind runs a jq program over
/// the world document (see [`crate::world`]), and the kinds differ in what they add to it first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Check {
    /// A jq program over the world alone: the entity, the change, the records, the release.
    Static {
        /// The program. Its output is `true`/`false`, a list of failure messages, or
        /// `{pass, message}`.
        expr: String,
    },
    /// One request to Palette, then a jq program over the world with `.response` added:
    /// `{ok, status, body}`. A 4xx is an answer, with `ok: false` and its `V1Error` body. A 204 is
    /// `ok: true` with a `null` body.
    Palette {
        /// `GET` or `POST`.
        #[serde(default)]
        method: Method,
        /// The path, with `{{ jq }}` templates over the world: `/v1/spectroclusters/{{ .cloud }}/validate`.
        path: String,
        /// For a POST: a jq program over the world that gives the body. Default `.entity`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        /// The program over the world with `.response`.
        expr: String,
    },
    /// One command on this machine, such as the tool of the cloud, then a jq program over the
    /// world with `.exec` added: `{exit, stdout, stderr}`. `stdout` is JSON when it parses, and
    /// the text otherwise. The command runs when each tool of `needs` is on the PATH; when one is
    /// not, the result is `NOT CHECKED` with the command to run by hand.
    Exec {
        /// The command and its arguments, each with `{{ jq }}` templates over the world.
        argv: Vec<String>,
        /// The tools the command needs, by name, checked on the PATH first.
        #[serde(default)]
        needs: Vec<String>,
        /// Seconds before the command is stopped. Default 60.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
        /// The program over the world with `.exec`.
        expr: String,
    },
    /// During a build: every `every` seconds, read the cluster, its summary, and its events, and
    /// run the program over the world with `.cluster`, `.summary`, `.events`, and `.elapsed`
    /// (seconds) added. A failure stops the watch with its evidence.
    Watch {
        /// The program. It fails when the cluster is in a known stuck state.
        expr: String,
    },
}

impl Check {
    /// The kind, as the rule files spell it.
    pub fn kind(&self) -> &'static str {
        match self {
            Check::Static { .. } => "static",
            Check::Palette { .. } => "palette",
            Check::Exec { .. } => "exec",
            Check::Watch { .. } => "watch",
        }
    }

    /// The tier a check of this kind belongs to.
    pub fn tier(&self) -> Tier {
        match self {
            Check::Static { .. } => Tier::Static,
            Check::Palette { .. } => Tier::Palette,
            Check::Exec { .. } => Tier::Cloud,
            Check::Watch { .. } => Tier::Watch,
        }
    }

    /// The main program of the check.
    pub fn expr(&self) -> &str {
        match self {
            Check::Static { expr }
            | Check::Palette { expr, .. }
            | Check::Exec { expr, .. }
            | Check::Watch { expr } => expr,
        }
    }
}

/// One rule of the catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Rule {
    /// A kebab-case id, unique in the catalog and in a probes file: the id of the row of #91.
    pub id: String,
    /// One line that names the fault.
    pub title: String,
    /// The section the rule is in, from the file.
    #[serde(default)]
    pub section: String,
    /// The engines the rule applies to, or `all`.
    pub engines: Vec<String>,
    /// Where a check can find the fault. A rule with a `check` has the tier of its check.
    pub tier: Tier,
    /// Probe or preflight check.
    #[serde(default)]
    pub level: Level,
    /// What a failure means.
    #[serde(default)]
    pub severity: Severity,
    /// Where the knowledge comes from.
    pub status: Status,
    /// The Palette releases the rule applies to, as `>=4.10`, `<4.11`, or `>=4.9 <4.11`. No
    /// value means every release. A rule that a release fixes stops at that release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub releases: Option<String>,
    /// What goes wrong, and what Palette shows.
    pub what: String,
    /// A jq program over the world that says when the rule applies: `.before != null` for a rule
    /// on a change, `.clusterUid != null` for a rule on an existing cluster, `.account != null`
    /// for a rule on the cloud account. No value means always. A rule that does not apply is
    /// `SKIP`, with the program as the reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// The check, when strata-preflight has one. A rule with none is a row of the catalog: the
    /// report prints it as `NOT CHECKED` with `reproduce`, or as out of scope for a tool rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<Check>,
    /// The fix for good, and its owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
    /// Commands a person runs by hand to see the same thing: the `curl`, `jq`, `aws`, or
    /// `gcloud` line. The report prints them for each failed check and each `NOT CHECKED` row.
    /// A template `{{ jq }}` over the world is filled in the report.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reproduce: Vec<String>,
    /// The runs, the source, the pages, and the reports that show the fault.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<Evidence>,
    /// The promotion of a preflight check: its bar and its runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promotion: Option<Promotion>,
    /// The fixtures the test of the catalog runs the check on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<RuleTest>,
}

impl Rule {
    /// True when the rule applies to `engine`.
    pub fn applies_to(&self, engine: &str) -> bool {
        self.engines
            .iter()
            .any(|e| e == "all" || e.eq_ignore_ascii_case(engine))
    }

    /// True when the rule applies to the Palette release `release`, or when no release is
    /// known.
    pub fn applies_to_release(&self, release: Option<&str>) -> bool {
        match (&self.releases, release) {
            (None, _) | (Some(_), None) => true,
            (Some(range), Some(r)) => release_in_range(r, range),
        }
    }

    /// The tier that runs: the tier of the check when there is one.
    pub fn effective_tier(&self) -> Tier {
        self.check.as_ref().map_or(self.tier, Check::tier)
    }
}

/// One section of the catalog: a file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Section {
    /// A kebab-case id: `networking`.
    pub section: String,
    /// The title of the report: `Networking`.
    pub title: String,
    /// One or two sentences on what the section covers.
    #[serde(default)]
    pub about: String,
    /// The rules, in the order of the report.
    pub rules: Vec<Rule>,
}

/// Every section and every rule that a run reads: the catalog of the binary, and the probes of
/// each `--probes FILE` after it.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    /// The sections, in order.
    pub sections: Vec<Section>,
}

impl Catalog {
    /// Parse one section file. `path` names the file in messages, and roots the fixture paths.
    pub fn parse_section(text: &str, path: &Path) -> Result<Section> {
        let mut section: Section = serde_yaml::from_str(text).map_err(|e| Error::Catalog {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        let dir = path.parent().unwrap_or(Path::new("."));
        for rule in &mut section.rules {
            rule.section = section.section.clone();
            for t in &mut rule.tests {
                if let Some(p) = &t.fault {
                    t.fault = Some(dir.join(p));
                }
                if let Some(p) = &t.clean {
                    t.clean = Some(dir.join(p));
                }
            }
        }
        check_section(&section, path)?;
        Ok(section)
    }

    /// Read one section file from disk.
    pub fn load_section(path: &Path) -> Result<Section> {
        let text = std::fs::read_to_string(path).map_err(|e| crate::error::io(path, e))?;
        Self::parse_section(&text, path)
    }

    /// Add a section. A rule id that a section already has is an error, so a probes file cannot
    /// shadow a rule of the catalog by mistake.
    pub fn push(&mut self, section: Section, path: &Path) -> Result<()> {
        let known: BTreeSet<&str> = self.rules().map(|r| r.id.as_str()).collect();
        for rule in &section.rules {
            if known.contains(rule.id.as_str()) {
                return Err(Error::Catalog {
                    path: path.to_path_buf(),
                    message: format!("rule {} is in the catalog already", rule.id),
                });
            }
        }
        // one section id can come in two files, such as a custom probe of networking: the rules
        // join the section that is there
        if let Some(have) = self
            .sections
            .iter_mut()
            .find(|s| s.section == section.section)
        {
            have.rules.extend(section.rules);
        } else {
            self.sections.push(section);
        }
        Ok(())
    }

    /// Every rule, in the order of the sections.
    pub fn rules(&self) -> impl Iterator<Item = &Rule> {
        self.sections.iter().flat_map(|s| s.rules.iter())
    }

    /// One rule by id.
    pub fn rule(&self, id: &str) -> Option<&Rule> {
        self.rules().find(|r| r.id == id)
    }

    /// The section of a rule.
    pub fn section_of(&self, rule: &Rule) -> Option<&Section> {
        self.sections.iter().find(|s| s.section == rule.section)
    }
}

/// The rules of the format that the parser cannot express.
fn check_section(section: &Section, path: &Path) -> Result<()> {
    let fail = |message: String| Error::Catalog {
        path: path.to_path_buf(),
        message,
    };
    if !is_kebab(&section.section) {
        return Err(fail(format!(
            "section '{}' is not kebab-case",
            section.section
        )));
    }
    let mut ids = BTreeSet::new();
    for rule in &section.rules {
        let at = |m: String| fail(format!("rule {}: {m}", rule.id));
        if !is_kebab(&rule.id) {
            return Err(fail(format!("rule id '{}' is not kebab-case", rule.id)));
        }
        if !ids.insert(rule.id.as_str()) {
            return Err(fail(format!("rule {} is in the file two times", rule.id)));
        }
        if rule.title.trim().is_empty() {
            return Err(at("no title".into()));
        }
        if rule.what.trim().is_empty() {
            return Err(at("no `what`".into()));
        }
        if rule.engines.is_empty() {
            return Err(at("no engines; name one, or `all`".into()));
        }
        for e in &rule.engines {
            if !ENGINES.contains(&e.as_str()) {
                return Err(at(format!(
                    "engine '{e}' is not one of {}",
                    ENGINES.join(", ")
                )));
            }
        }
        if let Some(check) = &rule.check {
            if check.tier() != rule.tier {
                return Err(at(format!(
                    "the check is of kind {}, which is tier {}, and the rule says tier {}",
                    check.kind(),
                    check.tier().name(),
                    rule.tier.name()
                )));
            }
            if check.expr().trim().is_empty() {
                return Err(at("the check has no expr".into()));
            }
            if let Check::Exec { argv, .. } = check
                && argv.is_empty()
            {
                return Err(at("an exec check needs an argv".into()));
            }
        }
        if rule.level == Level::Preflight {
            if rule.check.is_none() {
                return Err(at("a preflight check needs a check".into()));
            }
            match &rule.promotion {
                Some(p) if !p.runs.is_empty() => {}
                _ => {
                    return Err(at(
                        "a preflight check names the runs of its promotion".into()
                    ));
                }
            }
        }
        if rule.when.as_deref().is_some_and(|w| w.trim().is_empty()) {
            return Err(at("`when` is empty; drop it, or give a program".into()));
        }
        if let Some(range) = &rule.releases
            && parse_range(range).is_none()
        {
            return Err(at(format!(
                "releases '{range}' is not a range such as '>=4.10' or '>=4.9 <4.11'"
            )));
        }
        for t in &rule.tests {
            if t.fault.is_none() && t.clean.is_none() {
                return Err(at(
                    "a test names a fault fixture, a clean fixture, or both".into()
                ));
            }
            if rule.check.is_none() {
                return Err(at("a test needs a check".into()));
            }
        }
    }
    Ok(())
}

fn is_kebab(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

/// A version as a list of numbers: `4.10.0` is `[4, 10, 0]`. Text after the numbers is ignored.
fn version_parts(text: &str) -> Vec<u64> {
    text.trim()
        .trim_start_matches('v')
        .split('.')
        .map_while(|p| {
            p.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .ok()
        })
        .collect()
}

/// Compare two versions by their numbers; a missing part is `0`.
fn cmp_versions(a: &[u64], b: &[u64]) -> std::cmp::Ordering {
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

/// One bound of a range: the operator and the version.
fn parse_range(text: &str) -> Option<Vec<(&str, Vec<u64>)>> {
    let mut bounds = Vec::new();
    for part in text.split_whitespace() {
        let (op, ver) = ["<=", ">=", "<", ">", "="]
            .into_iter()
            .find_map(|op| part.strip_prefix(op).map(|v| (op, v)))?;
        let parts = version_parts(ver);
        if parts.is_empty() {
            return None;
        }
        bounds.push((op, parts));
    }
    if bounds.is_empty() {
        None
    } else {
        Some(bounds)
    }
}

/// True when `release` is in `range`.
pub fn release_in_range(release: &str, range: &str) -> bool {
    use std::cmp::Ordering::*;
    let Some(bounds) = parse_range(range) else {
        return true;
    };
    let r = version_parts(release);
    if r.is_empty() {
        return true;
    }
    bounds.iter().all(|(op, v)| {
        // compare on the parts the bound names: `<4.11` holds for 4.10.3, and `=4.10` for 4.10.3
        let r = &r[..r.len().min(v.len())];
        let c = cmp_versions(r, v);
        match *op {
            "<" => c == Less,
            "<=" => c != Greater,
            ">" => c == Greater,
            ">=" => c != Less,
            _ => c == Equal,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
section: compute
title: Compute and pools
rules:
  - id: gke-pool-multiple-3
    title: A GKE worker pool size is a multiple of 3
    engines: [gke]
    tier: static
    status: seen
    what: Palette refuses the create with InvalidParam, after a full apply.
    check:
      kind: static
      expr: '[.entity.spec.machinepoolconfig[] | select(.poolConfig.size % 3 != 0) | .poolConfig.name]'
    tests:
      - fault: fixtures/gke/pool-4.json
        clean: fixtures/gke/clean.json
"#;

    #[test]
    fn a_section_parses_and_names_its_rules() {
        let s = Catalog::parse_section(GOOD, Path::new("rules/compute.yaml")).unwrap();
        assert_eq!(s.rules[0].section, "compute");
        assert_eq!(s.rules[0].level, Level::Probe);
        assert_eq!(s.rules[0].severity, Severity::Fail);
        assert_eq!(
            s.rules[0].tests[0].fault.as_deref(),
            Some(Path::new("rules/fixtures/gke/pool-4.json"))
        );
        assert!(s.rules[0].applies_to("gke"));
        assert!(!s.rules[0].applies_to("eks"));
    }

    #[test]
    fn the_format_refuses_what_it_cannot_run() {
        let bad = |text: &str, needle: &str| {
            let err = Catalog::parse_section(text, Path::new("x.yaml")).unwrap_err();
            assert!(err.to_string().contains(needle), "{err}");
        };
        bad(
            &GOOD.replace("tier: static", "tier: palette"),
            "the check is of kind static",
        );
        bad(
            &GOOD.replace("engines: [gke]", "engines: [gkee]"),
            "engine 'gkee'",
        );
        bad(
            &GOOD.replace("status: seen", "status: seen\n    level: preflight"),
            "names the runs of its promotion",
        );
        bad(
            &GOOD.replace("status: seen", "status: seen\n    releases: banana"),
            "releases 'banana'",
        );
        bad(
            &GOOD.replace("id: gke-pool-multiple-3", "id: Gke_pool"),
            "not kebab-case",
        );
    }

    #[test]
    fn a_catalog_refuses_one_id_two_times() {
        let mut c = Catalog::default();
        let s = Catalog::parse_section(GOOD, Path::new("a.yaml")).unwrap();
        c.push(s.clone(), Path::new("a.yaml")).unwrap();
        let err = c.push(s, Path::new("probes.yaml")).unwrap_err();
        assert!(
            err.to_string().contains("is in the catalog already"),
            "{err}"
        );
    }

    #[test]
    fn release_ranges() {
        assert!(release_in_range("4.10.3", "<4.11"));
        assert!(!release_in_range("4.11.0", "<4.11"));
        assert!(release_in_range("4.10", ">=4.9 <4.11"));
        assert!(!release_in_range("4.8", ">=4.9 <4.11"));
        assert!(release_in_range("4.10.3", "=4.10"));
        assert!(release_in_range("v4.10.0-rc1", ">=4.10"));
        // an unknown release applies to every rule
        let r = Catalog::parse_section(GOOD, Path::new("x.yaml")).unwrap();
        assert!(r.rules[0].applies_to_release(None));
    }
}
