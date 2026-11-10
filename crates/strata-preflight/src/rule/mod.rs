//! The rule format.
//!
//! A rule is data: one thing that can stop or break a cluster build, and where a check can find
//! it. A rule also holds how much to trust the check, and the check itself as a jq program. The
//! catalog that the binary carries and a file of custom probes (`--probes FILE`) are the same
//! format. So a company reads and writes rules the way strata does.
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
//!           | "pool \(.poolConfig.name) names \((.cloudConfig.azs // []) | unique | length | count_of("zone")); EKS needs 2" ]
//!     fix:
//!       owner: palette
//!       text: Check the count of distinct zones in the EKS validate route.
//!     evidence:
//!       - kind: run
//!         ref: a live run
//!     tests:
//!       - fault: fixtures/eks/one-zone.json
//!         clean: fixtures/eks/clean.json
//! ```
//!
//! A check outputs one of three shapes, and the engine reads each the same way. The first shape
//! is `true` or `false`. The second is a list of messages: an empty list passes, and each message
//! is one failure. The third is an object `{pass, message}`.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strata::closed_set::legend::Described;
use strata::closed_set::{ClosedSet, impl_closed_set};
use strata::model::cloud::CLOUD_ALL;
use strata::reference::version;

use crate::error::{Error, Result};
use crate::world::clouds;

mod check;
mod validate;

use validate::{check_section, check_section_programs};

pub use check::{Check, CheckKind, ExecCheck, PaletteCheck, PaletteRead};

/// The engine `lab`: it marks a check of the lab. No run of the plan records it, so a promotion
/// counts the clouds alone.
const ENGINE_LAB: &str = "lab";

/// The engines a rule can name: `all`, which stands for every engine, then each cloud of
/// [`clouds`], then `lab`.
pub fn engines() -> impl Iterator<Item = &'static str> {
    std::iter::once(CLOUD_ALL)
        .chain(clouds())
        .chain(std::iter::once(ENGINE_LAB))
}

/// True when a rule can name `engine`.
fn is_engine(engine: &str) -> bool {
    engines().any(|e| e == engine)
}

/// The engines a rule can name, as a list for a message.
fn engine_list() -> String {
    engines().collect::<Vec<_>>().join(", ")
}

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

    /// The tier that a person types: its name, or its letter in either case.
    pub fn from_input(text: &str) -> Option<Tier> {
        Tier::ALL
            .iter()
            .copied()
            .find(|t| t.as_str() == text || t.letter().eq_ignore_ascii_case(text))
    }
}

impl_closed_set!(Tier {
    Static => "static",
    Palette => "palette",
    Cloud => "cloud",
    Watch => "watch",
    Race => "race",
    Tool => "tool",
});

impl Described for Tier {
    fn describe(self) -> &'static str {
        match self {
            Tier::Static => "the entity and the records alone; no key, no network",
            Tier::Palette => "a call to Palette",
            Tier::Cloud => "a call to the cloud with its tool",
            Tier::Watch => "the conditions and the events of the cluster during the build",
            Tier::Race => "no check before the build finds it; the watch finds its effect",
            Tier::Tool => "a rule of the tool that makes the entity, out of scope here",
        }
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

impl_closed_set!(Level {
    Probe => "probe",
    Preflight => "preflight",
});

impl Described for Level {
    fn describe(self) -> &'static str {
        match self {
            Level::Probe => {
                "the runs do not yet prove the check; a failure does not change the exit code without --strict"
            }
            Level::Preflight => "the runs prove the check on this engine; a failure is exit 1",
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

impl_closed_set!(Severity {
    Fail => "fail",
    Warn => "warn",
    Note => "note",
});

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

impl_closed_set!(Status {
    Seen => "seen",
    Source => "source",
    Inferred => "inferred",
    Proposed => "proposed",
});

/// Who fixes a fault for good.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Owner {
    /// Palette: a release of Palette fixes it.
    Palette,
    /// The provider of the cloud or of the tool.
    Provider,
    /// The lab that makes the runs.
    Lab,
    /// strata, or strata-preflight.
    Strata,
    /// The operator of the cluster: a change of the entity or of the account.
    Operator,
    /// Nobody: the fault has no fix, or no fix is needed.
    #[serde(rename = "none")]
    Nobody,
}

impl_closed_set!(Owner {
    Palette => "palette",
    Provider => "provider",
    Lab => "lab",
    Strata => "strata",
    Operator => "operator",
    Nobody => "none",
});

/// Who fixes the fault for good, and how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Fix {
    /// The owner of the fix. A rule file writes it as `palette`, `provider`, `lab`, `strata`,
    /// `operator`, or `none`.
    pub owner: Owner,
    /// The change.
    #[serde(default)]
    pub text: String,
}

impl Fix {
    /// The fix as a report shows it: the change, then the owner in brackets.
    pub fn line(&self) -> String {
        format!("{} ({})", self.text, self.owner.as_str())
    }
}

/// What a piece of evidence is.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceKind {
    /// A live run: its id, or the issue that holds it.
    Run,
    /// The source or the documentation of Palette shows it, and no run.
    Source,
    /// A public page.
    Doc,
    /// A findings report and its finding.
    Report,
}

impl_closed_set!(EvidenceKind {
    Run => "run",
    Source => "source",
    Doc => "doc",
    Report => "report",
});

/// One piece of evidence: a live run, the source of Palette, a public page, or a findings report.
///
/// An entry of the kind `source` quotes no code of Palette, and names no file, function, or
/// commit of it (rule 7 of the book). Its note states what Palette does, and its reference is
/// empty, or a file of this repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Evidence {
    /// What the evidence is.
    pub kind: EvidenceKind,
    /// The run id or issue, a file of this repository, the page, or the report and its finding.
    #[serde(rename = "ref", default, skip_serializing_if = "String::is_empty")]
    pub reference: String,
    /// What it shows, when the reference alone does not say. Each `source` entry has one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// The reproduce lines of a rule: one list, or a list for each engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Reproduce {
    /// The same lines for every engine.
    Lines(Vec<String>),
    /// A list for each engine, and `all` for an engine that has none of its own.
    ByEngine(std::collections::BTreeMap<String, Vec<String>>),
}

impl Default for Reproduce {
    fn default() -> Self {
        Reproduce::Lines(Vec::new())
    }
}

impl Reproduce {
    /// True with no line at all.
    pub fn is_empty(&self) -> bool {
        match self {
            Reproduce::Lines(l) => l.is_empty(),
            Reproduce::ByEngine(m) => m.values().all(Vec::is_empty),
        }
    }

    /// Each line of the rule, for every engine.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        let lines: Box<dyn Iterator<Item = &String> + '_> = match self {
            Reproduce::Lines(l) => Box::new(l.iter()),
            Reproduce::ByEngine(m) => Box::new(m.values().flatten()),
        };
        lines.map(String::as_str)
    }

    /// The lines for one engine.
    pub fn for_engine(&self, engine: &str) -> Vec<String> {
        match self {
            Reproduce::Lines(l) => l.clone(),
            Reproduce::ByEngine(m) => m
                .get(engine)
                .or_else(|| m.get(CLOUD_ALL))
                .cloned()
                .unwrap_or_default(),
        }
    }
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

/// The 95% lower bound that a rule must reach for a promotion when it names no bar of its own.
pub(crate) const DEFAULT_BAR: f64 = 0.8;

fn default_bar() -> f64 {
    DEFAULT_BAR
}

/// A pair of fixtures: one that holds the fault, and one that is clean.
///
/// The test of the catalog runs the check on each, and the rule fails its test when the fault
/// passes or the clean one fails. Paths are relative to the rule file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuleTest {
    /// A world document with the fault. The check must fail on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault: Option<PathBuf>,
    /// A world document with no fault. The check must pass on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clean: Option<PathBuf>,
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
    /// A jq program over the world that says when the rule applies. Use `.before != null` for a
    /// rule on a change. Use `.clusterUid != null` for a rule on an existing cluster. Use
    /// `.account != null` for a rule on the cloud account. No value means always. A rule that does
    /// not apply is `SKIP`, with the program as the reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// The check, when strata-preflight has one. A rule with none is a row of the catalog. The
    /// report prints it as `NOT CHECKED` with `reproduce`, or as out of scope for a tool rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<Check>,
    /// The fix for good, and its owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
    /// Commands a person runs by hand to see the same thing: the `curl`, `jq`, `aws`, or
    /// `gcloud` line. The report prints them for each failed check and each `NOT CHECKED` row.
    /// A template `{{ jq }}` over the world is filled in the report. A list holds the lines for
    /// every engine; a map holds a list for each engine, and `all` for the rest.
    #[serde(default, skip_serializing_if = "Reproduce::is_empty")]
    pub reproduce: Reproduce,
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
    /// The 95% lower bound that the runs of this rule must reach for a promotion. It is the bar of
    /// its promotion, or the default bar of a promotion.
    pub fn bar(&self) -> f64 {
        self.promotion.as_ref().map_or(DEFAULT_BAR, |p| p.bar)
    }

    /// True when the rule applies to `engine`.
    pub fn applies_to(&self, engine: &str) -> bool {
        self.engines
            .iter()
            .any(|e| e == CLOUD_ALL || e.eq_ignore_ascii_case(engine))
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
    /// Parse one section file, and compile each program of it with the variables that it has
    /// when it runs. `path` names the file in messages, and roots the fixture paths.
    ///
    /// # Errors
    ///
    /// [`Error::Catalog`] when the file does not parse, breaks a rule of the format, or holds a
    /// program that cannot run. Such a program does not compile, or it reads `$item` where a
    /// check has none.
    pub fn parse_section(text: &str, path: &Path) -> Result<Section> {
        let section = Self::parse_embedded(text, path)?;
        check_section_programs(&section, path)?;
        Ok(section)
    }

    /// Parse one section file as [`Catalog::parse_section`] does, but compile no program.
    ///
    /// The binary reads its own rule files with this. The compile of every program costs about a
    /// tenth of a second at each start. The tests of this crate compile every program of every
    /// file of the binary. A file that a person gives is read by `parse_section`.
    pub(crate) fn parse_embedded(text: &str, path: &Path) -> Result<Section> {
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
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when strata-preflight cannot read the file, and the errors of
    /// [`Catalog::parse_section`].
    pub fn load_section(path: &Path) -> Result<Section> {
        let text = std::fs::read_to_string(path).map_err(|e| crate::error::io(path, e))?;
        Self::parse_section(&text, path)
    }

    /// Add a section. A rule id that a section already has is an error, so a probes file cannot
    /// shadow a rule of the catalog by mistake.
    ///
    /// # Errors
    ///
    /// [`Error::Catalog`] when `section` has a rule id that the catalog has already. `path` names
    /// the file of `section` in the message.
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

/// One bound of a range: the operator and the version that it names.
fn parse_range(text: &str) -> Option<Vec<(&str, &str)>> {
    let mut bounds = Vec::new();
    for part in text.split_whitespace() {
        let (op, ver) = ["<=", ">=", "<", ">", "="]
            .into_iter()
            .find_map(|op| part.strip_prefix(op).map(|v| (op, v)))?;
        version::numbers(ver)?;
        bounds.push((op, ver));
    }
    if bounds.is_empty() {
        None
    } else {
        Some(bounds)
    }
}

/// Compare `release` with `bound`, or None when `release` does not parse as a version.
///
/// A bound that names three numbers is a version, and the order is [`version::compare`]: a
/// pre-release is below its version, so `4.10.3-rc.1` is below `4.10.3`. A bound that names fewer
/// is a release line: only the numbers that it names count, so `4.10.3` equals `4.10`.
fn compare_to_bound(release: &str, bound: &str) -> Option<Ordering> {
    let named = version::numbers(bound)?.len();
    let line = version::line(release, named)?;
    if named == 3 {
        return Some(version::compare(release, bound));
    }
    Some(version::compare(&line, bound))
}

/// True when `release` is in `range`. A release that does not parse is in each range, so the
/// command refuses it first: see [`require_release`].
pub fn release_in_range(release: &str, range: &str) -> bool {
    let Some(bounds) = parse_range(range) else {
        return true;
    };
    bounds.iter().all(|(op, bound)| {
        let Some(c) = compare_to_bound(release, bound) else {
            return true;
        };
        match *op {
            "<" => c == Ordering::Less,
            "<=" => c != Ordering::Greater,
            ">" => c == Ordering::Greater,
            ">=" => c != Ordering::Less,
            _ => c == Ordering::Equal,
        }
    })
}

/// Make sure that `release` is a Palette release that a range can order: one to three numbers, such
/// as `4.10` or `4.10.12`.
///
/// A release such as `4.11.x` does not parse, and [`release_in_range`] puts it in each range. So
/// each rule with `releases` applies to it, and the run can fail a check that the release does
/// not have.
///
/// # Errors
///
/// [`Error::Usage`] when `release` does not parse as a version.
pub fn require_release(release: &str) -> Result<()> {
    if version::numbers(release).is_some() {
        return Ok(());
    }
    Err(Error::Usage {
        message: format!("--release {release}: give a release such as 4.10 or 4.10.12"),
    })
}

#[cfg(test)]
mod tests;
