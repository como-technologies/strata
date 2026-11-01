//! The book's own consistency: every use case is listed everywhere it belongs, in one order.
//!
//! `docs/src/use-cases/index.md` lists every page in the order that teaches the tool, and
//! `docs/src/SUMMARY.md` is the navigation beside it. A page added to one and forgotten in the
//! other is invisible to a reader who came the other way. That happened to "What changed between
//! two pack versions", which reached the navigation and never reached the list.

use crate::common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::{INSTALLING_WORKFLOWS, repo_file, schema, script, version_of, walk};
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

/// The section of release 0.25.0 in the compatibility page, with each run of white space as one
/// space.
fn release_0_25_0(page: &str) -> String {
    one_line(section(
        page,
        "## Release 0.25.0: changes that need action",
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

    let release = release_0_25_0(&read(&docs().join("reference/compatibility.md")));
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
    let release = release_0_25_0(&page);
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

/// The block of `text` that holds the first line with `anchor`. It is a paragraph, a list item
/// without its nested items, or a table row. The block is one line, without the mark of the list.
fn block(text: &str, anchor: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.contains(anchor))
        .unwrap_or_else(|| panic!("no line holds {anchor:?}"));
    if lines[at].starts_with('|') {
        return one_line(lines[at]);
    }
    let starts_item = |l: &str| l.trim_start().starts_with("- ");
    let mut from = at;
    while from > 0 && !starts_item(lines[from]) && !lines[from - 1].trim().is_empty() {
        from -= 1;
    }
    let mut to = at + 1;
    while to < lines.len() && !lines[to].trim().is_empty() && !starts_item(lines[to]) {
        to += 1;
    }
    one_line(&lines[from..to].join(" "))
        .trim_start_matches("- ")
        .to_string()
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
        The name and the version alone do not fix the content. Palette can update a \
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
    assert_eq!(rows.len(), 9, "the table of the day has nine steps");
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
    assert_eq!(table_rows(part).len(), 11, "the table has eleven jobs");

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
