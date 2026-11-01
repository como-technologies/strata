//! The crate root, `src/lib.rs`, and the old paths of the modules that moved to `model/`.
//!
//! `docs/src/development/code.md` says where a shared module goes: a type of Palette in `model/`,
//! a read of Palette in `api/`, and a helper of the whole crate at the root. These tests keep the
//! root to the modules that are neither.

use std::any::TypeId;
use std::collections::BTreeSet;
use std::path::Path;

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

/// True when the types `A` and `B` are one type.
fn same_type<A: 'static, B: 'static>() -> bool {
    TypeId::of::<A>() == TypeId::of::<B>()
}

#[test]
fn each_old_path_of_a_moved_module_names_the_type_of_its_new_path() {
    assert!(same_type::<strata::cloud::Cloud, strata::model::cloud::Cloud>());
    assert!(same_type::<
        strata::cluster_template::ClusterTemplateRef,
        strata::model::cluster_template::ClusterTemplateRef,
    >());
    assert!(same_type::<
        strata::conjunction::Conjunction,
        strata::model::conjunction::Conjunction,
    >());
    assert!(same_type::<
        strata::content::layer::Layer,
        strata::model::layer::Layer,
    >());
    assert!(same_type::<
        strata::oci_provider::OciProvider,
        strata::model::oci_provider::OciProvider,
    >());
    assert!(same_type::<
        strata::pack_state::PackState,
        strata::model::pack_state::PackState,
    >());
    assert!(same_type::<
        strata::pack_type::PackType,
        strata::model::pack_type::PackType,
    >());
    assert!(same_type::<
        strata::profile_type::ProfileType,
        strata::model::profile_type::ProfileType,
    >());
    assert!(same_type::<
        strata::repave::PendingRepave,
        strata::model::repave::PendingRepave,
    >());
    assert!(same_type::<
        strata::scope::ProfileScope,
        strata::model::scope::ProfileScope,
    >());
    assert!(same_type::<
        strata::validation::ProfileValidation,
        strata::model::validation::ProfileValidation,
    >());
}
