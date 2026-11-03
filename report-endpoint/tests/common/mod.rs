//! What the tests of the endpoint share: the files of the repository that they read.

// Each test file is a crate of its own and uses some of these functions, so the others look unused.
#![allow(dead_code)]

use std::path::Path;

/// The text of the file `path`, which is relative to the directory of the package.
pub fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
}

/// The text that a line `name=text` of a script, or `name = text` of a manifest, assigns, without the
/// quotes. The name starts the line.
pub fn assigned(file: &str, name: &str) -> String {
    file.lines()
        .find_map(|line| {
            line.strip_prefix(&format!("{name}="))
                .or_else(|| line.strip_prefix(&format!("{name} = ")))
        })
        .unwrap_or_else(|| panic!("the file has no line {name}=..."))
        .trim_matches(['"', '\''])
        .to_string()
}

/// The label of the weekly contract run, as its script assigns it.
pub fn contract_label() -> String {
    assigned(&read("../examples/live/contract-report.sh"), "label")
}
