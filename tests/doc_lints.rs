//! The four clippy lints that keep the rules of "Comments" in `docs/src/development/code.md`.
//!
//! `just lint` runs clippy with `-D warnings`, so a lint that is on fails the run on a comment
//! that breaks it. These tests make sure that the lints stay on for each package, and that no
//! file turns one off.

use std::path::Path;

use crate::common::walk;

/// The lints, by their names in the table `[lints.clippy]`.
const DOC_LINTS: [&str; 4] = [
    "missing_errors_doc",
    "missing_panics_doc",
    "too_long_first_doc_paragraph",
    "doc_markdown",
];

/// The manifest of each package that has public documentation.
const MANIFESTS: [&str; 3] = [
    "Cargo.toml",
    "preflight/Cargo.toml",
    "report-endpoint/Cargo.toml",
];

/// The lines of the table `[lints.clippy]` of the manifest `manifest`, in the root of the repository.
fn clippy_lints(manifest: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(manifest);
    let text = std::fs::read_to_string(&path).unwrap();
    text.lines()
        .skip_while(|line| line.trim() != "[lints.clippy]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .map(|line| line.trim().to_string())
        .collect()
}

#[test]
fn the_manifest_of_each_package_denies_the_four_doc_lints() {
    for manifest in MANIFESTS {
        let lines = clippy_lints(manifest);
        for lint in DOC_LINTS {
            let denied = format!("{lint} = \"deny\"");
            assert!(
                lines.contains(&denied),
                "{manifest}: [lints.clippy] must hold `{denied}`, and it holds {lines:?}"
            );
        }
    }
}

#[test]
fn no_source_file_turns_off_one_of_the_four_doc_lints() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let own = root.join("tests/doc_lints.rs");
    let mut found = Vec::new();
    for dir in [
        "src",
        "tests",
        "preflight/src",
        "preflight/tests",
        "report-endpoint/src",
        "report-endpoint/tests",
    ] {
        for file in walk(&root.join(dir)) {
            if file == own || file.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&file).unwrap();
            for (n, line) in text.lines().enumerate() {
                if DOC_LINTS
                    .iter()
                    .any(|lint| line.contains(&format!("clippy::{lint}")))
                {
                    found.push(format!("{}:{}: {}", file.display(), n + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "fix the comment, and do not turn off the lint:\n{}",
        found.join("\n")
    );
}
