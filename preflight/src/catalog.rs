//! The rules the binary carries: one file for each section under `preflight/rules/`, in the
//! order of the report. The files are data, and the binary embeds them, so a run needs no file
//! beside it and a pipeline reads the same list with `--rules --json`.

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::rule::{Catalog, Section};

/// Each section file, in the order of the report.
pub const SECTIONS: &[(&str, &str)] = &[
    ("identity", include_str!("../rules/identity.yaml")),
    (
        "cloud-services",
        include_str!("../rules/cloud-services.yaml"),
    ),
    ("quotas", include_str!("../rules/quotas.yaml")),
    ("networking", include_str!("../rules/networking.yaml")),
    ("compute", include_str!("../rules/compute.yaml")),
    ("kubernetes", include_str!("../rules/kubernetes.yaml")),
    ("storage", include_str!("../rules/storage.yaml")),
    ("images", include_str!("../rules/images.yaml")),
    ("profiles", include_str!("../rules/profiles.yaml")),
    ("lifecycle", include_str!("../rules/lifecycle.yaml")),
    ("platform", include_str!("../rules/platform.yaml")),
    ("tools", include_str!("../rules/tools.yaml")),
    ("teardown", include_str!("../rules/teardown.yaml")),
];

/// The ledger of the runs of the lab, which the binary carries with the rules.
pub const LEDGER: &str = include_str!("../evidence/runs.yaml");

/// The directory of the rule files in the source tree, for the fixtures of the tests.
pub fn rules_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("rules")
}

/// The catalog of the binary.
pub fn builtin() -> Result<Catalog> {
    let mut catalog = Catalog::default();
    let dir = rules_dir();
    for (name, text) in SECTIONS {
        let path = dir.join(format!("{name}.yaml"));
        let section: Section = Catalog::parse_section(text, &path)?;
        if section.section != *name {
            return Err(crate::error::Error::Catalog {
                path,
                message: format!(
                    "the file is section '{}', and the catalog lists it as '{name}'",
                    section.section
                ),
            });
        }
        catalog.push(section, &path)?;
    }
    Ok(catalog)
}

/// The catalog of the binary, with each probes file after it.
pub fn with_probes(probes: &[PathBuf]) -> Result<Catalog> {
    let mut catalog = builtin()?;
    for path in probes {
        let section = Catalog::load_section(path)?;
        catalog.push(section, path)?;
    }
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builtin_catalog_parses_and_each_id_is_unique() {
        let c = builtin().unwrap();
        assert_eq!(c.sections.len(), SECTIONS.len());
        let ids: Vec<&str> = c.rules().map(|r| r.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len(), "an id is in the catalog two times");
        assert!(ids.len() >= 3, "{} rules", ids.len());
    }

    #[test]
    fn the_ledger_of_the_binary_parses() {
        crate::confidence::Ledger::parse(LEDGER, Path::new("evidence/runs.yaml")).unwrap();
    }
}
