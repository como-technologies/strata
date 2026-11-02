//! The `--rules` listing: the rules of the catalog as a list of text and as a document.

use serde_json::{Value, json};
use strata::closed_set::ClosedSet;

use crate::report::Tool;
use crate::rule::{Catalog, Level, Rule, Tier};

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
pub(crate) const RULES_SCHEMA: &str = "strata-preflight-rules/1";

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
