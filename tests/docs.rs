//! The book's own consistency: every use case is listed everywhere it belongs, in one order.
//!
//! `docs/src/use-cases/index.md` lists every page in the order that teaches the tool, and
//! `docs/src/SUMMARY.md` is the navigation beside it. A page added to one and forgotten in the
//! other is invisible to a reader who came the other way. That happened to "What changed between
//! two pack versions", which reached the navigation and never reached the list.

use crate::common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::{INSTALLING_WORKFLOWS, recipe, repo_file, schema, script, version_of, walk};
use strata::api::codes::ValidateRoute;
use strata::content::canonical_json;

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

/// `text` with each run of white space as one space, as a line break can fall inside a sentence.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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
    let readme = repo_file("README.md");
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
    let kinds = schema_kinds();
    assert!(kinds.len() > 5, "only {} kinds parsed", kinds.len());

    for (page, cell) in [
        ("commands/schema.md", "| `{}` |"),
        ("reference/documents.md", "| `{}` |"),
    ] {
        let text = read(&docs().join(page));
        for kind in &kinds {
            if *kind == "candidate" && page == "reference/documents.md" {
                continue; // the candidate file has no schema id, so that table omits it
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

/// The page of the compatibility rules named the release of Wave 4 as "after 0.17.0". It also did
/// not name most of the fields of that wave. A reader who holds an older record cannot tell a field
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

    let flat = one_line(&page);
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

/// The section of `release` in the compatibility page, such as "0.25.0", with each run of white
/// space as one space.
fn release_section(page: &str, release: &str) -> String {
    one_line(section(
        page,
        &format!("## Release {release}: changes that need action"),
        Some("\n## "),
    ))
}

/// The changes of Wave 11 were in a few paragraphs, and the page had no section that tells a
/// reader of the release notes what to do. The section sits above the one of release 0.24.0, and
/// the opening of the page links to both.
#[test]
fn compatibility_has_a_section_for_release_0_25_0_above_release_0_24_0_and_links_to_both() {
    let page = read(&docs().join("reference/compatibility.md"));
    let newer = page
        .find("\n## Release 0.25.0: changes that need action\n")
        .expect("the page has no section for release 0.25.0");
    let older = page
        .find("\n## Release 0.24.0: changes that need action\n")
        .expect("the page has no section for release 0.24.0");
    assert!(newer < older, "release 0.25.0 comes above release 0.24.0");

    let opening = &page[..page.find("\n## The documents").unwrap()];
    for anchor in [
        "(#release-0250-changes-that-need-action)",
        "(#release-0240-changes-that-need-action)",
    ] {
        assert!(opening.contains(anchor), "the opening lacks {anchor}");
    }

    let release = section(
        &page,
        "## Release 0.25.0: changes that need action",
        Some("\n## "),
    );
    for heading in [
        "### For a user of the command line or of a document\n",
        "### For a user of the CI drop-ins\n",
        "### For a user of the library of strata\n",
        "### For a user of the library of strata-preflight\n",
    ] {
        assert!(release.contains(heading), "the section lacks {heading:?}");
    }
}

/// Each reusable workflow that installs strata runs the action of `cosign` and checks the
/// signature. A caller whose organization limits the actions, or whose runner is not on GitHub,
/// failed at that step with no word of it in the page. The section names each workflow, the
/// action to allow, and the `strata-version` that has the option which the workflows give.
#[test]
fn the_section_of_release_0_25_0_names_each_workflow_that_checks_the_signature_and_what_a_caller_does()
 {
    let checking: Vec<String> = common::workflow_files()
        .into_iter()
        .filter(|file| {
            let text = common::workflow_text(file);
            text.contains("workflow_call:") && text.contains("sigstore/cosign-installer@")
        })
        .collect();
    assert_eq!(
        checking, INSTALLING_WORKFLOWS,
        "a reusable workflow runs cosign, and the list of the tests lacks it"
    );

    let release = release_section(&read(&docs().join("reference/compatibility.md")), "0.25.0");
    for file in INSTALLING_WORKFLOWS {
        assert!(
            release.contains(&format!("`{file}`")),
            "the section of release 0.25.0 does not name {file}"
        );
    }
    for needle in [
        "installs `cosign` with the action `sigstore/cosign-installer`",
        "runs the `install-strata.sh` of the tag in `strata-version`, with `--require-signature`",
        "`cosign verify-blob`",
        "must permit `sigstore/cosign-installer`",
        "`retire-gate.yml` also runs `actions/checkout`",
        "permit `actions/checkout` too",
        "reach github.com",
        "**`strata-version` must be v0.25.0 or later.**",
        "so the job stops with `unknown option`",
        "It exits `2` and installs nothing.",
    ] {
        assert!(
            release.contains(needle),
            "the section of release 0.25.0 lacks {needle:?}"
        );
    }
}

/// The kinds that `strata schema` takes, from its generated help.
fn schema_kinds() -> Vec<String> {
    let help = read(&docs().join("output/help-schema.txt"));
    let list = help
        .split("[possible values: ")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .expect("the help of schema lists its kinds");
    list.split(", ").map(str::to_string).collect()
}

/// The rename of the two read errors reached `strata-diff/1` and `strata-verify-summary/1`, which
/// carry the record of a cluster too. The page named only the record. A document that strata
/// only writes has no older place to read, so the page names each one, with the id rule that
/// allows the move.
#[test]
fn the_page_names_each_document_that_carries_the_record_of_a_cluster_with_the_rename_of_its_errors()
{
    let mut carrying = Vec::new();
    for kind in schema_kinds() {
        let document = schema(&kind);
        let cluster = &document["$defs"]["ClusterSource"]["properties"];
        if !cluster.is_object() {
            continue;
        }
        assert!(cluster["errors"].is_object(), "{kind}: no errors");
        for older in ["updatesPendingError", "pendingRepaveError"] {
            assert!(cluster.get(older).is_none(), "{kind} still has {older}");
        }
        carrying.push(
            document["properties"]["schema"]["const"]
                .as_str()
                .unwrap_or_else(|| panic!("{kind} has no id"))
                .to_string(),
        );
    }
    for id in [
        "strata-snapshot/1",
        "strata-diff/1",
        "strata-verify-summary/1",
    ] {
        assert!(
            carrying.iter().any(|c| c == id),
            "{id} has no cluster: {carrying:?}"
        );
    }

    let page = read(&docs().join("reference/compatibility.md"));
    let release = release_section(&page, "0.25.0");
    let documents = one_line(section(&page, "## The documents", Some("\n## ")));
    for id in &carrying {
        assert!(
            release.contains(&format!("`{id}`")),
            "the section of release 0.25.0 does not name {id}"
        );
        assert!(
            documents.contains(&format!("`{id}`")),
            "the rules of the ids do not name {id}"
        );
    }
    for needle in [
        "One exception is a field outside the root that only says why another field is absent.",
        "It can move with no new id.",
        "Releases 0.8.0 to 0.24.0 wrote `updatesPendingError`",
        "Strata only writes these two documents",
    ] {
        assert!(documents.contains(needle), "the rules lack {needle:?}");
    }
    assert!(
        !documents.contains("Release 0.24.0 wrote"),
        "only release 0.24.0 is named as the writer of the older fields"
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

    let flat = one_line(&page);
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
    let drop_in: serde_yaml::Value =
        serde_yaml::from_str(&repo_file("examples/ci/gitlab/strata.yml")).unwrap();
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

/// The use case gave the fix for a team and omitted two steps of the workflow: the report as a
/// comment, and the watch. A reader who follows the use case then builds the old job.
#[test]
fn stop_a_build_shows_the_comment_and_the_watch_of_the_workflow() {
    let page = read(&docs().join("use-cases/stop-a-build.md"));
    let runs = section(&page, "## Where this runs", None);
    let flat = one_line(runs);
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

// ---- the style review of Wave 11 -------------------------------------------------------------

/// The length of the mark of a list item at the start of `line`: `- `, `* `, or a number and `. `.
/// It is 0 when `line` starts no item.
fn list_marker(line: &str) -> usize {
    if line.starts_with("- ") || line.starts_with("* ") {
        return 2;
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        return digits + 2;
    }
    0
}

/// The blocks of `text`: each paragraph, each list item without its nested items, and each table
/// row. A heading and a fenced block are no block. A block is one line, without the mark of the
/// list.
fn blocks_of(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    let mut fenced = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            out.extend(current.take());
            fenced = !fenced;
            continue;
        }
        let ends_block = line.is_empty() || line.starts_with('#') || line.starts_with('|');
        let marker = list_marker(line);
        if fenced || ends_block || marker > 0 {
            out.extend(current.take());
        }
        if fenced || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('|') {
            out.push(line.to_string());
            continue;
        }
        let line = line[marker..].trim_start_matches('>').trim();
        if line.starts_with("{{#") {
            continue;
        }
        match current.as_mut() {
            Some(text) => {
                text.push(' ');
                text.push_str(line);
            }
            None => current = Some(line.to_string()),
        }
    }
    out.extend(current);
    out
}

/// The block of `text` that holds `anchor`. It is the first block of [`blocks_of`] with the text.
fn block(text: &str, anchor: &str) -> String {
    blocks_of(text)
        .into_iter()
        .find(|block| block.contains(anchor))
        .unwrap_or_else(|| panic!("no block holds {anchor:?}"))
}

/// The sentences of `block`. A sentence ends at one of three marks: a full stop, a question mark,
/// or an exclamation mark. The mark is outside a code span, and a space or the end follows it.
fn sentences(block: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_code = false;
    let mut chars = block.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        in_code ^= c == '`';
        let ends = matches!(c, '.' | '?' | '!') && chars.peek().is_none_or(|n| n.is_whitespace());
        if ends && !in_code {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

/// The words of a sentence: each run of characters between spaces that holds a letter or a
/// digit. The target of a link is not a word.
fn words(sentence: &str) -> usize {
    let mut text = String::new();
    let mut rest = sentence;
    while let Some(at) = rest.find("](") {
        text.push_str(&rest[..=at]);
        rest = rest[at + 2..]
            .find(')')
            .map_or("", |end| &rest[at + 2 + end + 1..]);
    }
    text.push_str(rest);
    text.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
}

/// The style review of Wave 11 found sentences over the limits of "The style rules". The pages
/// are in `docs/src/`, and the other files are in the repository. A procedure has 20 words at most,
/// and a description has 25.
#[test]
fn the_sentences_that_the_style_review_of_wave_11_named_keep_to_the_limits() {
    let sentences_to_check = [
        (
            "automation/preflight.md",
            "checks the signature of `SHA256SUMS`, installs the two binaries,",
            25,
        ),
        ("automation/preflight.md", "markdown to the job summary", 25),
        (
            "automation/preflight.md",
            "reads one variable for each input",
            25,
        ),
        ("development/build-test.md", "names it. For each option", 25),
        (
            "reference/documents.md",
            "`actorType` and `action`, as Palette",
            25,
        ),
        (
            "automation/gitlab.md",
            "downloads one release. When `cosign` 3 or later is on the",
            25,
        ),
        ("automation/gitlab.md", "To require the check, install", 20),
        (
            "automation/pr-comments.md",
            "The workflow installs the release binary",
            25,
        ),
        (
            "development/live-runs.md",
            "makes a checkout of the caller",
            25,
        ),
        (
            "development/live-runs.md",
            "#419: with `CLUSTER`, the record of the cluster",
            25,
        ),
        (
            "development/live-runs.md",
            "#158: `verify-report.sh --who` puts",
            25,
        ),
        (
            "automation/library.md",
            "`with_palette` adds the profiles",
            25,
        ),
        (
            "use-cases/edited-in-place.md",
            "Then the issue also lists each write",
            20,
        ),
        (
            "use-cases/no-digest.md",
            "Then give `--allow-no-digest`",
            20,
        ),
        ("use-cases/no-digest.md", "Give `--allow no-digest` to", 20),
    ];
    for (page, anchor, limit) in sentences_to_check {
        let text = read(&docs().join(page));
        let found = sentences(&block(&text, anchor));
        let sentence = found
            .iter()
            .find(|s| s.contains(anchor.split(". ").last().unwrap()))
            .unwrap_or_else(|| panic!("{page}: no sentence holds {anchor:?}"));
        assert!(
            words(sentence) <= limit,
            "{page}: {} words in a sentence of {limit} at most: {sentence}",
            words(sentence)
        );
    }

    // the cells of the table of the drop-ins: one short sentence for each trigger
    let table = repo_file("examples/ci/README.md");
    for row in ["| `github/strata.yml` |", "| `gitlab/strata.yml` |"] {
        for sentence in sentences(&block(&table, row)) {
            assert!(
                words(&sentence) <= 25,
                "examples/ci/README.md: {} words: {sentence}",
                words(&sentence)
            );
        }
    }
}

/// The style review of Wave 11 found paragraphs and list items over six sentences. A nested list
/// item counts as a block of its own.
#[test]
fn the_blocks_that_the_style_review_of_wave_11_named_have_six_sentences_or_fewer() {
    let blocks = [
        (
            "reference/compatibility.md",
            "After 0.24.0, a record of a cluster holds why",
        ),
        (
            "reference/compatibility.md",
            "A script that reads `updatesPendingError`",
        ),
        (
            "reference/compatibility.md",
            "After 0.24.0, the schema states the shape",
        ),
        ("reference/compatibility.md", "One case differs."),
        (
            "reference/compatibility.md",
            "Strata does not read a record that holds `name: null`",
        ),
        (
            "reference/compatibility.md",
            "A field can also become absent",
        ),
        ("reference/compatibility.md", "In 0.7.0 `source.consumers`"),
        (
            "reference/documents.md",
            "`tiers`: each tier that the record carries",
        ),
        ("reference/documents.md", "`who`, only with `--who`"),
        ("reference/documents.md", "Each changed value is a leaf"),
        ("reference/documents.md", "`segments` is the path as a list"),
        ("reference/documents.md", "`consumers[]`: `kind`"),
        (
            "reference/documents.md",
            "A cluster: `ref`, `name`, `uid`, `state`",
        ),
        ("development/build-test.md", "Each reusable workflow in"),
        ("use-cases/no-digest.md", "Where a pack has none"),
    ];
    for (page, anchor) in blocks {
        let found = sentences(&block(&read(&docs().join(page)), anchor)).len();
        assert!(
            found <= 6,
            "{page}: {found} sentences in the block of {anchor:?}"
        );
    }
}

/// The comment that holds the first comment line with `anchor`. The comment is the lines around it
/// that start with `//`, up to a blank comment line. The result is one line without the marks.
fn comment(text: &str, anchor: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let body = |i: usize| {
        let line = lines[i].trim_start();
        let marks = line.starts_with("//");
        let words = line.trim_start_matches('/').trim_start_matches('!').trim();
        (marks && !words.is_empty()).then_some(words)
    };
    let at = (0..lines.len())
        .find(|&i| body(i).is_some() && lines[i].contains(anchor))
        .unwrap_or_else(|| panic!("no comment line holds {anchor:?}"));
    let from = (0..=at).rev().take_while(|&i| body(i).is_some()).last();
    let to = (at..lines.len()).take_while(|&i| body(i).is_some()).last();
    let text: Vec<&str> = (from.unwrap_or(at)..=to.unwrap_or(at))
        .filter_map(body)
        .collect();
    text.join(" ")
}

/// The style review of Wave 11 found comments in the Rust files with a sentence over the limit of
/// 25 words for a description.
#[test]
fn the_rust_comments_that_the_style_review_of_wave_11_named_keep_to_25_words() {
    let comments = [
        (
            "src/snapshot/cluster.rs",
            "The cluster of a record of any release",
        ),
        (
            "src/snapshot/cluster.rs",
            "The parts of the cluster in `raw` that strata could not read",
        ),
        ("src/snapshot/pack.rs", "The schema of [`Pack::params`]"),
        (
            "src/snapshot/variable.rs",
            "The variable that strata keeps of `v`",
        ),
        (
            "xtask/src/chapters/repave.rs",
            "the table says repave for a new Kubernetes version",
        ),
        (
            "tests/ci_scripts.rs",
            "The options of a tool that a wrapper neither sets",
        ),
        (
            "tests/ci_scripts.rs",
            "A script passes to its command the options",
        ),
        (
            "tests/ci_scripts.rs",
            "the stand-in runs the real comparison without the option",
        ),
        (
            "tests/ci_scripts.rs",
            "a pull request with a comparison comment and no preflight",
        ),
        (
            "tests/cli.rs",
            "a value that Palette has today in one of these fields",
        ),
        (
            "tests/cli.rs",
            "True when the name of a property is the name of a time field",
        ),
        (
            "tests/live_scripts.rs",
            "`strata behind --json` gives `report`",
        ),
        (
            "tests/live_scripts.rs",
            "the stage edits the add-on through the API",
        ),
        ("tests/workflows.rs", "`act` is not in every container"),
        (
            "tests/workflows.rs",
            "GitHub fails a call from a caller that grants less",
        ),
        (
            "tests/workflows.rs",
            "The comment takes the report from the artifact",
        ),
        ("tests/docs.rs", "The block of `text` that holds"),
        ("tests/docs.rs", "The sentences of `block`."),
        (
            "tests/docs.rs",
            "The comment that holds the first comment line",
        ),
    ];
    for (file, anchor) in comments {
        for sentence in sentences(&comment(&repo_file(file), anchor)) {
            assert!(
                words(&sentence) <= 25,
                "{file}: {} words in a comment sentence of 25 at most: {sentence}",
                words(&sentence)
            );
        }
    }
}

/// The review of Wave 11 named phrasal verbs and "would" for a fact. Each file below held one.
#[test]
fn the_phrasal_verbs_and_the_would_of_the_wave_11_review_are_gone() {
    let phrases = [
        ("docs/src/install.md", "and goes on"),
        ("docs/src/development/build-test.md", "leave it out"),
        (
            "docs/src/development/live-runs.md",
            "turns the markdown report",
        ),
        ("docs/src/development/live-runs.md", "checks out the caller"),
        ("examples/ci/README.md", "turn it on"),
        ("examples/ci/github/strata.yml", "turn on the comment"),
        ("examples/ci/github/strata.yml", "to turn it on"),
        ("examples/live/loop.sh", "turns the markdown report"),
        ("examples/library-preflight/src/main.rs", "program would"),
        ("scripts/ci/pr-comment.sh", "goes on with the table"),
        ("tests/workflows.rs", "they would run two times"),
    ];
    for (file, phrase) in phrases {
        assert!(
            !repo_file(file).contains(phrase),
            "{file} still holds {phrase:?}"
        );
    }
}

// ---- the style sweep of Wave 12 ----------------------------------------------------------------

/// The sentences of a block. A table row has the sentences of each cell.
fn block_sentences(block: &str) -> Vec<String> {
    if block.starts_with('|') {
        return block
            .trim_matches('|')
            .split('|')
            .flat_map(|cell| sentences(cell.trim()))
            .collect();
    }
    sentences(block)
}

/// The pages that Wave 8 touched. The style review of Wave 8 found sentences of more than 25
/// words and blocks of more than six sentences in them.
const WAVE_8_PAGES: [&str; 31] = [
    "automation/drift-issues.md",
    "automation/gates.md",
    "automation/gitlab.md",
    "automation/library.md",
    "automation/merge-gate.md",
    "automation/pr-comments.md",
    "automation/preflight.md",
    "commands/behind.md",
    "commands/consumers.md",
    "commands/packs.md",
    "commands/same-release.md",
    "commands/snapshot.md",
    "commands/snapshot/cluster.md",
    "commands/snapshot/profile-version.md",
    "commands/verify.md",
    "development/build-test.md",
    "development/changes.md",
    "development/code.md",
    "development/documentation.md",
    "development/live-runs.md",
    "install.md",
    "internals/palette/packs.md",
    "internals/palette/profiles.md",
    "reference/candidate-file.md",
    "reference/compatibility.md",
    "reference/documents.md",
    "reference/errors.md",
    "reference/formats.md",
    "reference/palette-api.md",
    "reference/refs.md",
    "use-cases/publish-fan-out.md",
];

/// A description has 25 words at most, and a paragraph, a list item, or a table row has six
/// sentences at most.
#[test]
fn the_pages_that_wave_8_touched_keep_to_25_words_and_six_sentences() {
    let mut faults = Vec::new();
    for page in WAVE_8_PAGES {
        for block in blocks_of(&read(&docs().join(page))) {
            let found = block_sentences(&block);
            faults.extend(
                found
                    .iter()
                    .filter(|s| words(s) > 25)
                    .map(|s| format!("{page}: {} words: {}", words(s), s.trim())),
            );
            if !block.starts_with('|') && found.len() > 6 {
                faults.push(format!("{page}: {} sentences: {block}", found.len()));
            }
        }
    }
    assert!(
        faults.is_empty(),
        "{} faults:\n{}",
        faults.len(),
        faults.join("\n")
    );
}

/// The Rust comments that the style review of Wave 8 named: the crate doc and the comments of the
/// types of `same_release.rs`. Each sentence has 25 words at most.
#[test]
fn the_rust_comments_that_the_style_review_of_wave_8_named_keep_to_25_words() {
    let comments = [
        ("src/lib.rs", "Strata records, compares, and verifies"),
        ("src/lib.rs", "A profile is a stack of layers"),
        ("src/lib.rs", "A snapshot is the record of one published"),
        ("src/ops/same_release.rs", "The roots differ only by the"),
        ("src/ops/same_release.rs", "The key of each pack whose only"),
        (
            "src/ops/same_release.rs",
            "The differences between the records",
        ),
        (
            "src/ops/same_release.rs",
            "such a pair is `unknown-upstream`",
        ),
    ];
    for (file, anchor) in comments {
        for sentence in sentences(&comment(&repo_file(file), anchor)) {
            assert!(
                words(&sentence) <= 25,
                "{file}: {} words in a comment sentence of 25 at most: {sentence}",
                words(&sentence)
            );
        }
    }
}

/// The legend of a pair status is plain words for a reader without the book: each sentence has 25
/// words at most, and no sentence is a fragment after a semicolon.
#[test]
fn the_legend_of_a_pair_status_keeps_to_25_words_and_has_no_semicolon() {
    let legend = strata::closed_set::legend::legend::<strata::ops::PairStatus>();
    assert!(!legend.is_empty());
    for (status, text) in legend {
        assert!(!text.contains(';'), "{status}: {text}");
        for sentence in sentences(&text) {
            assert!(
                words(&sentence) <= 25,
                "{status}: {} words in a sentence of 25 at most: {sentence}",
                words(&sentence)
            );
        }
    }
}

/// Each phrase in `text` where "leave", "leaves", "left", or "leaving" has "out" within the next
/// four words in the same sentence.
fn leave_out_phrases(text: &str) -> Vec<String> {
    let words: Vec<&str> = text
        .split_whitespace()
        .filter(|w| !matches!(*w, "//" | "///" | "//!" | "#" | "*" | "-" | "|"))
        .collect();
    let bare = |w: &str| {
        w.trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase()
    };
    let mut found = Vec::new();
    for (at, word) in words.iter().enumerate() {
        if !matches!(bare(word).as_str(), "leave" | "leaves" | "left" | "leaving") {
            continue;
        }
        for offset in 1..=4 {
            let Some(next) = words.get(at + offset) else {
                break;
            };
            if bare(next) == "out" {
                found.push(words[at..=at + offset].join(" "));
                break;
            }
            if next.ends_with(['.', ';', ':', '?', '!']) {
                break;
            }
        }
    }
    found
}

/// The house word for "leave out" is "omit". A phrasal verb is not STE, and one word for one thing
/// keeps the text clear. This file holds the phrase in its own test data, so the scan skips it.
#[test]
fn no_file_of_the_tree_uses_the_phrasal_verb_leave_out() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let own = root.join("tests/docs.rs");
    let mut files: Vec<PathBuf> = [
        "adapter",
        "benches",
        "deploy",
        "docs/src",
        "examples",
        "fuzz",
        "preflight",
        "scripts",
        "src",
        "tests",
        "xtask",
        ".github",
    ]
    .iter()
    .flat_map(|dir| walk(&root.join(dir)))
    .collect();
    files.extend(
        ["README.md", "CONTRIBUTING.md", "CLAUDE.md", "justfile"]
            .iter()
            .map(|file| root.join(file)),
    );
    let mut faults = Vec::new();
    for file in files {
        let path = file.strip_prefix(root).unwrap_or(&file).to_string_lossy();
        let skipped = file == own
            || path.contains("/target/")
            || path.starts_with("examples/packs/")
            || path.starts_with("target/");
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        if skipped {
            continue;
        }
        faults.extend(
            leave_out_phrases(&text)
                .into_iter()
                .map(|phrase| format!("{path}: {phrase}")),
        );
    }
    assert!(
        faults.is_empty(),
        "write \"omit\" for \"leave out\":\n{}",
        faults.join("\n")
    );
}

/// The checks that the live window of Wave 11 ran have their result in the book. A row that says
/// "no result yet" for one of them tells a reader that nobody ran the check.
#[test]
fn the_rows_of_the_checks_that_the_wave_11_live_window_ran_state_a_result() {
    let page = read(&docs().join("development/live-runs.md"));
    for check in [
        "ask-repave-comment",
        "preflight-comment",
        "preflight-key-pair",
        "nightly-who",
        "bump-cluster-same",
        "bump-cluster-asked",
        "bump-cluster-refused",
        "cluster-route-doc",
        "validate",
    ] {
        let start = format!("| `{check}`");
        let row = page
            .lines()
            .find(|l| l.starts_with(&start))
            .unwrap_or_else(|| panic!("live-runs.md has no row for {check}"));
        assert!(
            !row.to_lowercase().contains("no result yet"),
            "the row of {check} says that it has no result: {row}"
        );
    }

    // stage 6 needs a cluster two minor versions behind, and a baseline check needs the same
    let packs = section(&page, "## validate-packs.sh", Some("\n## "));
    for needle in ["-var k8s_version=1.35.8", "TF_VAR_k8s_version=1.35.8"] {
        assert!(
            packs.contains(needle),
            "the section of validate-packs.sh does not give {needle}"
        );
    }
}

/// A rule that waits for a call of the route says why (#409), and a rule that a live run showed the
/// route answers waits no more. The lab has cloud accounts of AWS and GCP only, so the cases of the
/// other clouds have no result.
#[test]
fn a_rule_that_waits_for_the_route_says_why_and_a_rule_that_the_route_answers_does_not_wait() {
    let page = read(&docs().join("reference/preflight-rules.md"));
    let answers = section(
        &page,
        "### The checks that the route answers",
        Some("\n### "),
    );
    let waits = section(
        &page,
        "### The checks that wait for a call of the route",
        Some("\n### "),
    );
    let gaps = section(&page, "### The gaps that preflight fills", Some("\n## "));
    assert!(answers.contains("| `eks-ami-type` |"), "{answers}");
    for (name, table) in [("waits", waits), ("gaps", gaps)] {
        assert!(
            !table.contains("`eks-ami-type`"),
            "the rule eks-ami-type left the catalog, and the table {name} names it"
        );
    }
    let rows = table_rows(waits);
    assert!(!rows.is_empty(), "{waits}");
    for row in rows {
        assert!(
            row.len() == 3 && !row[2].is_empty(),
            "the rule {} waits with no reason: {row:?}",
            row[0]
        );
    }

    let runs = read(&docs().join("development/live-runs.md"));
    let route = section(&runs, "## validate-route.sh", Some("\n## route-bodies.sh"));
    for row in route
        .lines()
        .filter(|l| l.starts_with('|') && l.to_lowercase().contains("no result yet"))
    {
        assert!(
            row.contains("the lab has no cloud account of"),
            "a case with no result gives no reason: {row}"
        );
    }
    let row = route
        .lines()
        .find(|l| l.starts_with("| `eks-ami-type-al2`"))
        .expect("validate-route.sh has a row for the cases of eks-ami-type");
    for needle in ["UnsupportedAmiType", "IsValidValue"] {
        assert!(row.contains(needle), "{row}");
    }
    assert!(!row.to_lowercase().contains("no result yet"), "{row}");
}

// ---- the everyday loop -------------------------------------------------------------------------

/// The text of `docs/src/loop.md`.
fn the_loop() -> String {
    read(&docs().join("loop.md"))
}

/// The cells of each row of the first table in `text`, without the header and the rule line.
fn table_rows(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .filter(|l| l.starts_with('|'))
        .skip(2)
        .map(|l| {
            l.trim_matches('|')
                .split('|')
                .map(|c| c.trim().to_string())
                .collect()
        })
        .collect()
}

/// The code spans of `text`: the text between a pair of backticks.
fn code_spans(text: &str) -> Vec<&str> {
    text.split('`').skip(1).step_by(2).collect()
}

/// The words that start the command of a person: the commands of strata and the scripts of
/// `scripts/ci/`.
fn command_words() -> Vec<String> {
    let mut words = common::command_names();
    words.extend(
        std::fs::read_dir(script("ci"))
            .expect("scripts/ci exists")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned()),
    );
    words
}

/// Whether `text` shows a command to a person: a code span that starts with a command of strata,
/// a script, or an option.
fn names_a_command(text: &str, names: &[String]) -> bool {
    code_spans(text).iter().any(|span| {
        let first = span.split_whitespace().next().unwrap_or_default();
        first.starts_with('-') || first == "strata" || names.iter().any(|n| n == first)
    })
}

/// The first two paragraphs name the reason before the contrast. A name alone does not fix the
/// content of a version, so the record holds the content.
#[test]
fn the_everyday_loop_opens_with_the_reason_that_the_record_holds_the_content_and_not_only_the_name()
{
    let text = the_loop();
    let top = section(&text, "# The everyday loop", Some("\n## "));
    let want = "# The everyday loop \
        Strata is one loop, and each command is one step of it. A team reviews a change, \
        and the pipeline publishes it. A record in the repository keeps the content that \
        the team approved. From then on, each step compares Palette with that record. \
        Each difference reaches a person as a comment, a gate that fails, or an issue. \
        A name and a version do not tell you the content. Palette can update a \
        published version after its publish, and the name stays. So the record holds the \
        content, and not only the name. A record in Git, checked against Palette, shows \
        each change after the approval. \
        [Where each part belongs](./boundary.md) gives the reasons.";
    assert_eq!(one_line(top), want);
}

#[test]
fn a_day_of_work_gives_each_step_what_you_do_what_strata_tells_you_and_where_it_runs() {
    let text = the_loop();
    let part = section(&text, "## A day of work", Some("\n## "));
    let header = part
        .lines()
        .find(|l| l.starts_with('|'))
        .expect("the section has a table");
    assert_eq!(
        header, "| You do this | Strata tells you | Where it runs |",
        "the table of the day has three columns"
    );

    let names = command_words();
    let rows = table_rows(part);
    assert_eq!(rows.len(), 10, "the table of the day has ten steps");
    for row in rows {
        assert_eq!(row.len(), 3, "a row has three cells: {row:?}");
        for cell in &row {
            assert!(!cell.is_empty(), "a cell is empty in {row:?}");
        }
        for cell in &row[..2] {
            assert!(
                !names_a_command(cell, &names) && !cell.contains(".sh"),
                "the cell {cell:?} names a command"
            );
        }
    }
}

#[test]
fn the_chart_of_the_everyday_loop_has_a_lane_for_you_the_pipeline_and_the_night_and_no_box_names_a_command()
 {
    let text = the_loop();
    let part = section(&text, "## A day of work", Some("\n## "));
    let chart = section(part, "```mermaid", Some("\n```\n"));
    let names = command_words();

    let mut lanes: Vec<(String, Vec<String>)> = Vec::new();
    let mut nodes = BTreeSet::new();
    let mut edges = Vec::new();
    for line in chart.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("subgraph ") {
            let title = rest.split('"').nth(1).expect("a lane has a title");
            lanes.push((title.to_string(), Vec::new()));
        } else if line.contains("-->") {
            edges.extend(line.split("-->").map(|id| id.trim().to_string()));
        } else if let Some((id, label)) = line.split_once("[\"") {
            let (_, boxes) = lanes.last_mut().expect("a box is inside a lane");
            boxes.push(label.trim_end_matches("\"]").to_string());
            nodes.insert(id.to_string());
        }
    }

    let titles: Vec<&str> = lanes.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(
        titles,
        ["You, in a pull request", "Your pipeline", "Each night"]
    );
    for (title, boxes) in &lanes {
        assert!(!boxes.is_empty(), "the lane {title:?} has no box");
        for label in boxes {
            assert!(
                !names_a_command(label, &names) && !label.contains(".sh") && !label.contains('`'),
                "the box {label:?} names a command"
            );
        }
    }
    for id in edges {
        assert!(
            nodes.contains(&id),
            "an arrow starts or ends at {id:?}, which is no box"
        );
    }
}

/// The table of today became "The jobs and their commands", for the person who writes the
/// pipeline. It keeps each command and each page that the table had.
#[test]
fn the_jobs_and_their_commands_keep_each_command_and_each_link_of_the_table_of_today() {
    let text = the_loop();
    let part = section(&text, "## The jobs and their commands", Some("\n## "));
    let header = part
        .lines()
        .find(|l| l.starts_with('|'))
        .expect("the section has a table");
    assert_eq!(header, "| When | Who | The question | What runs | Page |");
    assert_eq!(table_rows(part).len(), 12, "the table has twelve jobs");

    let spans = code_spans(part);
    for command in [
        "snapshot --from-candidate --validate",
        "merge-gate.sh",
        "compare cluster:<name> <version> --repave",
        "pr-comment.sh --repave",
        "pr-comment.sh --preflight",
        "verify-report.sh",
        "snapshot filter:<name> --dir --prune",
        "upstream-report.sh",
        "list --pack",
        "consumers",
        "same-release",
        "verify --who",
        "report --link",
    ] {
        assert!(
            spans.iter().any(|s| s.starts_with(command)),
            "the table does not show `{command}`"
        );
    }
    for link in [
        "(./use-cases/review.md)",
        "(./automation/merge-gate.md)",
        "(./commands/compare/repave.md)",
        "(./commands/compare/validate.md)",
        "(./use-cases/promote.md)",
        "(./use-cases/stop-a-build.md)",
        "(./automation/drift-issues.md)",
        "(./use-cases/fleet.md)",
        "(./automation/upstream-issues.md)",
        "(./use-cases/find-a-pack.md)",
        "(./use-cases/retire.md)",
        "(./use-cases/unknown-value.md)",
    ] {
        assert!(part.contains(link), "the table does not link {link}");
    }
}

/// The heading "Who runs what, and when" became two sections. `just docs-check` fails a link to a
/// heading that a page does not have, so this test needs only the headings and the two new links.
#[test]
fn the_everyday_loop_has_the_two_new_sections_and_the_quick_start_and_the_commands_link_them() {
    let text = the_loop();
    for heading in ["\n## A day of work\n", "\n## The jobs and their commands\n"] {
        assert!(text.contains(heading), "loop.md has no heading {heading:?}");
    }
    assert!(
        !text.contains("## Who runs what, and when"),
        "loop.md still has the old heading"
    );

    for (page, link) in [
        ("quickstart.md", "loop.md#a-day-of-work"),
        ("commands/index.md", "loop.md#the-jobs-and-their-commands"),
    ] {
        assert!(
            read(&docs().join(page)).contains(link),
            "{page} does not link {link}"
        );
    }
}

// ---- the text review at the end of Wave 12 -----------------------------------------------------

/// The blocks of the book that the review at the end of Wave 12 found over the limits. A sentence
/// has the limit of its entry, and a block has six sentences at most.
#[test]
fn the_blocks_that_the_review_at_the_end_of_wave_12_named_keep_to_the_limits() {
    let blocks = [
        (
            "commands/preflight/rules.md",
            "A file with a program that cannot run",
            25,
        ),
        (
            "commands/preflight/watch.md",
            "`--watch cluster:UID` reads",
            25,
        ),
        ("commands/preflight/watch.md", "`--verbose` prints", 25),
        (
            "commands/preflight/watch.md",
            "To see a watch on a real build",
            25,
        ),
        (
            "commands/preflight/tiers.md",
            "A run sends each request one time",
            25,
        ),
        ("commands/preflight/tiers.md", "So `palette-validate`", 25),
        ("internals/hashes.md", "The JSON is also YAML", 25),
        ("internals/hashes.md", "These are DEL", 25),
        (
            "development/documentation.md",
            "Find the chapter of the page",
            20,
        ),
        (
            "reference/refs.md",
            "`snapshot --raw-dir` takes a profile or a cluster",
            25,
        ),
    ];
    for (page, anchor, limit) in blocks {
        let found = sentences(&block(&read(&docs().join(page)), anchor));
        assert!(
            found.len() <= 6,
            "{page}: {} sentences in the block of {anchor:?}",
            found.len()
        );
        for sentence in found {
            assert!(
                words(&sentence) <= limit,
                "{page}: {} words in a sentence of {limit} at most: {sentence}",
                words(&sentence)
            );
        }
    }
}

/// The phrasal verbs that the review at the end of Wave 12 named in the book. Each file below held
/// one, and a line break can fall inside the phrase.
#[test]
fn the_phrasal_verbs_of_the_review_at_the_end_of_wave_12_are_gone_from_the_book() {
    for (page, phrase) in [
        ("commands/preflight/watch.md", "turns them off"),
        ("reference/refs.md", "read back"),
    ] {
        assert!(
            !one_line(&read(&docs().join(page))).contains(phrase),
            "{page} still holds {phrase:?}"
        );
    }
}

/// The comment blocks of a YAML file: each run of lines that start with `#`. A block is one line,
/// without the marks.
fn yaml_comments(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut run: Vec<&str> = Vec::new();
    for line in text.lines().map(str::trim_start) {
        match line.strip_prefix('#').map(str::trim) {
            Some(words) if !words.is_empty() => run.push(words),
            _ if !run.is_empty() => blocks.push(std::mem::take(&mut run).join(" ")),
            _ => {}
        }
    }
    blocks.extend((!run.is_empty()).then(|| run.join(" ")));
    blocks
}

/// A user copies the drop-ins of the CI systems, so their comments are user text. The review at the
/// end of Wave 12 named three comments that are over the limit: each sentence of them has 25 words
/// at most.
#[test]
fn the_comments_of_the_ci_drop_ins_that_the_review_named_keep_to_25_words() {
    let comments = [
        ("examples/ci/github/strata.yml", "needed to read Palette"),
        (
            "examples/ci/github/strata.yml",
            "A token does not give access",
        ),
        ("examples/ci/gitlab/strata.yml", "needed to read Palette"),
    ];
    for (file, anchor) in comments {
        let block = yaml_comments(&repo_file(file))
            .into_iter()
            .find(|block| block.contains(anchor))
            .unwrap_or_else(|| panic!("{file}: no comment holds {anchor:?}"));
        for sentence in sentences(&block) {
            assert!(
                words(&sentence) <= 25,
                "{file}: {} words in a comment sentence of 25 at most: {sentence}",
                words(&sentence)
            );
        }
    }
}

/// Each place that says which version `@latest` picks says that a pre-release is not the pick.
#[test]
fn each_place_that_says_what_latest_picks_says_that_a_pre_release_is_not_the_pick() {
    for (page, anchor) in [
        ("use-cases/pack-versions.md", "`pack:<name>@latest` takes"),
        ("use-cases/pack-versions.md", "For `@latest`, strata takes"),
        ("commands/index.md", "| The highest version of a name"),
        ("reference/refs.md", "| `<name>@latest` |"),
        ("reference/refs.md", "| `pack:<name>@latest` |"),
    ] {
        let found = block(&read(&docs().join(page)), anchor);
        assert!(
            found.contains("not a pre-release"),
            "{page}: the block of {anchor:?} does not say that a pre-release is not the pick: {found}"
        );
    }
}

/// A gate that fails does not wait, and the last cell of a row names a place, not a command.
#[test]
fn the_merge_step_of_a_day_of_work_says_that_the_gate_fails_and_each_step_names_a_place() {
    let text = the_loop();
    let part = section(&text, "## A day of work", Some("\n## "));
    let names = command_words();
    for row in table_rows(part) {
        let place = &row[2];
        assert!(
            !names.contains(place) && place != "preflight",
            "the place {place:?} is the name of a command"
        );
        if row[0] == "You merge" {
            assert!(
                row[1].contains("the gate fails") && !row[1].contains("waits"),
                "the merge step does not say that the gate fails: {}",
                row[1]
            );
        }
    }
}

/// The packages of the rustdoc builds of `just lint`, in the order of the recipe, as the list
/// that a sentence gives: "`a`, `b`, and `c`".
fn rustdoc_packages() -> String {
    let mut packages: Vec<String> = Vec::new();
    for line in recipe("lint").iter().filter(|l| l.contains("cargo doc")) {
        let words: Vec<&str> = line.split_whitespace().collect();
        for pair in words.windows(2).filter(|pair| pair[0] == "-p") {
            if !packages.iter().any(|package| package == pair[1]) {
                packages.push(pair[1].to_string());
            }
        }
    }
    let (last, first) = packages.split_last().expect("the recipe builds a rustdoc");
    let first: Vec<String> = first.iter().map(|p| format!("`{p}`")).collect();
    format!("{}, and `{last}`", first.join(", "))
}

/// The book states what the code does: the rustdoc of `just lint`, the property of the version
/// order, and the save of the group values of a cluster.
#[test]
fn build_test_states_the_rustdoc_packages_the_version_order_and_the_save_of_a_cluster() {
    let page = one_line(&read(&docs().join("development/build-test.md")));
    let want = format!(
        "`just lint` builds the rustdoc of {} two times",
        rustdoc_packages()
    );
    assert!(page.contains(&want), "build-test.md lacks {want:?}");
    assert!(
        page.contains("| The version order is a total preorder. |")
            && page.contains("`properties::the_version_order_is_a_total_preorder`")
            && !page.contains("a total order"),
        "build-test.md does not call the version order a total preorder"
    );
    assert!(
        page.contains(
            "`RawCluster::save_dir` masks each password in the values of a cluster group"
        ) && !page.contains("The save of a cluster with `--raw-dir`"),
        "build-test.md does not name the function that masks the group values"
    );
}

/// The public items show their comments to each user of the library, so each sentence of the
/// comments that the review at the end of Wave 12 named has 25 words at most, and none has a
/// phrasal verb that the review named.
#[test]
fn the_public_rustdoc_that_the_review_at_the_end_of_wave_12_named_keeps_to_25_words() {
    let comments = [
        (
            "src/reference/version.rs",
            "Palette orders by semantic version.",
        ),
        (
            "src/reference/version.rs",
            "A string that does not parse keeps",
        ),
        ("src/candidate/resolve.rs", "The documents are those that"),
        (
            "src/candidate/resolve.rs",
            "When the listing does not have the version",
        ),
        (
            "src/candidate/resolve.rs",
            "4. When the listing has no such version",
        ),
        (
            "src/content/canon.rs",
            "The rule: an object writes its keys",
        ),
        ("src/content/canon.rs", "2. Write the value with"),
        (
            "src/snapshot/mod.rs",
            "Give each pack the images and the charts",
        ),
        ("src/term/color.rs", "Each stream has its own color"),
        (
            "src/error/mod.rs",
            "The record of the update that waits for a cluster holds",
        ),
        ("src/diff/mod.rs", "The document describes itself"),
        ("preflight/src/html.rs", "The page shows the readiness"),
        (
            "src/api/raw.rs",
            "The raw API responses that a profile snapshot",
        ),
        (
            "src/api/raw.rs",
            "The raw API responses that a cluster snapshot",
        ),
    ];
    for (file, anchor) in comments {
        let text = comment(&repo_file(file), anchor);
        for sentence in sentences(&text) {
            assert!(
                words(&sentence) <= 25,
                "{file}: {} words in a comment sentence of 25 at most: {sentence}",
                words(&sentence)
            );
        }
        for phrase in ["read back", "them back", "spelled out"] {
            assert!(
                !text.contains(phrase),
                "{file}: the comment of {anchor:?} holds {phrase:?}"
            );
        }
    }
}

/// A block is one item of a list, whatever the mark of the item is, and a quote has no mark.
/// Before Wave 12 ended, `block` knew only the mark `- `, so it joined the items of a numbered
/// list into one block.
#[test]
fn a_block_is_one_item_of_a_numbered_list_a_star_list_or_a_quote_and_one_paragraph() {
    let text = "Intro.\n\n1. First item.\n   Same item.\n2. Second item.\n\n* Star one.\n* Star two.\n\n> Quoted\n> text.\n\n```text\nA fence.\n```\n\n| Row | Cell |\n";
    assert_eq!(block(text, "First"), "First item. Same item.");
    assert_eq!(block(text, "Second"), "Second item.");
    assert_eq!(block(text, "Star two"), "Star two.");
    assert_eq!(block(text, "Quoted"), "Quoted text.");
    assert_eq!(block(text, "Row"), "| Row | Cell |");
}

/// The book has numbered items that the old reader joined into one block. The item of the
/// setting of the access has its own block, and it holds the two sentences of the step.
#[test]
fn the_block_of_a_numbered_step_does_not_hold_the_next_step() {
    let page = read(&docs().join("automation/pr-comments.md"));
    let step = block(&page, "Accessible from repositories in the organization");
    assert!(
        step.starts_with("In this repository, open Settings"),
        "{step}"
    );
    assert!(!step.contains("strata-token"), "{step}");
}

// ---- the release notes and the live results at the end of Wave 12 -------------------------------

/// The changes of Wave 12 were in paragraphs that start "After 0.25.0", and most gave no action.
/// The section sits above the one of release 0.25.0, and the opening of the page links to all
/// three sections.
#[test]
fn compatibility_has_a_section_for_release_0_26_0_above_release_0_25_0_and_links_to_all_three() {
    let page = read(&docs().join("reference/compatibility.md"));
    let newer = page
        .find("\n## Release 0.26.0: changes that need action\n")
        .expect("the page has no section for release 0.26.0");
    let older = page
        .find("\n## Release 0.25.0: changes that need action\n")
        .expect("the page has no section for release 0.25.0");
    assert!(newer < older, "release 0.26.0 comes above release 0.25.0");

    let opening = &page[..page.find("\n## The documents").unwrap()];
    for anchor in [
        "(#release-0260-changes-that-need-action)",
        "(#release-0250-changes-that-need-action)",
        "(#release-0240-changes-that-need-action)",
    ] {
        assert!(opening.contains(anchor), "the opening lacks {anchor}");
    }

    let release = section(
        &page,
        "## Release 0.26.0: changes that need action",
        Some("\n## "),
    );
    for heading in [
        "### First: a saved directory of a cluster group can hold a password in clear\n",
        "### For a user of the command line or of a document\n",
        "### For a user of the CI drop-ins\n",
        "### For a user of the library of strata\n",
        "### For a user of the library of strata-preflight\n",
    ] {
        assert!(release.contains(heading), "the section lacks {heading:?}");
    }
    let first = release.find("### First:").unwrap();
    for heading in [
        "### For a user of the command line or of a document\n",
        "### For a user of the CI drop-ins\n",
    ] {
        assert!(
            first < release.find(heading).unwrap(),
            "the first part comes after {heading:?}"
        );
    }
}

/// The review of the end of Wave 12 found four changes that the page stated with no action, or
/// did not state: the directory of a cluster group, the root of a canonical text, the verdict of
/// the merge gate, and the artifact of the GitLab job. Each one has its action in the section.
#[test]
fn the_section_of_release_0_26_0_gives_the_action_for_the_directory_the_root_the_gate_and_the_job()
{
    let release = release_section(&read(&docs().join("reference/compatibility.md")), "0.26.0");
    for needle in [
        // A2: who has such a directory, and what to do
        "**Save each directory of a cluster group again, and search each repository that holds one.**",
        "`strata snapshot --raw-dir` refuses a cluster group in each release.",
        "Releases 0.20.0 to 0.25.0 did not mask a password in values with no canonical form.",
        "Releases 0.11.0 to 0.19.x masked no password of the group values.",
        "If the directory left the host, change each password that it held.",
        "git grep -c clustersConfig $(git rev-list --all -- '*cluster.json') -- '*cluster.json'",
        "**`save_dir` can give `serialize` now.**",
        "Release 0.25.0 saved such values with the password in clear.",
        // A3: the commands and the gates that show the root change, and the action
        "`compare` of an older record and a new record of one release shows `AUTHORED`",
        "`same-release` of the two gives `NOT THE SAME RELEASE` and exit `1`.",
        "Take each such record again with 0.26.0. Take the two records of a pair with one release.",
        "**A gate can fail for records of two releases.** The gates are `comment.yml`, `merge-gate.yml`, and `promote-gate.yml`.",
        // A4: the other verdict of the merge gate
        "now fails the gate with `BEHIND`",
        "A move from a pre-release to its final version, such as `1.21.1-rc.1` to `1.21.1`, passes now.",
        "A move from a version that does not parse to one that parses, such as `1.2.3.4` to `1.3.0`, fails the gate now.",
        "**Run `merge-gate.sh` with the strata of the same release.**",
        // A5: the artifact of the upstream report
        "**The GitLab job `strata-upstream-save` keeps the report as an artifact.**",
        "must name `strata-upstream-save`",
    ] {
        assert!(
            release.contains(needle),
            "the section of release 0.26.0 lacks {needle:?}"
        );
    }
}

/// The section lists the characters that the canonical text escapes, and the search command covers
/// the same characters. A change of the set must change the section too.
#[test]
fn the_section_of_release_0_26_0_names_the_characters_that_the_canonical_text_escapes() {
    let release = release_section(&read(&docs().join("reference/compatibility.md")), "0.26.0");
    assert!(
        release.contains(
            "The characters are DEL, U+0080 to U+009F, U+2028, U+2029, U+FFFE, and U+FFFF."
        ),
        "the section does not list the characters of the canonical text"
    );
    assert!(
        release.contains("'[\\x{7f}-\\x{9f}\\x{2028}\\x{2029}\\x{fffe}\\x{ffff}]'"),
        "the search command does not cover the same characters"
    );
    // the book lists the characters that the canonical text escapes, and no others
    let canon = |c: char| canonical_json(&serde_json::Value::String(c.to_string()));
    for c in [
        '\u{7f}', '\u{80}', '\u{85}', '\u{9f}', '\u{2028}', '\u{2029}', '\u{fffe}', '\u{ffff}',
    ] {
        assert_eq!(canon(c), format!("\"\\u{:04x}\"", c as u32), "{c:?}");
    }
    for c in ['\u{7e}', '\u{a0}', '\u{2027}', '\u{202a}', '\u{fffd}'] {
        assert_eq!(canon(c), format!("\"{c}\""), "{c:?} is escaped");
    }
}

/// The GitLab job that keeps the artifact is the job that the section names, and the job
/// `strata-upstream` keeps none.
#[test]
fn the_gitlab_job_that_the_section_names_keeps_the_artifact_and_strata_upstream_keeps_none() {
    let yml = repo_file("examples/ci/gitlab/strata.yml");
    let upstream = section(
        &yml,
        "\nstrata-upstream:\n",
        Some("\nstrata-upstream-save:\n"),
    );
    assert!(
        !upstream.contains("artifacts:"),
        "strata-upstream keeps an artifact"
    );
    let save = section(&yml, "\nstrata-upstream-save:\n", Some("\n# "));
    assert!(
        save.contains("paths: [\"$STRATA_SAVE\"]"),
        "strata-upstream-save keeps no artifact"
    );
}

/// The page said that release 0.25.0 panicked for a world whose metadata is not an object, and it
/// did not. The library gave a value and ran the checks. Only a base or a `_patch` that is not an
/// object, in a file with a key beside it, panicked. The two places that state it agree: the list
/// of the library of `strata-preflight`, and the section of release 0.26.0.
#[test]
fn compatibility_says_what_release_0_25_0_did_for_a_world_and_a_fixture_that_are_not_objects() {
    let page = one_line(&read(&docs().join("reference/compatibility.md")));
    for phrase in ["Before, it panicked.", "Before, each case panicked."] {
        assert!(!page.contains(phrase), "the page still says {phrase:?}");
    }
    for needle in [
        "Release 0.25.0 gave the value. `run::run` and `run::watch` give the same error. Release 0.25.0 ran the checks.",
        "Release 0.25.0 gave a value in these cases. One case panicked: a base or a `_patch` that is not an object, in a file with a key such as `_note`.",
    ] {
        assert_eq!(
            page.matches(needle).count(),
            2,
            "the list of the changes and the section of release 0.26.0 must each state {needle:?}"
        );
    }
}

/// The live window of Wave 12 ran `packs-palette-latest`. Its row states the result as a fact, with
/// no "no result yet": the marked version of each pack, and the count of versions.
#[test]
fn the_row_of_packs_palette_latest_states_the_result_of_the_live_window_of_wave_12() {
    let page = read(&docs().join("development/live-runs.md"));
    let row = page
        .lines()
        .find(|l| l.starts_with("| `packs-palette-latest`"))
        .expect("live-runs.md has no row for packs-palette-latest");
    assert!(
        !row.to_lowercase().contains("no result yet"),
        "the row says that the check has no result: {row}"
    );
    for fact in [
        "`1.37.0` of `kubernetes`, which has 137 versions",
        "`1.21.2` of `cert-manager`, which has 9",
        "the rebuilds `1.19.3-rev1` and `1.19.1-rev1` below their base",
    ] {
        assert!(row.contains(fact), "the row lacks {fact:?}: {row}");
    }
}

/// The pages of `behind` that depend on the order of versions of Palette link the section that
/// shows it live: the page of the command at "A pre-release", and step 4 of the use case.
#[test]
fn the_behind_pages_link_the_version_that_palette_calls_the_latest_from_the_pre_release() {
    let link = "(../internals/palette/packs.md#the-version-that-palette-calls-the-latest)";
    for (page, anchor) in [
        ("commands/behind.md", "A pipeline can publish a pre-release"),
        (
            "use-cases/behind.md",
            "A pipeline can publish a pre-release",
        ),
    ] {
        let found = block(&read(&docs().join(page)), anchor);
        assert!(
            found.contains(link),
            "{page}: the block of {anchor:?} does not link {link}: {found}"
        );
    }
    let packs = read(&docs().join("internals/palette/packs.md"));
    assert!(
        packs.contains("\n## The version that Palette calls the latest\n"),
        "packs.md has no section for the link"
    );
    let latest = one_line(section(
        &packs,
        "## The version that Palette calls the latest",
        Some("\n## "),
    ));
    assert!(
        latest.contains(
            "`kubernetes` and `cert-manager` in `Public Repo` each have one marked version"
        ) && latest.contains("The marked version is the newest version for strata."),
        "the section does not state the live result: {latest}"
    );
}

/// The lead read the cloud config of the AWS cluster of the live example, which sets no override of
/// a machine pool. The page of the clusters states what Palette gave.
#[test]
fn the_page_of_the_clusters_states_that_a_pool_with_no_override_has_no_override_field() {
    let page = one_line(&read(&docs().join("internals/palette/clusters.md")));
    for needle in [
        "Palette omits a field that has no value.",
        "no pool of its cloud config holds `overrideClusterAPIConfig`, `overrideHealthCheckConfiguration`, or `overrideKubeadmConfiguration`.",
        "Its `spec.clusterConfig` holds `hybridConfig`, `region`, and `sshKeyName`.",
    ] {
        assert!(page.contains(needle), "clusters.md lacks {needle:?}");
    }
}

/// A session of riff reads the code of Palette at `$HOME/palette`. `riff.toml` mounts the volume
/// with a comment that says what it holds, and the book states where the copy is and how old it can
/// be. A session that finds no mount cannot read the code that rule 7 tells it to read.
#[test]
fn riff_mounts_the_code_of_palette_and_the_book_says_where_a_session_finds_it() {
    let riff = repo_file("riff.toml");
    let lines: Vec<&str> = riff.lines().collect();
    let volumes = lines
        .iter()
        .position(|line| *line == "[session.volumes]")
        .expect("riff.toml has no [session.volumes] table");
    let mount = lines
        .iter()
        .position(|line| *line == "strata-palette = \"$HOME/palette\"")
        .expect("riff.toml does not mount strata-palette at $HOME/palette");
    assert!(volumes < mount, "the mount is outside [session.volumes]");
    assert!(
        lines[volumes..mount]
            .iter()
            .rev()
            .take_while(|line| line.starts_with('#'))
            .any(|line| line.contains("code of Palette")),
        "the mount has no comment that says what the volume holds"
    );

    let page = one_line(&read(&docs().join("development/changes.md")));
    for needle in [
        "A session reads the code of Palette at `$HOME/palette`.",
        "On another host, the volume is empty.",
        "so it can be some hours old.",
        "it asks the lead with `riff tell lead`.",
        "A session on the host itself, not in a container, has no `$HOME/palette`.",
        "the copy comes from the clones in `~/repos/spectrocloud/`.",
        "`palette-src-refresh`, in `~/.local/bin/`, pulls the clones and refreshes the copy.",
        "A session on the host runs it first, then reads the clones.",
    ] {
        assert!(page.contains(needle), "changes.md lacks {needle:?}");
    }
}

/// Rule 7 tells the author to read the code of Palette first, and still bars a citation of it. The
/// rule is in the book and in `CLAUDE.md`, and the two must agree.
#[test]
fn rule_7_says_to_read_the_code_of_palette_first_and_still_bars_a_citation() {
    let book = one_line(&read(&docs().join("development/documentation.md")));
    let guide = one_line(&repo_file("CLAUDE.md"));
    for (name, text) in [("documentation.md", &book), ("CLAUDE.md", &guide)] {
        for needle in [
            "7. **Read the code of Palette first.**",
            "where to find the latest code.",
            "no path, line number, or function name",
        ] {
            let found = text.to_lowercase().contains(&needle.to_lowercase());
            assert!(found, "{name}: rule 7 lacks {needle:?}");
        }
    }
}

// ---- the report of the unknown values ---------------------------------------------------------

/// The pages that describe `strata report`, and the page of the endpoint that takes the report.
const REPORT_PAGES: [&str; 4] = [
    "reference/report.md",
    "commands/report.md",
    "use-cases/unknown-value.md",
    "development/report-endpoint.md",
];

/// `text` without the code spans: the text between two backticks.
fn without_code(text: &str) -> String {
    text.split('`')
        .enumerate()
        .filter(|(n, _)| n % 2 == 0)
        .map(|(_, part)| part)
        .collect::<Vec<_>>()
        .join(" ")
}

/// A sentence has 25 words at most, and a paragraph, a list item, or a table row has six
/// sentences at most. The pages use none of the words that "The style rules" refuse, and no
/// phrasal verb that the reviews named.
#[test]
fn the_pages_of_the_report_keep_to_the_style_rules() {
    let refused = [
        "may", "might", "should", "would", "ensure", "ensures", "isn't", "doesn't", "can't",
        "won't", "don't", "it's", "that's",
    ];
    let phrasal = [
        "set up",
        "look up",
        "carry out",
        "leave out",
        "leaves out",
        "turn on",
        "turn off",
        "turns on",
        "turns off",
        "go on",
        "goes on",
        "stays out",
        "stay out",
        "read back",
    ];
    let mut faults = Vec::new();
    for page in REPORT_PAGES {
        let text = read(&docs().join(page));
        for block in blocks_of(&text) {
            let found = block_sentences(&block);
            faults.extend(
                found
                    .iter()
                    .filter(|s| words(s) > 25)
                    .map(|s| format!("{page}: {} words: {}", words(s), s.trim())),
            );
            if !block.starts_with('|') && found.len() > 6 {
                faults.push(format!("{page}: {} sentences: {block}", found.len()));
            }
            let prose = without_code(&block).to_lowercase();
            for word in prose.split(|c: char| !(c.is_alphanumeric() || c == '\'')) {
                if refused.contains(&word) {
                    faults.push(format!("{page}: the word {word:?} in: {block}"));
                }
            }
            for phrase in phrasal {
                if one_line(&prose).contains(phrase) {
                    faults.push(format!("{page}: the phrase {phrase:?} in: {block}"));
                }
            }
        }
    }
    assert!(
        faults.is_empty(),
        "{} faults:\n{}",
        faults.len(),
        faults.join("\n")
    );
}

/// `strata report` adds two changes that break code outside the crate: a required method of the
/// public trait `Renderer`, and a variant of `Error`, which is not `#[non_exhaustive]`. The section
/// of release 0.27.0 sits above the one of release 0.26.0. The opening of the page links to all
/// four sections, and the section gives the action for each change.
#[test]
fn the_section_of_release_0_27_0_gives_the_action_for_the_two_breaking_changes_of_the_report() {
    let page = read(&docs().join("reference/compatibility.md"));
    let newer = page
        .find("\n## Release 0.27.0: changes that need action\n")
        .expect("the page has no section for release 0.27.0");
    let older = page
        .find("\n## Release 0.26.0: changes that need action\n")
        .expect("the page has no section for release 0.26.0");
    assert!(newer < older, "release 0.27.0 comes above release 0.26.0");

    let opening = &page[..page.find("\n## The documents").unwrap()];
    assert!(
        one_line(opening).contains("Four sections list the changes that need action"),
        "the opening does not name four sections"
    );
    for anchor in [
        "(#release-0270-changes-that-need-action)",
        "(#release-0260-changes-that-need-action)",
        "(#release-0250-changes-that-need-action)",
        "(#release-0240-changes-that-need-action)",
    ] {
        assert!(opening.contains(anchor), "the opening lacks {anchor}");
    }

    let release = section(
        &page,
        "## Release 0.27.0: changes that need action",
        Some("\n## "),
    );
    let library = section(release, "### For a user of the library of strata\n", None);
    let renderer = block(library, "`render::Renderer`");
    for needed in ["`render_report`", "required", "Add `fn render_report"] {
        assert!(renderer.contains(needed), "the item lacks {needed:?}");
    }
    let error = block(library, "`error::Error`");
    for needed in [
        "`LogInvalid`",
        "`#[non_exhaustive]`",
        "wildcard arm",
        "Add a `_` arm",
    ] {
        assert!(error.contains(needed), "the item lacks {needed:?}");
    }
}

/// Release 0.27.0 removed the old paths of the modules that moved to `model/`, and the deprecated
/// constants of a cluster state. The section of the release gives the new path for each, and the
/// page no longer says that the old paths stay.
#[test]
fn the_section_of_release_0_27_0_gives_the_new_path_for_each_removed_old_path() {
    let page = read(&docs().join("reference/compatibility.md"));
    let release = section(
        &page,
        "## Release 0.27.0: changes that need action",
        Some("\n## "),
    );
    let library = section(release, "### For a user of the library of strata\n", None);
    let paths = block(library, "The old paths of the types of Palette are gone");
    for needed in [
        "`strata::cloud`",
        "`strata::content::layer`",
        "`strata::model::<module>`",
        "(#the-rust-library)",
    ] {
        assert!(paths.contains(needed), "the item lacks {needed:?}");
    }
    let constants = block(library, "`ops::RUNNING_STATE`");
    for needed in [
        "`ops::ERROR_STATE`",
        "`ops::DELETED_STATE`",
        "`model::cluster_state`",
        "`ClusterState::Running.is(text)`",
    ] {
        assert!(constants.contains(needed), "the item lacks {needed:?}");
    }

    let opening = one_line(&page[..page.find("\n## The documents").unwrap()]);
    assert!(
        opening.contains("release 0.27.0 removed the old paths"),
        "the page does not say that release 0.27.0 removed the old paths"
    );
    assert!(
        !opening.contains("They stay for one minor release"),
        "the page says that the constants of a state stay"
    );
}

// ---- the release notes and the text at the end of Wave 13 --------------------------------------

/// One part of the section of release 0.27.0 in compatibility.md, as one line. The part starts at
/// the heading `heading` and ends at the next heading of its level.
fn release_0_27_0_part(heading: &str) -> String {
    let page = read(&docs().join("reference/compatibility.md"));
    let release = section(
        &page,
        "## Release 0.27.0: changes that need action",
        Some("\n## "),
    );
    one_line(section(release, heading, Some("\n### ")))
}

/// The section of release 0.27.0 said nothing of the rule `eks-ami-type` and of five changes of
/// `strata-preflight` that can break a script or a pipeline. Each one has its action.
#[test]
fn the_section_of_release_0_27_0_names_each_change_of_strata_preflight_that_can_break_a_script() {
    let part = release_0_27_0_part("### For a user of the command line or of a document\n");
    for needle in [
        // the rule that left the catalog
        "**`strata-preflight`: the rule `eks-ami-type` is gone.**",
        "`--rule eks-ami-type` is a `usage` error with exit `2`, because the id is not in the catalog.",
        "With `--strict`, such a pool gave exit `1`, and now it gives exit `0`.",
        "A run of your ledger that names `eks-ami-type` counts for no rule.",
        "A run of your ledger that names `iam-ten-policies` counts for no rule. Rename it in `rule:`.",
        // the five changes
        "**`strata-preflight --promotion --release` counts only the runs of that release.**",
        "`gke-no-release-channel` is `demote` for each release below 4.11.",
        "`--promotion --release 4.10` gave exit `0` in 0.26.0, and it gives exit `1` now.",
        "A rule with older runs alone shows `no runs, older runs <n>`.",
        "`static-subnets` and `sg-name-collision` are in the `palette` tier.** `--tier cloud` does not run them.",
        "`aws-quotas`, `gcp-quotas`, and `azure-vcpu-quota` need the key and `spec.cloudAccountUid`.**",
        "A run with no key shows `NOT RUN` for them, and the readiness falls.",
        "A pool whose type the route does not list fails the rule.",
        "`instance-offering` sends `cpuGtEq=0&memoryGtEq=0`.** A small type, such as `t3.small`, no longer fails the rule.",
        // the command and the message that the draft of the review marked
        "**`strata report` is a new command.**",
        "Write `strata compare report <B>` for such a ref.",
        "**`api`: the message for a 403 with `ProjectUidIsNotEmpty` changed.**",
    ] {
        assert!(
            part.contains(needle),
            "the part of the command line lacks {needle:?}"
        );
    }
}

/// The rule `gke-no-release-channel` has the range `<4.11`, so the section says "below 4.11", and
/// the example of the page uses a release in that range.
#[test]
fn compatibility_says_that_gke_no_release_channel_is_demoted_below_4_11_only() {
    let rules = repo_file("preflight/rules/kubernetes.yaml");
    let rule = section(
        &rules,
        "  - id: gke-no-release-channel\n",
        Some("\n  - id: "),
    );
    assert!(rule.contains("releases: \"<4.11\""), "{rule}");

    let page = one_line(&read(&docs().join("reference/compatibility.md")));
    assert!(
        page.contains("`gke-no-release-channel` is `demote` for each release below 4.11, the range of the rule."),
        "the page does not give the range of the rule"
    );
    for old in [
        "is `demote` for each release until the ledger holds a run of the lab with a release",
        "`strata-preflight --promotion --release 4.11` counts the runs of `4.11` alone.",
    ] {
        assert!(!page.contains(old), "the page still says {old:?}");
    }
}

/// The section lacked six changes of the library `strata-preflight` that do not build, and the
/// text of the 5xx said that each 5xx other than a named code is an error.
#[test]
fn the_section_of_release_0_27_0_names_each_change_of_the_library_of_strata_preflight() {
    let part = release_0_27_0_part("### For a user of the library of strata-preflight\n");
    for needle in [
        "The list [After 0.26.0](#the-rust-library) gives the details.",
        "`confidence::promotion` has a third argument, `release: Option<&str>`.",
        "Give `None` for the result of 0.26.0.",
        "`confidence::Ledger::counts_for` counts a run with no release for no release.",
        "`confidence::Counts` has the new field `older: Vec<String>`",
        "`confidence::Confidence` has the new field `older_runs: u32`",
        "`confidence::PromotionReport` has the new field `release: Option<String>`.",
        "`confidence::Confidence::text` gives `no runs` for no run. It ends with `older runs <n>` when `older_runs` is not `0`.",
        "`rule::Check::Exec` holds a `rule::ExecCheck`.",
        "`Check::Exec(ExecCheck { argv, needs, timeout, palette: None, also: Vec::new(), expr })`",
        "`probe::Ran::Exec` has the new fields `also: Option<String>` and `request: Option<probe::Call>`.",
        "The catalog has no `eks-ami-type`, and `iam-ten-policies` is `iam-role-policy-quota`.",
        "`Catalog::rule(\"iam-ten-policies\")` gives `None`.",
    ] {
        assert!(
            part.contains(needle),
            "the part of the library of strata-preflight lacks {needle:?}"
        );
    }
}

/// A refusal of a validate route with HTTP 500 is an answer with no code in the list, for both
/// backends. The text said that `&[]` gives the result of 0.26.0, and that each 5xx other than a
/// named code is an error. Both are false for the refusal of a validate route.
#[test]
fn compatibility_says_that_a_refusal_of_a_validate_route_is_an_answer_with_no_code_in_the_list() {
    let page = one_line(&read(&docs().join("reference/compatibility.md")));
    let want = "So is a refusal of a validate route, such as `POST /v1/spectroclusters/{cloud}/validate`, as in 0.26.0.";
    assert_eq!(
        page.matches(want).count(),
        2,
        "the Rust library and the section of release 0.27.0 must each state {want:?}"
    );
    for needle in [
        "For `LiveBackend`, `&[]` gives the result of 0.26.0.",
        "For `FixtureBackend`, an entry with another 5xx is now an error, where 0.26.0 gave a `Response`",
        "is still `ERROR`, except a refusal of a validate route such as `POST /v1/spectroclusters/{cloud}/validate`.",
        "or for a refusal of a validate route.",
    ] {
        assert!(page.contains(needle), "the page lacks {needle:?}");
    }
    for old in [
        "Give `&[]` to get the result of 0.26.0.",
        "and an error for each other 5xx.",
        "Give `&[]` for the result of 0.26.0. A 5xx whose `code` is in the list is a `Response` with `ok: false`. Each other 5xx is an error.",
        "A 5xx with a code that the check does not name is still `ERROR`. The loader",
        "the check would read the refusal",
    ] {
        assert!(!page.contains(old), "the page still says {old:?}");
    }
}

/// Each row of `live-runs.md` that has no result gives its reason. A reason follows `no result yet`
/// after a colon, or after the words `for this form of the check`.
#[test]
fn each_row_of_live_runs_with_no_result_gives_its_reason() {
    let page = one_line(&read(&docs().join("development/live-runs.md")));
    let lower = page.to_lowercase();
    let mut found = 0;
    for (at, _) in lower.match_indices("no result yet") {
        found += 1;
        let rest = &page[at + "no result yet".len()..];
        assert!(
            rest.starts_with(": ") || rest.starts_with(" for this form of the check: "),
            "a place with no result gives no reason: {}",
            &page[at.saturating_sub(120)..(at + 60).min(page.len())]
        );
    }
    assert!(found > 0, "the page has no row without a result");
}

/// The rows that the end-of-wave review named for #390, #391, #530, #393, #539, #543, #540, #531,
/// and #515 have their results. The rows that wait for the endpoint of the report, the live run of
/// the fleet, and the other engines of the plan say what they wait for.
#[test]
fn the_rows_that_the_review_named_have_their_results_or_say_what_they_wait_for() {
    let page = read(&docs().join("development/live-runs.md"));
    let none = |row: &str| !row.to_lowercase().contains("no result yet");
    let contract = section(&page, "## contract.sh", Some("\n## "));
    for item in ["#390", "#393", "#530", "#539", "#540", "#543"] {
        let rows: Vec<&str> = contract
            .lines()
            .filter(|l| l.starts_with("| `contract.json` | ") && l.contains(item))
            .collect();
        assert!(!rows.is_empty(), "contract.sh has no row for {item}");
        assert!(rows.iter().all(|r| none(r)), "{item}: {rows:?}");
    }
    let profiles = section(&page, "## contract-profiles.sh", Some("\n## "));
    for row in profiles.lines().filter(|l| l.contains("#531")) {
        assert!(none(row), "{row}");
    }
    let tiers = section(&page, "## tiers.sh", Some("\n## "));
    let pool: Vec<&str> = tiers
        .lines()
        .filter(|l| l.starts_with("| `pool-"))
        .collect();
    assert_eq!(pool.len(), 9, "{pool:?}");
    assert!(pool.iter().all(|r| none(r)), "{pool:?}");
    let plan = section(&page, "## preflight-plan.sh", Some("\n## "));
    for name in [
        "instancetypes-cpu",
        "instancetypes-default-filter",
        "vpcs-route-keys",
        "zones-route-keys",
        "sgs-route-keys",
    ] {
        let row = plan
            .lines()
            .find(|l| l.starts_with(&format!("| `{name}` |")))
            .unwrap_or_else(|| panic!("preflight-plan.sh has no row for {name}"));
        assert!(none(row), "{row}");
    }

    let waits = |section_name: &str, anchor: &str, reason: &str| {
        let part = section(&page, section_name, Some("\n## "));
        let rows: Vec<&str> = part.lines().filter(|l| l.contains(anchor)).collect();
        assert!(!rows.is_empty(), "{section_name} has no row for {anchor}");
        for row in rows {
            assert!(row.contains(reason), "{row}");
        }
    };
    let endpoint = "No result yet: the lead has not deployed the endpoint of the report.";
    for check in [
        "`send-1`",
        "`issue`",
        "`send-2`",
        "`repeat-1`",
        "`repeat-2`",
    ] {
        waits("## report-send.sh", &format!("| {check}"), endpoint);
    }
    waits(
        "## fleet.sh",
        "| `filters-project`",
        "No result yet: no live window has run `just live-fleet`",
    );
    assert!(
        one_line(plan).contains(
            "The other engines have no result yet: the live window ran the plan on AWS alone."
        ),
        "the section of the plan does not say why the other engines have no result"
    );
}

/// The default filter of the GCP and Azure instance type routes, and the annotation `freeCredit`
/// of an AWS account, come from the code of Palette. No run shows them, so `limits.md` states them
/// and `validate-routes.md` links there. A fact of `limits.yaml` holds each one.
#[test]
fn the_rules_of_the_instance_type_routes_that_only_the_code_of_palette_gives_are_in_limits() {
    let limits = one_line(&read(&docs().join("internals/palette/limits.md")));
    for needle in [
        "**The default filter of the GCP and Azure instance type routes.** They list only the types with 2 vCPUs or more and 4 GiB of memory or more, when the query does not say. Strata sends `cpuGtEq=0&memoryGtEq=0` to each route.",
        "**An AWS account with the annotation `freeCredit` set to `true`.** The AWS instance type route lists only the types with up to 4 vCPUs and 8 GiB of memory for such an account.",
    ] {
        assert!(limits.contains(needle), "limits.md lacks {needle:?}");
    }

    let routes = one_line(&read(&docs().join("internals/palette/validate-routes.md")));
    for old in [
        "The code of Palette gives the same default to the GCP and Azure routes",
        "freeCredit",
        "A run shows that `m3.large` is absent.",
        "This comes from the code of Palette, and no run shows it.",
    ] {
        assert!(
            !routes.contains(old),
            "validate-routes.md still says {old:?}"
        );
    }
    for needle in [
        "A call by hand shows that `m3.large` is absent.",
        "No run shows this default on GCP and Azure: see [What the source of Palette says](limits.md#what-the-source-of-palette-says).",
    ] {
        assert!(
            routes.contains(needle),
            "validate-routes.md lacks {needle:?}"
        );
    }

    let facts = repo_file("xtask/facts/limits.yaml");
    for id in [
        "limit-instance-type-default-filter",
        "limit-instance-type-free-credit",
    ] {
        let fact = section(&facts, &format!("  - id: {id}\n"), Some("\n  - id: "));
        assert!(
            fact.contains(
                "unchecked: The page states the rule from the source of Palette, and no run shows it."
            ),
            "{fact}"
        );
    }
}

/// The sentences of the book that the review at the end of Wave 13 found over the limits. A
/// sentence has the limit of its entry, and a block has six sentences at most.
#[test]
fn the_sentences_that_the_review_at_the_end_of_wave_13_named_keep_to_the_limits() {
    let blocks = [
        (
            "quickstart.md",
            "[Palette gives a value that strata does not know]",
            25,
        ),
        (
            "use-cases/index.md",
            "[Palette gives a value that strata does not know]",
            25,
        ),
        ("reference/settings.md", "| `XDG_STATE_HOME` |", 25),
        (
            "commands/preflight/tiers.md",
            "Palette lists some facts of a cloud account",
            25,
        ),
        (
            "internals/preflight.md",
            "When a report names a release",
            25,
        ),
        (
            "internals/preflight.md",
            "A rule that no run proved on the release",
            25,
        ),
        (
            "reference/preflight-rules.md",
            "| `palette` | `method`,",
            25,
        ),
        ("reference/preflight-rules.md", "| `exec` | `argv`,", 25),
        ("development/live-example.md", "| `aws_profile` |", 25),
        (
            "development/facts.md",
            "Strata depends on facts about Palette.",
            25,
        ),
        ("development/facts.md", "| `table` | Only a fact", 25),
        ("development/facts.md", "| `palette` | Only a fact", 25),
        (
            "development/facts.md",
            "Name the live checks that prove the fact.",
            20,
        ),
        (
            "development/facts.md",
            "`just docs-check` and `just docs-examples` run each chapter",
            25,
        ),
        (
            "development/facts.md",
            "The stand-in puts the query in the name",
            25,
        ),
        (
            "development/facts.md",
            "An answer with no value at the field",
            25,
        ),
        (
            "development/facts.md",
            "A fact can name the values of a closed set",
            25,
        ),
        (
            "development/facts.md",
            "Prove the fact again with its live checks.",
            20,
        ),
        ("development/facts.md", "Prove the value again.", 20),
        (
            "reference/report.md",
            "The note of an unknown value can end with",
            25,
        ),
    ];
    for (page, anchor, limit) in blocks {
        let text = read(&docs().join(page));
        let block = block(&text, anchor);
        let found = block_sentences(&block);
        assert!(
            block.starts_with('|') || found.len() <= 6,
            "{page}: {} sentences in the block of {anchor:?}",
            found.len()
        );
        for sentence in found {
            assert!(
                words(&sentence) <= limit,
                "{page}: {} words in a sentence of {limit} at most: {sentence}",
                words(&sentence)
            );
        }
    }
}

/// The paragraphs that the review at the end of Wave 13 found over six sentences.
#[test]
fn the_paragraphs_that_the_review_at_the_end_of_wave_13_named_have_six_sentences_or_fewer() {
    let blocks = [
        (
            "commands/preflight/tiers.md",
            "Some rules need a fact that Palette lists",
        ),
        ("commands/preflight/tiers.md", "The request has the query"),
        (
            "development/facts.md",
            "The stand-in puts the query in the name",
        ),
        (
            "development/facts.md",
            "The run takes the value from a listing",
        ),
        (
            "development/facts.md",
            "A request names a project with the header",
        ),
        (
            "development/facts.md",
            "A route of the tenant is not an operation",
        ),
        ("development/facts.md", "Each account sees its own values"),
        ("development/facts.md", "When the answer lists some values"),
        (
            "internals/palette/clusters.md",
            "**Saved tag filters** belong to the tenant",
        ),
        (
            "internals/palette/clusters.md",
            "The query takes `tag`, `meta`, or `resource`.",
        ),
        (
            "internals/palette/validate-routes.md",
            "A run on AWS also shows the key pair route.",
        ),
        (
            "internals/palette/validate-routes.md",
            "No run shows the GCP answer",
        ),
        (
            "internals/preflight.md",
            "A new Palette release sets the counts",
        ),
        ("internals/preflight.md", "When a report names a release"),
        (
            "reference/preflight-rules.md",
            "The binary carries the ledger of the lab.",
        ),
        (
            "reference/preflight-rules.md",
            "`4.10.3` and `4.10` are one release",
        ),
    ];
    for (page, anchor) in blocks {
        let found = sentences(&block(&read(&docs().join(page)), anchor)).len();
        assert!(
            found <= 6,
            "{page}: {found} sentences in the block of {anchor:?}"
        );
    }
}

/// The phrasal verbs that the review at the end of Wave 13 named. Each file below held one, and a
/// line break can fall inside the phrase. `just --list` prints the comments of the justfile.
#[test]
fn the_phrasal_verbs_of_the_review_at_the_end_of_wave_13_are_gone() {
    for (file, phrase) in [
        ("src/cli/args.rs", "filled in"),
        ("docs/src/commands/report.md", "filled in"),
        ("docs/src/reference/report.md", "filled in"),
        ("docs/src/reference/report.md", "use up"),
        (
            "docs/src/reference/preflight-rules.md",
            "no request goes out",
        ),
        (
            "docs/src/reference/preflight-rules.md",
            "The request goes out",
        ),
        ("docs/src/development/report-endpoint.md", "goes through"),
        ("docs/src/development/report-endpoint.md", "move them out"),
        ("docs/src/development/report-endpoint.md", "logged in"),
        ("docs/src/development/facts.md", "turns the result into"),
        ("docs/src/development/facts.md", "stay off"),
        ("docs/src/development/facts.md", "no one is logged in"),
        ("docs/src/development/facts.md", "write the decision down"),
        (
            "docs/src/development/live-runs.md",
            "logged in to the repository",
        ),
        ("docs/src/development/live-runs.md", "puts the pool back"),
        ("docs/src/development/live-runs.md", "puts them back"),
        ("justfile", "logged in"),
        ("justfile", "puts the pool back"),
    ] {
        assert!(
            !one_line(&repo_file(file)).contains(phrase),
            "{file} still holds {phrase:?}"
        );
    }
}

/// The public rustdoc that the review at the end of Wave 13 named: each sentence has 25 words at
/// most, and none has a word or a phrasal verb that the review refused.
#[test]
fn the_public_rustdoc_that_the_review_at_the_end_of_wave_13_named_keeps_to_25_words() {
    let comments = [
        (
            "preflight/src/probe/backend.rs",
            "The answer of Palette to one request.",
        ),
        (
            "preflight/src/world/mod.rs",
            "True for each 4xx on a read of the world",
        ),
        ("src/closed_set/unknown.rs", "After the first note of a day"),
        (
            "src/closed_set/unknown.rs",
            "A watch with no log shows no hint",
        ),
        ("src/closed_set/unknown/hint.rs", "When and where the hint"),
        (
            "src/closed_set/unknown/log.rs",
            "comes from an answer of status 2xx",
        ),
        ("src/ops/report.rs", "This function only reads."),
        ("src/ops/report.rs", "The address of a new issue of strata."),
        ("src/snapshot/pack.rs", "Build one pack from its ref"),
        ("src/term/ask.rs", "Write `question` to `out`"),
        (
            "src/error/mod.rs",
            "Palette answered with a non-2xx status.",
        ),
    ];
    for (file, anchor) in comments {
        let text = comment(&repo_file(file), anchor);
        for sentence in sentences(&text) {
            assert!(
                words(&sentence) <= 25,
                "{file}: {} words in a comment sentence of 25 at most: {sentence}",
                words(&sentence)
            );
        }
        for phrase in [" would ", " may ", "went out", "filled in", "use up"] {
            assert!(
                !text.contains(phrase),
                "{file}: the comment of {anchor:?} holds {phrase:?}"
            );
        }
    }
}

/// The review found four lines with the word "would" in the diff of #553. The word is gone from
/// each one.
#[test]
fn the_four_lines_with_would_that_the_review_of_553_named_are_gone() {
    for (file, anchor) in [
        (
            "docs/src/reference/compatibility.md",
            "The loader refuses a code in `refusal_codes`",
        ),
        (
            "preflight/src/probe/backend.rs",
            "An entry of `_responses` with a status that is not 2xx",
        ),
        (
            "preflight/src/rule/tests.rs",
            "a 5xx with another code is sent again",
        ),
        (
            "preflight/tests/catalog.rs",
            "A refusal at a 5xx is read one time.",
        ),
    ] {
        let text = repo_file(file);
        let at = text
            .find(anchor)
            .unwrap_or_else(|| panic!("{file} lacks {anchor:?}"));
        let near = &text[at..(at + 700).min(text.len())];
        assert!(!near.contains(" would "), "{file}: {near}");
    }
}

/// The everyday loop shows the report in its two tables, its seed, and its live run, and it says
/// that a yes to the question also starts the send.
#[test]
fn the_everyday_loop_shows_the_report_in_its_tables_its_seed_and_its_live_run() {
    let text = the_loop();
    assert!(
        one_line(&text).contains(
            "unless a person chooses the send, with `STRATA_REPORT=1` or a yes to its one question."
        ),
        "loop.md does not say how a person chooses the send"
    );
    assert!(
        !text.contains("unless a person sets `STRATA_REPORT=1`"),
        "loop.md still says that only the setting starts the send"
    );

    let day = section(&text, "## A day of work", Some("\n## "));
    assert!(
        table_rows(day).iter().any(|row| row[0]
            == "A job writes a note about a value that strata does not know"
            && row[1].contains("Strata sends nothing unless you choose the send.")),
        "the day of work has no row for the note"
    );
    let jobs = section(&text, "## The jobs and their commands", Some("\n## "));
    let row = table_rows(jobs)
        .into_iter()
        .find(|row| row[0] == "A note of an unknown value")
        .expect("the table of the jobs has no row for the note");
    assert!(
        row[3].contains("`report --link`") && row[3].contains("`STRATA_REPORT=1`"),
        "{row:?}"
    );
    assert!(row[4].contains("(./use-cases/unknown-value.md)"), "{row:?}");

    let unknown = section(&text, "## A value that strata does not know", Some("\n## "));
    assert!(
        unknown.contains(
            "**Follow along.** `just playground unknown-value` serves a tenant with such a value."
        ),
        "the section of the note has no seed"
    );
    assert!(
        one_line(&text).contains("a read of the log of the unknown values"),
        "the live run of the loop does not read the log"
    );
}

/// `loop.sh` reads the log of the unknown values with `strata report`. The log is in the run
/// directory, and the book has a row for the check with the item #396.
#[test]
fn loop_sh_has_the_check_report_log_and_the_book_has_its_row() {
    let script = repo_file("examples/live/loop.sh");
    let state = script
        .find("export XDG_STATE_HOME=$PWD/state")
        .expect("loop.sh sets XDG_STATE_HOME to a folder of the run");
    let first = script.find("  check ").expect("loop.sh has a check");
    assert!(state < first, "the folder is set after the first check");
    assert!(
        script.contains("check report-log 0 \"#396: strata report reads the log of the unknown values that this run wrote\" -- \"$strata\" report"),
        "loop.sh has no check report-log"
    );

    let page = read(&docs().join("development/live-runs.md"));
    let part = section(&page, "## loop.sh", Some("\n## "));
    let row = part
        .lines()
        .find(|l| l.starts_with("| `report-log` |"))
        .expect("the section of loop.sh has no row for report-log");
    assert!(
        row.contains("| 0 | #396:") && row.contains("No result yet:"),
        "{row}"
    );
}
