//! The crate root, `src/lib.rs`, and the paths of the modules that moved to `model/`.
//!
//! `docs/src/development/code.md` says where a shared module goes: a type of Palette in `model/`,
//! a read of Palette in `api/`, and a helper of the whole crate at the root. These tests keep the
//! root to the modules that are neither.

use std::collections::BTreeSet;
use std::path::Path;

use crate::common::walk;

/// The modules of the crate root. Each is a layer of strata or a helper of the whole crate.
/// No type of Palette is here: it goes in `model/`, and a read of Palette goes in `api/`.
const ROOT_MODULES: [&str; 17] = [
    // The layers.
    "api",
    "candidate",
    "diff",
    "error",
    "ops",
    "reference",
    "render",
    "snapshot",
    "term",
    // The content that the hashes read, and the types of Palette.
    "content",
    "model",
    // The helpers of the whole crate.
    "closed_set",
    "config",
    "file",
    "json",
    "text",
    "timestamp",
];

/// The names that `lib.rs` declares as modules, as `pub mod NAME;` or `pub(crate) mod NAME;`.
fn declared_modules() -> BTreeSet<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs");
    std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .filter_map(|line| {
            line.strip_prefix("pub mod ")
                .or_else(|| line.strip_prefix("pub(crate) mod "))
        })
        .map(|name| name.trim_end_matches(';').to_string())
        .collect()
}

/// The files and the directories of `src/` that are a module of the library: not `lib.rs`, the
/// binary `main.rs`, or `cli/`, which `main.rs` declares.
fn module_entries() -> BTreeSet<String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    std::fs::read_dir(src)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| !["lib.rs", "main.rs", "cli"].contains(&name.as_str()))
        .map(|name| name.trim_end_matches(".rs").to_string())
        .collect()
}

#[test]
fn the_crate_root_holds_no_module_of_a_type_of_palette() {
    let allowed: BTreeSet<String> = ROOT_MODULES.iter().map(|m| m.to_string()).collect();
    assert_eq!(
        declared_modules(),
        allowed,
        "src/lib.rs declares a module that ROOT_MODULES does not name, or the reverse. A type of \
         Palette goes in src/model/, and a read of Palette goes in src/api/"
    );
}

#[test]
fn each_file_and_directory_of_src_is_a_module_that_lib_declares() {
    assert_eq!(module_entries(), declared_modules());
}

/// Each Rust file of `src/`, with its path under `src/` and its text.
fn source_files() -> Vec<(String, String)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    walk(&src)
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .map(|path| {
            let name = path.strip_prefix(&src).unwrap().display().to_string();
            (name, std::fs::read_to_string(&path).unwrap())
        })
        .collect()
}

#[test]
fn no_file_of_src_keeps_the_old_path_of_a_module_that_moved_to_model() {
    let model = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/model");
    let modules: BTreeSet<String> = std::fs::read_dir(model)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .map(|name| name.trim_end_matches(".rs").to_string())
        .filter(|name| name != "mod")
        .collect();
    let kept: Vec<String> = source_files()
        .iter()
        .flat_map(|(file, text)| {
            text.lines()
                .filter_map(|line| line.strip_prefix("pub use "))
                .map(|path| path.trim_end_matches(';'))
                .filter_map(|path| {
                    path.strip_prefix("crate::model::")
                        .or(path.strip_prefix("model::"))
                })
                .filter(|name| modules.contains(*name))
                .map(move |name| format!("{file}: pub use model::{name}"))
        })
        .collect();
    assert!(
        kept.is_empty(),
        "release 0.27.0 removed the old paths of the modules of model/: {kept:?}"
    );
}

#[test]
fn no_file_of_src_keeps_the_constants_of_a_cluster_state_that_release_0_27_0_removed() {
    let kept: Vec<String> = source_files()
        .into_iter()
        .filter(|(_, text)| {
            ["RUNNING_STATE", "ERROR_STATE", "DELETED_STATE"]
                .iter()
                .any(|name| text.contains(name))
        })
        .map(|(file, _)| file)
        .collect();
    assert!(
        kept.is_empty(),
        "`ClusterState::is` replaces the constants of a state: {kept:?}"
    );
}
