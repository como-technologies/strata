//! The book's own consistency: every use case is listed everywhere it belongs, in one order.
//!
//! `docs/src/use-cases/index.md` lists every page in the order that teaches the tool, and
//! `docs/src/SUMMARY.md` is the navigation beside it. A page added to one and forgotten in the
//! other is invisible to a reader who came the other way. That happened to "What changed between
//! two pack versions", which reached the navigation and never reached the list.

use crate::common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::{version_of, walk};
use strata::api::codes::ValidateRoute;

fn docs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/src")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The use-case pages one markdown text links to, in the order it links to them, without repeats.
///
/// A link is `<prefix><name>.md)`: the prefix is `use-cases/` in the navigation, and `](./` on
/// the index page, whose links are relative.
fn linked(text: &str, prefix: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(prefix) {
        rest = &rest[at + prefix.len()..];
        let Some(end) = rest.find(".md)") else { break };
        let name = &rest[..end];
        if !name.is_empty()
            && name != "index"
            && !name.contains('/')
            && seen.insert(name.to_string())
        {
            out.push(name.to_string());
        }
    }
    out
}

fn section<'a>(text: &'a str, from: &str, to: Option<&str>) -> &'a str {
    let start = text
        .find(from)
        .unwrap_or_else(|| panic!("the page has no {from:?} heading"));
    let rest = &text[start..];
    match to.and_then(|t| rest.find(t)) {
        Some(end) => &rest[..end],
        None => rest,
    }
}

#[test]
fn every_use_case_is_in_the_navigation_and_the_list_in_one_order() {
    let index = read(&docs().join("use-cases/index.md"));
    let summary = read(&docs().join("SUMMARY.md"));

    let nav = linked(&summary, "use-cases/");
    let listed_in_order = linked(
        section(&index, "## The pages, in order", Some("## The examples")),
        "](./",
    );

    assert_eq!(
        nav, listed_in_order,
        "the navigation and the list of pages name different pages, or name them in another order"
    );

    // and every page on disk is one of them
    let mut on_disk: Vec<String> = std::fs::read_dir(docs().join("use-cases"))
        .expect("the use-cases directory")
        .filter_map(|e| {
            let name = e.ok()?.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".md")?.to_string();
            (stem != "index").then_some(stem)
        })
        .collect();
    on_disk.sort();
    let mut listed = nav.clone();
    listed.sort();
    assert_eq!(
        listed, on_disk,
        "a use-case page exists that the book does not list, or the book lists one that is gone"
    );
}

/// The version badge in the README is static, because shields.io cannot read a private
/// repository: a badge that asks it for the version renders "repo not found". Static means it can
/// drift, so the drift is a test failure instead of a wrong number on the front page.
#[test]
fn the_readme_version_badge_is_the_version_in_cargo_toml() {
    let version = version_of("Cargo.toml");
    let readme = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md"));
    let badge = format!("badge/version-{version}-blue");
    assert!(
        readme.contains(&badge),
        "the README badge does not name version {version}: bump it with the version in Cargo.toml"
    );
}

/// A release ships `strata` and `strata-preflight` together, and the book says that
/// `strata-preflight --version` is the version of `strata`. The preflight crate has its own
/// manifest, so it can drift: `just version` rewrites it only while it names the same version.
#[test]
fn the_preflight_crate_has_the_version_of_strata() {
    let strata = version_of("Cargo.toml");
    let preflight = version_of("preflight/Cargo.toml");
    assert_eq!(
        preflight, strata,
        "preflight/Cargo.toml is {preflight} and Cargo.toml is {strata}: a release keeps them equal"
    );
}

#[test]
fn every_page_the_navigation_names_exists() {
    let summary = read(&docs().join("SUMMARY.md"));
    let mut rest = summary.as_str();
    let mut checked = 0;
    while let Some(at) = rest.find("](./") {
        rest = &rest[at + 2..];
        let Some(end) = rest.find(')') else { break };
        let link = &rest[..end];
        let page = docs().join(link.trim_start_matches("./"));
        assert!(
            page.is_file(),
            "SUMMARY.md names {link}, which is not a file"
        );
        checked += 1;
    }
    assert!(
        checked > 40,
        "only {checked} pages checked; the parsing broke"
    );
}

/// `strata schema` grew the kind `packs`, and the two hand-written tables that list the kinds did
/// not. One of them sits above the generated help that names it.
#[test]
fn the_kind_tables_list_every_kind_the_help_names() {
    let help = read(&docs().join("output/help-schema.txt"));
    let at = help
        .find("possible values: ")
        .expect("the help lists the kinds");
    let list = &help[at + "possible values: ".len()..];
    let list = &list[..list.find(']').expect("the list ends")];
    let kinds: Vec<&str> = list.split(", ").collect();
    assert!(kinds.len() > 5, "only {} kinds parsed", kinds.len());

    for (page, cell) in [
        ("commands/schema.md", "| `{}` |"),
        ("reference/documents.md", "| `{}` |"),
    ] {
        let text = read(&docs().join(page));
        for kind in &kinds {
            if *kind == "candidate" && page == "reference/documents.md" {
                continue; // the candidate file has no schema id, so that table leaves it out
            }
            let row = cell.replace("{}", kind);
            assert!(
                text.contains(&row),
                "{page} does not list the schema kind {kind}"
            );
        }
    }
}

/// The text of `reference/errors.md` that names `route`. The table of its codes follows the first
/// place that holds this text.
fn route_anchor(route: ValidateRoute) -> &'static str {
    match route {
        ValidateRoute::ClusterPacks => "`POST /v1/spectroclusters/{uid}/validate/packs`",
        ValidateRoute::ProfilePacks => "`POST /v1/clusterprofiles/validate/packs`",
        ValidateRoute::ProfileName => "`GET /v1/clusterprofiles/validate/name`",
        ValidateRoute::ProfileImport => "`POST /v1/clusterprofiles/import/validate`",
        ValidateRoute::ClusterCreate => "`POST /v1/spectroclusters/{cloudType}/validate`",
    }
}

/// The codes of the first table of codes after `anchor`, sorted: the first cell of each row,
/// without its backticks. A code that the table holds two times is two entries.
fn codes_after(text: &str, anchor: &str) -> Vec<String> {
    let at = text
        .find(anchor)
        .unwrap_or_else(|| panic!("errors.md does not name {anchor}"));
    let mut codes: Vec<String> = text[at..]
        .lines()
        .skip_while(|line| !line.starts_with("| Code |"))
        .skip(2) // the header and the line under it
        .take_while(|line| line.starts_with('|'))
        .map(|row| {
            let cell = row.split('|').nth(1).expect("a row has a first cell");
            cell.trim().trim_matches('`').to_string()
        })
        .collect();
    codes.sort();
    codes
}

/// `codes.rs` keeps the codes of each validate route, and `errors.md` copies them by hand. A code
/// added to one place and not to the other is a wrong answer in the book.
#[test]
fn the_code_table_of_each_validate_route_in_the_book_holds_the_codes_of_the_route() {
    let errors = read(&docs().join("reference/errors.md"));
    for route in ValidateRoute::ALL {
        let mut kept: Vec<String> = route.refusal_codes().map(str::to_string).collect();
        kept.sort();
        let book = codes_after(&errors, route_anchor(route));
        assert!(
            !book.is_empty(),
            "{route:?}: errors.md has no table of codes"
        );
        assert_eq!(
            book, kept,
            "{route:?}: the table in errors.md and ValidateRoute::refusal_codes differ"
        );
    }
}

/// The fields that release 0.18.0 added to a record, with no new schema id. A reader of an older
/// record must know the release, and what a field that is absent or null means.
const RELEASE_0_18_0_FIELDS: &[&str] = &[
    "pendingRepave",
    "rbacs",
    "namespaces",
    "resources",
    "rbac",
    "updateWorkerPoolsInParallel",
    "controlPlaneHealthCheckTimeout",
    "clusterMetaAttribute",
    "applySetting",
    "agentUpgrades",
    "cloudType",
    "invalid",
    "invalidReason",
    "masked",
    "installOrder",
    "containerRegistryUid",
];

/// The page of the compatibility rules named the release of Wave 4 as "after 0.17.0", and it left
/// most of the fields of that wave out. A reader who holds an older record cannot tell a field
/// that Palette did not give from a field that the release did not write.
#[test]
fn compatibility_names_release_0_18_0_and_what_each_absent_field_of_wave_4_means() {
    let page = read(&docs().join("reference/compatibility.md"));
    let table = section(&page, "### The fields of release 0.18.0", Some("\n### "));
    let rows: Vec<&str> = table.lines().filter(|l| l.starts_with('|')).collect();
    for field in RELEASE_0_18_0_FIELDS {
        let name = format!("`{field}`");
        assert!(
            rows.iter().any(|row| row.contains(&name)),
            "the table of release 0.18.0 in compatibility.md has no row for {name}"
        );
    }

    // a line break can fall inside a sentence
    let flat = page.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("`repavePending` joined `source.cluster` in 0.18.0"),
        "compatibility.md does not name the release that added repavePending"
    );
    assert!(
        flat.contains("One root changed in 0.18.0"),
        "compatibility.md does not name the release that changed the root of an app profile"
    );
    assert!(
        !flat.contains("after 0.17.0"),
        "compatibility.md names the release before 0.18.0 in place of 0.18.0"
    );
}

/// The page of the fan-out showed the installer with no word of the signature check. A reader who
/// copies the command then installs a binary that nothing proved.
#[test]
fn publish_fan_out_runs_the_installer_with_the_signature_check_and_says_what_it_checks() {
    let page = read(&docs().join("use-cases/publish-fan-out.md"));
    let commands: Vec<&str> = page
        .lines()
        .filter(|l| l.contains("install-strata.sh") && !l.starts_with('#'))
        .collect();
    assert!(
        !commands.is_empty(),
        "publish-fan-out.md has no install-strata.sh command"
    );
    for command in commands {
        assert!(
            command.contains("--require-signature"),
            "publish-fan-out.md runs the installer with no --require-signature: {command}"
        );
    }

    // a line break can fall inside a sentence
    let flat = page.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("signed `SHA256SUMS`"),
        "publish-fan-out.md does not say that the installer checks the signature of SHA256SUMS"
    );
    assert!(
        flat.contains("(../install.md#check-the-signature)"),
        "publish-fan-out.md does not link to the identity that the signature must have"
    );
}

/// Each variable of the GitLab preflight job has its description on `gitlab.md`, and no other page
/// lists them. Two pages that list them need two changes for one default. A page can name one
/// variable to explain a behavior, and then it links to `gitlab.md`.
#[test]
fn the_variables_of_the_gitlab_preflight_job_are_described_on_gitlab_md_and_listed_nowhere_else() {
    let drop_in: serde_yaml::Value = serde_yaml::from_str(&read(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/ci/gitlab/strata.yml"),
    ))
    .unwrap();
    let names: Vec<&str> = drop_in["variables"]
        .as_mapping()
        .unwrap()
        .keys()
        .filter_map(|k| k.as_str())
        .filter(|k| k.starts_with("STRATA_PREFLIGHT_"))
        .collect();
    assert!(names.len() >= 17, "the drop-in sets {names:?}");

    // the table abbreviates a run of names: `STRATA_PREFLIGHT_CLOUD`, `_TIER`, `_RECORDS`
    let gitlab = read(&docs().join("automation/gitlab.md"));
    let table = section(&gitlab, "## The variables", Some("\n## "));
    for name in &names {
        let short = format!("`{}`", name.trim_start_matches("STRATA_PREFLIGHT"));
        assert!(
            table.contains(name) || table.contains(&short),
            "the table of gitlab.md does not describe {name}"
        );
    }

    // the generated output holds what strata prints, and live-runs.md holds the checks of the lab
    for page in walk(&docs()) {
        if page.extension().is_none_or(|e| e != "md")
            || page.starts_with(docs().join("output"))
            || page.ends_with("automation/gitlab.md")
            || page.ends_with("development/live-runs.md")
        {
            continue;
        }
        let text = read(&page);
        // a list can write the prefix once, and then each name as `CLOUD`
        let named = names
            .iter()
            .filter(|n| {
                let bare = format!("`{}`", n.trim_start_matches("STRATA_PREFLIGHT_"));
                text.contains(**n) || text.contains(&bare)
            })
            .count();
        assert!(
            named <= 2,
            "{} names {named} variables of the preflight job: link to gitlab.md in place of a list",
            page.display()
        );
    }
    let preflight = read(&docs().join("automation/preflight.md"));
    assert!(preflight.contains("(./gitlab.md#the-variables)"));
}

/// The use case gave the fix for a team and left out two steps of the workflow: the report as a
/// comment, and the watch. A reader who follows the use case then builds the old job.
#[test]
fn stop_a_build_shows_the_comment_and_the_watch_of_the_workflow() {
    let page = read(&docs().join("use-cases/stop-a-build.md"));
    let runs = section(&page, "## Where this runs", None);
    // a line break can fall inside a sentence
    let flat = runs.split_whitespace().collect::<Vec<_>>().join(" ");
    for needle in [
        "`preflight-comment.yml`, or `pr-comment.sh --preflight` on another host, posts the report as a comment",
        "(../automation/preflight.md#the-report-as-a-comment)",
        "In the workflow, give the input `watch`",
        "(../automation/preflight.md#the-watch-in-the-workflow)",
    ] {
        assert!(flat.contains(needle), "stop-a-build.md lacks {needle:?}");
    }
    let preflight = read(&docs().join("automation/preflight.md"));
    for heading in [
        "### The report as a comment",
        "### The watch in the workflow",
    ] {
        assert!(
            preflight.contains(heading),
            "preflight.md has no {heading:?}"
        );
    }
}
