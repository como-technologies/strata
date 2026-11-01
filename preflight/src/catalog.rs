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
        let section: Section = Catalog::parse_embedded(text, &path)?;
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
    fn each_program_of_the_catalog_of_the_binary_compiles_with_the_variables_it_runs_with() {
        for (name, text) in SECTIONS {
            let path = rules_dir().join(format!("{name}.yaml"));
            if let Err(e) = Catalog::parse_section(text, &path) {
                panic!("{e}");
            }
        }
    }

    #[test]
    fn each_reproduce_line_names_the_default_url_that_strata_connects_to() {
        let mut found = 0;
        for (section, text) in SECTIONS {
            for part in text.split("${PALETTE_API:-").skip(1) {
                let url = part.split('}').next().unwrap_or_default();
                assert_eq!(url, strata::config::DEFAULT_BASE_URL, "{section}");
                found += 1;
            }
        }
        assert!(found > 0, "no rule names the default URL");
    }

    /// Nothing sets `PALETTE_HOST`, so a line that reads it asks `curl` for `/v1/...` with no host.
    /// Each line that calls Palette reads `PALETTE_API` and sends the header `ProjectUid`, as
    /// `curl_get` of the report does.
    #[test]
    fn each_reproduce_line_that_calls_palette_has_one_form() {
        let mut wrong = Vec::new();
        let mut calls = 0;
        for (section, text) in SECTIONS {
            for (n, line) in text.lines().enumerate() {
                if !(line.contains("PALETTE_API_KEY") || line.contains("PALETTE_HOST")) {
                    continue;
                }
                calls += 1;
                if line.contains("PALETTE_HOST")
                    || !line.contains("${PALETTE_API:-")
                    || !line.contains("ProjectUid: $PALETTE_PROJECT_UID")
                {
                    wrong.push(format!("{section}.yaml:{}: {}", n + 1, line.trim()));
                }
            }
        }
        assert!(calls > 20, "the test reads the lines that call Palette");
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn the_ledger_of_the_binary_parses() {
        crate::confidence::Ledger::parse(LEDGER, Path::new("evidence/runs.yaml")).unwrap();
    }
}
