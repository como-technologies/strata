//! The book's own consistency: every use case is listed everywhere it belongs, in one order.
//!
//! `docs/src/use-cases/index.md` lists every page in the order that teaches the tool, and
//! `docs/src/SUMMARY.md` is the navigation beside it. A page added to one and forgotten in the
//! other is invisible to a reader who came the other way. That happened to "What changed between
//! two pack versions", which reached the navigation and never reached the list.

use crate::common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::{
    GITLAB_DROP_IN, INSTALLING_WORKFLOWS, gitlab_drop_in, one_line, recipe, repo_file, schema,
    script, version_of, walk,
};
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
/// repository. A badge that asks it for the version renders "repo not found". Static means it can
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

/// The changes of Wave 11 were in a few paragraphs. The page had no section that tells a
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
/// failed at that step. The page had no word of it. The section names each workflow, the
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
/// only writes has no older place to read. So the page names each one, with the id rule that
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
    let drop_in = gitlab_drop_in();
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

/// The most words in a sentence of a description. "The style rules" of `documentation.md` give it.
const MOST_WORDS: usize = 25;

/// The most words in a sentence of a procedure. "The style rules" of `documentation.md` give it.
const MOST_WORDS_IN_A_PROCEDURE: usize = 20;

/// The most sentences in a paragraph, a list item, or a table cell.
const MOST_SENTENCES: usize = 6;

/// The faults of the sentences `found`, which `place` holds: one line for each sentence of more
/// than `limit` words.
fn word_faults(place: &str, found: &[String], limit: usize) -> Vec<String> {
    found
        .iter()
        .filter(|s| words(s) > limit)
        .map(|s| {
            let count = words(s);
            format!(
                "{place}: {count} words in a sentence of {limit} at most: {}",
                s.trim()
            )
        })
        .collect()
}

/// The faults of the block `block`, which `place` holds. A sentence of more than `MOST_WORDS` words
/// is a fault. A block that is no table row and has more than `MOST_SENTENCES` sentences is one too.
fn block_faults(place: &str, block: &str) -> Vec<String> {
    let found = block_sentences(block);
    let mut faults = word_faults(place, &found, MOST_WORDS);
    if !block.starts_with('|') && found.len() > MOST_SENTENCES {
        faults.push(format!("{place}: {} sentences: {block}", found.len()));
    }
    faults
}

/// The faults of the text `text`, which `place` holds: a sentence of more than `MOST_WORDS` words.
fn sentence_faults(place: &str, text: &str) -> Vec<String> {
    word_faults(place, &sentences(text), MOST_WORDS)
}

/// Stop the test when `faults` has an entry. The message has each fault on a line of its own.
fn assert_no_faults(faults: &[String]) {
    assert!(
        faults.is_empty(),
        "{} faults:\n{}",
        faults.len(),
        faults.join("\n")
    );
}

/// Stop the test when a comment that `comments` names has a sentence of more than `MOST_WORDS`
/// words. Each entry is a file and the start of a comment in it.
fn assert_comments_keep_to_the_limit(comments: &[(&str, &str)]) {
    let faults: Vec<String> = comments
        .iter()
        .flat_map(|(file, anchor)| sentence_faults(file, &comment(file, anchor)))
        .collect();
    assert_no_faults(&faults);
}

#[test]
fn the_faults_of_a_block_name_a_sentence_over_the_limit_and_a_block_over_the_most_sentences() {
    let at_limit = format!("{}.", vec!["word"; MOST_WORDS].join(" "));
    let over = format!("{}.", vec!["word"; MOST_WORDS + 1].join(" "));
    assert!(block_faults("page.md", &at_limit).is_empty());
    let faults = block_faults("page.md", &over);
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert!(
        faults[0].starts_with(&format!(
            "page.md: {} words in a sentence of {MOST_WORDS} at most: ",
            MOST_WORDS + 1
        )),
        "{faults:?}"
    );

    let many = ["A short one."; MOST_SENTENCES + 1].join(" ");
    assert_eq!(block_faults("page.md", &many).len(), 1);
    assert!(
        block_faults("page.md", &format!("| {many} |")).is_empty(),
        "a table row has no limit of sentences"
    );

    let procedure = word_faults("page.md", &sentences(&at_limit), MOST_WORDS_IN_A_PROCEDURE);
    assert_eq!(procedure.len(), 1, "a procedure has a shorter limit");
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
            MOST_WORDS,
        ),
        (
            "automation/preflight.md",
            "markdown to the job summary",
            MOST_WORDS,
        ),
        (
            "automation/preflight.md",
            "reads one variable for each input",
            MOST_WORDS,
        ),
        (
            "development/build-test.md",
            "names it. For each option",
            MOST_WORDS,
        ),
        (
            "reference/documents.md",
            "`actorType` and `action`, as Palette",
            MOST_WORDS,
        ),
        (
            "automation/gitlab.md",
            "downloads one release. When `cosign` 3 or later is on the",
            MOST_WORDS,
        ),
        (
            "automation/gitlab.md",
            "To require the check, install",
            MOST_WORDS_IN_A_PROCEDURE,
        ),
        (
            "automation/pr-comments.md",
            "The workflow installs the release binary",
            MOST_WORDS,
        ),
        (
            "development/live-runs.md",
            "makes a checkout of the caller",
            MOST_WORDS,
        ),
        (
            "development/live-runs.md",
            "#419: with `CLUSTER`, the record of the cluster",
            MOST_WORDS,
        ),
        (
            "development/live-runs.md",
            "#158: `verify-report.sh --who` puts",
            MOST_WORDS,
        ),
        (
            "automation/library.md",
            "`with_palette` adds the profiles",
            MOST_WORDS,
        ),
        (
            "use-cases/edited-in-place.md",
            "Then the issue also lists each write",
            MOST_WORDS_IN_A_PROCEDURE,
        ),
        (
            "use-cases/no-digest.md",
            "Then give `--allow-no-digest`",
            MOST_WORDS_IN_A_PROCEDURE,
        ),
        (
            "use-cases/no-digest.md",
            "Give `--allow no-digest` to",
            MOST_WORDS_IN_A_PROCEDURE,
        ),
    ];
    for (page, anchor, limit) in sentences_to_check {
        let text = read(&docs().join(page));
        let found = sentences(&block(&text, anchor));
        let sentence = found
            .iter()
            .find(|s| s.contains(anchor.split(". ").last().unwrap()))
            .unwrap_or_else(|| panic!("{page}: no sentence holds {anchor:?}"));
        assert_no_faults(&word_faults(page, std::slice::from_ref(sentence), limit));
    }

    // the cells of the table of the drop-ins: one short sentence for each trigger
    let table = repo_file("examples/ci/README.md");
    for row in ["| `github/strata.yml` |", "| `gitlab/strata.yml` |"] {
        assert_no_faults(&sentence_faults(
            "examples/ci/README.md",
            &block(&table, row),
        ));
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
            found <= MOST_SENTENCES,
            "{page}: {found} sentences in the block of {anchor:?}"
        );
    }
}

/// The comment blocks of a file. A block is a run of lines that start with `mark`. A line that is
/// not one, a blank comment line, a list item, a literal line, or a fence ends the run. A block is
/// one line, without the marks. `///` and `//!` are `//` with one more mark.
///
/// A literal line has two spaces or more after the mark, as a table or a usage text has. It is a
/// block of its own. A fenced block of code in a comment is not prose, so no block holds it. A
/// shebang is not a comment.
fn comment_blocks(text: &str, mark: &str) -> Vec<String> {
    fn end_block(blocks: &mut Vec<String>, run: &mut Vec<&str>) {
        if !run.is_empty() {
            blocks.push(std::mem::take(run).join(" "));
        }
    }
    fn is_list_item(line: &str) -> bool {
        let digits = line.trim_start_matches(|c: char| c.is_ascii_digit());
        line.starts_with("- ")
            || line.starts_with("* ")
            || (digits.len() < line.len() && (digits.starts_with(". ") || digits.starts_with(") ")))
    }
    let mut blocks = Vec::new();
    let mut run: Vec<&str> = Vec::new();
    let mut fenced = false;
    for (at, line) in text.lines().enumerate() {
        let line = line.trim_start();
        let rest = line
            .strip_prefix(mark)
            .filter(|_| !(at == 0 && line.starts_with("#!")));
        let Some(rest) = rest else {
            end_block(&mut blocks, &mut run);
            fenced = false;
            continue;
        };
        let rest = rest.trim_start_matches(['/', '!']);
        let words = rest.trim();
        if words.starts_with("```") {
            end_block(&mut blocks, &mut run);
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if words.is_empty() {
            end_block(&mut blocks, &mut run);
            continue;
        }
        let literal = rest.starts_with("  ");
        if literal || is_list_item(words) {
            end_block(&mut blocks, &mut run);
        }
        run.push(words);
        if literal {
            end_block(&mut blocks, &mut run);
        }
    }
    end_block(&mut blocks, &mut run);
    blocks
}

/// The comment blocks of a file of the repository: `//` in a Rust file, and `#` in any other.
fn comments_of(file: &str) -> Vec<String> {
    let mark = if file.ends_with(".rs") { "//" } else { "#" };
    comment_blocks(&repo_file(file), mark)
}

/// The comment of a file of the repository that holds `anchor`.
fn comment(file: &str, anchor: &str) -> String {
    comments_of(file)
        .into_iter()
        .find(|block| block.contains(anchor))
        .unwrap_or_else(|| panic!("{file}: no comment holds {anchor:?}"))
}

/// The style review of Wave 11 found comments in the Rust files with a sentence over the limit of
/// 25 words for a description.
#[test]
fn the_rust_comments_that_the_style_review_of_wave_11_named_keep_to_25_words() {
    let comments = [
        (
            "src/snapshot/cluster.rs",
            "The two fields of a record of strata 0.24 and before",
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
            "The comment of a file of the repository that holds",
        ),
    ];
    assert_comments_keep_to_the_limit(&comments);
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
            faults.extend(block_faults(page, &block));
        }
    }
    assert_no_faults(&faults);
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
    assert_comments_keep_to_the_limit(&comments);
}

/// The legend of a pair status is plain words for a reader without the book. Each sentence has 25
/// words at most. No sentence is a fragment after a semicolon.
#[test]
fn the_legend_of_a_pair_status_keeps_to_25_words_and_has_no_semicolon() {
    let legend = strata::closed_set::legend::legend::<strata::ops::PairStatus>();
    assert!(!legend.is_empty());
    for (status, text) in legend {
        assert!(!text.contains(';'), "{status}: {text}");
        assert_no_faults(&sentence_faults(&status, &text));
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
        "report-endpoint",
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

/// A rule that waits for a call of the route says why (#409). A rule that a live run showed the
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
/// heading that a page does not have. So this test needs only the headings and the two new links.
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
            MOST_WORDS,
        ),
        (
            "commands/preflight/watch.md",
            "`--watch cluster:UID` reads",
            MOST_WORDS,
        ),
        (
            "commands/preflight/watch.md",
            "`--verbose` prints",
            MOST_WORDS,
        ),
        (
            "commands/preflight/watch.md",
            "To see a watch on a real build",
            MOST_WORDS,
        ),
        (
            "commands/preflight/tiers.md",
            "A run sends each request one time",
            MOST_WORDS,
        ),
        (
            "commands/preflight/tiers.md",
            "So `palette-validate`",
            MOST_WORDS,
        ),
        ("internals/hashes.md", "The JSON is also YAML", MOST_WORDS),
        ("internals/hashes.md", "These are DEL", MOST_WORDS),
        (
            "development/documentation.md",
            "Find the chapter of the page",
            MOST_WORDS_IN_A_PROCEDURE,
        ),
        (
            "reference/refs.md",
            "`snapshot --raw-dir` takes a profile or a cluster",
            MOST_WORDS,
        ),
    ];
    for (page, anchor, limit) in blocks {
        let found = sentences(&block(&read(&docs().join(page)), anchor));
        assert!(
            found.len() <= MOST_SENTENCES,
            "{page}: {} sentences in the block of {anchor:?}",
            found.len()
        );
        assert_no_faults(&word_faults(page, &found, limit));
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
    assert_comments_keep_to_the_limit(&comments);
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

/// The packages of the rustdoc builds of `just lint`, in the order of the recipe. They are the
/// list that a sentence gives: "`a`, `b`, and `c`".
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

/// The book states what the code does. It states the rustdoc of `just lint`, the property of the
/// version order, and the save of the group values of a cluster.
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

/// The public items show their comments to each user of the library. So each sentence of the
/// comments that the review at the end of Wave 12 named has 25 words at most. None has a
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
        let text = comment(file, anchor);
        assert_no_faults(&sentence_faults(file, &text));
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
/// did not state. Two are the directory of a cluster group and the root of a canonical text. Two
/// are the verdict of the merge gate and the artifact of the GitLab job. Each one has its action in
/// the section.
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
    let yml = repo_file(GITLAB_DROP_IN);
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
/// shows it live. They are the page of the command at "A pre-release", and step 4 of the use case.
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
/// with a comment that says what it holds. The book states where the copy is and how old it can
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

/// The pages that describe `strata report`. They include the page of the endpoint that takes the
/// report. The page of the facts holds the contract report.
const REPORT_PAGES: [&str; 5] = [
    "reference/report.md",
    "commands/report.md",
    "use-cases/unknown-value.md",
    "development/report-endpoint.md",
    "development/facts.md",
];

/// True when `text` holds `phrase` as whole words. A letter or a digit before or after the phrase
/// makes it part of another word, such as "input back" for "put back".
fn has_phrase(text: &str, phrase: &str) -> bool {
    let text = one_line(text);
    text.match_indices(phrase).any(|(at, found)| {
        let before = text[..at].chars().next_back();
        let after = text[at + found.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// `text` without the code spans: the text between two backticks.
fn without_code(text: &str) -> String {
    text.split('`')
        .enumerate()
        .filter(|(n, _)| n % 2 == 0)
        .map(|(_, part)| part)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The page of the report states the address that the release names, so a person can read where
/// a send goes. The page and the constant agree.
#[test]
fn the_page_of_the_report_states_the_address_of_the_endpoint_that_the_release_names() {
    let address = strata::closed_set::unknown::send::ENDPOINT
        .expect("the release names the endpoint of the report");
    let page = read(&docs().join("reference/report.md"));
    assert!(
        page.contains(&format!("`{address}`")),
        "reference/report.md does not state {address}"
    );
}

/// The words that "The style rules" refuse: the modal verbs, "ensure", the future "will", and the
/// contractions.
const REFUSED_WORDS: [&str; 14] = [
    "may", "might", "should", "would", "will", "ensure", "ensures", "isn't", "doesn't", "can't",
    "won't", "don't", "it's", "that's",
];

/// The phrasal verbs that the reviews named, in the forms that a sentence uses. Each one has a
/// plain verb: "omit" for "leave out", "send" for "go out", "continue" for "go on".
const PHRASAL_VERBS: [&str; 57] = [
    "set up",
    "look up",
    "carry out",
    "leave out",
    "leaves out",
    "left out",
    "turn on",
    "turn off",
    "turns on",
    "turns off",
    "turned on",
    "turned off",
    "go on",
    "goes on",
    "went on",
    "go out",
    "goes out",
    "went out",
    "go down",
    "goes down",
    "go up",
    "goes up",
    "stays out",
    "stay out",
    "read back",
    "filled in",
    "use up",
    "logged in",
    "goes through",
    "stay off",
    "put back",
    "write down",
    "writes down",
    "wrote down",
    "written down",
    "write out",
    "writes out",
    "written out",
    "hold back",
    "holds back",
    "held back",
    "stand in for",
    "stands in for",
    "stood in for",
    "pass over",
    "passes over",
    "give back",
    "gives back",
    "gave back",
    "find out",
    "finds out",
    "found out",
    "carries out",
    "carried out",
    "looks up",
    "looked up",
    "sets up",
];

/// The idioms that the reviews named. "The style rules" refuse an idiom and a figure of speech.
const IDIOMS: [&str; 2] = ["went stale", "by design"];

/// The faults of the prose of `block`, which `place` holds: a word that "The style rules" refuse, a
/// phrasal verb, or an idiom. A code span is no prose.
fn wording_faults(place: &str, block: &str) -> Vec<String> {
    let prose = without_code(block).to_lowercase();
    let mut faults = Vec::new();
    for word in prose.split(|c: char| !(c.is_alphanumeric() || c == '\'')) {
        if REFUSED_WORDS.contains(&word) {
            faults.push(format!("{place}: the word {word:?} in: {block}"));
        }
    }
    for phrase in PHRASAL_VERBS.iter().chain(&IDIOMS) {
        if has_phrase(&prose, phrase) {
            faults.push(format!("{place}: the phrase {phrase:?} in: {block}"));
        }
    }
    faults
}

/// A sentence has 25 words at most, and a paragraph, a list item, or a table row has six
/// sentences at most. The pages use none of the words that "The style rules" refuse, no phrasal
/// verb, and no idiom that the reviews named.
#[test]
fn the_pages_of_the_report_keep_to_the_style_rules() {
    let mut faults = Vec::new();
    for page in REPORT_PAGES {
        let text = read(&docs().join(page));
        for block in blocks_of(&text) {
            faults.extend(block_faults(page, &block));
            faults.extend(wording_faults(page, &block));
        }
    }
    assert_no_faults(&faults);
}

/// Step 6 of the deploy states the three points of Cloud Run as results. A live run gave two of
/// them. The lab machine has no IPv6 route. So a failed `curl -6` shows a fault of the route of the
/// machine, and the page says so.
#[test]
fn step_6_of_the_deploy_states_the_three_results_of_cloud_run() {
    let page = one_line(&read(&docs().join("development/report-endpoint.md")));
    let from = page
        .find("6. Know three points of Cloud Run")
        .expect("step 6");
    let step = &page[from..from + page[from..].find("7. Name the address").expect("step 7")];
    for needle in [
        "**The last entry of `X-Forwarded-For` is the address of the client.** Valid reports with another forged first entry each count as one sender.",
        "The service refuses them at the limit with 429. A control with no forged header also gets 429.",
        "A bad body spends no limit, so the check needs valid reports.",
        "**IPv6.** The service publishes IPv6 addresses, so a sender can reach it on IPv6. The rule of the /64 network has an effect.",
        "The lab machine has no IPv6 route, and `curl -6` exits 7. So no live request shows the rule, and the tests of `network_of` show it.",
        "A failed `curl -6` shows a fault of the route of the machine, not a fault of the service.",
        "**A slow body.** 12 connections, above the 8 requests that the service takes at a time, each sent a body one byte in a second. A quick request beside them took 0.20 s.",
        "So a slow body holds up nothing that a sender sees. No run shows where the front end buffers.",
    ] {
        assert!(step.contains(needle), "step 6 lacks {needle:?}: {step}");
    }
    for old in [
        "If the connection fails",
        "has no effect",
        "Cloud Run buffers a slow body",
    ] {
        assert!(!step.contains(old), "step 6 still says {old:?}");
    }
    // the page names the 8 requests at a time that the deploy sets
    assert!(repo_file("deploy/report/deploy.sh").contains("--concurrency 8"));
}

/// `strata report` adds two changes that break code outside the crate. One is a required method of
/// the public trait `Renderer`. The other is a variant of `Error`, which is not
/// `#[non_exhaustive]`. The section of release 0.27.0 sits above the one of release 0.26.0. The
/// opening of the page links to all four sections, and the section gives the action for each
/// change.
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
        one_line(opening).contains("Five sections list the changes that need action"),
        "the opening does not name five sections"
    );
    for anchor in [
        "(#release-0280-changes-that-need-action)",
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

/// The section lacked six changes of the library `strata-preflight` that do not build. The text of
/// the 5xx said that each 5xx other than a named code is an error.
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
/// #515, #554, and #567 have their results. So have the rows of the send of the report, the
/// listing of the filters, and the log of the unknown values. The rows of the other engines of the
/// plan say what they wait for.
#[test]
fn the_rows_that_the_review_named_have_their_results_or_say_what_they_wait_for() {
    let page = read(&docs().join("development/live-runs.md"));
    let none = |row: &str| !row.to_lowercase().contains("no result yet");
    let contract = section(&page, "## contract.sh", Some("\n## "));
    for item in [
        "#390", "#393", "#530", "#539", "#540", "#543", "#554", "#567", "#576",
    ] {
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

    let has_result = |section_name: &str, anchor: &str| {
        let part = section(&page, section_name, Some("\n## "));
        let rows: Vec<&str> = part.lines().filter(|l| l.contains(anchor)).collect();
        assert!(!rows.is_empty(), "{section_name} has no row for {anchor}");
        for row in rows {
            assert!(none(row), "{row}");
        }
    };
    for check in ["`send-1`", "`send-2`", "`repeat-1`", "`repeat-2`"] {
        has_result("## report-send.sh", &format!("| {check}"));
    }
    // #574 gave the endpoint a label of its own, and a live run found the issue under it
    let send = section(&page, "## report-send.sh", Some("\n## "));
    let issue = send
        .lines()
        .find(|l| l.starts_with("| `issue`"))
        .expect("report-send.sh has no row for issue");
    assert!(none(issue), "{issue}");
    assert!(issue.contains("`unknown-value`"), "{issue}");
    has_result("## fleet.sh", "| `filters-project`");
    has_result("## loop.sh", "| `report-log`");
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
            MOST_WORDS,
        ),
        (
            "use-cases/index.md",
            "[Palette gives a value that strata does not know]",
            MOST_WORDS,
        ),
        ("reference/settings.md", "| `XDG_STATE_HOME` |", MOST_WORDS),
        (
            "commands/preflight/tiers.md",
            "Palette lists some facts of a cloud account",
            MOST_WORDS,
        ),
        (
            "internals/preflight.md",
            "When a report names a release",
            MOST_WORDS,
        ),
        (
            "internals/preflight.md",
            "A rule that no run proved on the release",
            MOST_WORDS,
        ),
        (
            "reference/preflight-rules.md",
            "| `palette` | `method`,",
            MOST_WORDS,
        ),
        (
            "reference/preflight-rules.md",
            "| `exec` | `argv`,",
            MOST_WORDS,
        ),
        (
            "development/live-example.md",
            "| `aws_profile` |",
            MOST_WORDS,
        ),
        (
            "development/facts.md",
            "Strata depends on facts about Palette.",
            MOST_WORDS,
        ),
        (
            "development/facts.md",
            "| `table` | Only a fact",
            MOST_WORDS,
        ),
        (
            "development/facts.md",
            "| `palette` | Only a fact",
            MOST_WORDS,
        ),
        (
            "development/facts.md",
            "Name the live checks that prove the fact.",
            MOST_WORDS_IN_A_PROCEDURE,
        ),
        (
            "development/facts.md",
            "`just docs-check` and `just docs-examples` run each chapter",
            MOST_WORDS,
        ),
        (
            "development/facts.md",
            "The stand-in puts the query in the name",
            MOST_WORDS,
        ),
        (
            "development/facts.md",
            "An answer with no value at the field",
            MOST_WORDS,
        ),
        (
            "development/facts.md",
            "A fact can name the values of a closed set",
            MOST_WORDS,
        ),
        (
            "development/facts.md",
            "Prove the fact again with its live checks.",
            MOST_WORDS_IN_A_PROCEDURE,
        ),
        (
            "development/facts.md",
            "Prove the value again.",
            MOST_WORDS_IN_A_PROCEDURE,
        ),
        (
            "reference/report.md",
            "The note of an unknown value can end with",
            MOST_WORDS,
        ),
    ];
    for (page, anchor, limit) in blocks {
        let text = read(&docs().join(page));
        let block = block(&text, anchor);
        let found = block_sentences(&block);
        assert!(
            block.starts_with('|') || found.len() <= MOST_SENTENCES,
            "{page}: {} sentences in the block of {anchor:?}",
            found.len()
        );
        assert_no_faults(&word_faults(page, &found, limit));
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
            found <= MOST_SENTENCES,
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

/// The public rustdoc that the review at the end of Wave 13 named. Each sentence has 25 words at
/// most. None has a word or a phrasal verb that the review refused.
#[test]
fn the_public_rustdoc_that_the_review_at_the_end_of_wave_13_named_keeps_to_25_words() {
    let comments = [
        (
            "preflight/src/probe/backend.rs",
            "The answer of Palette to one request.",
        ),
        (
            "preflight/src/probe/backend.rs",
            "True when the 4xx `status` is the answer of a route",
        ),
        (
            "preflight/src/probe/backend.rs",
            "A 401 is the key and a 429 is the rate for each reader.",
        ),
        ("src/closed_set/unknown.rs", "After the first note of a day"),
        (
            "src/closed_set/unknown.rs",
            "A watch with no log shows no hint",
        ),
        ("src/closed_set/unknown/hint.rs", "When and where the hint"),
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
        let text = comment(file, anchor);
        assert_no_faults(&sentence_faults(file, &text));
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
            "the library sends a 5xx with another code again",
        ),
        (
            "preflight/tests/catalog.rs",
            "The check reads a refusal at a 5xx one time.",
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

/// The everyday loop shows the report in its two tables, its seed, and its live run. It says that
/// a yes to the question also starts the send.
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
        row.contains("| 0 | #396:") && !row.to_lowercase().contains("no result yet"),
        "{row}"
    );
}

// ---- the cleanup of Wave 12, in Wave 14 ---------------------------------------------------------

/// One part of the section of release 0.28.0 in compatibility.md, as one line. The part starts at
/// the heading `heading` and ends at the next heading of its level.
fn release_0_28_0_part(heading: &str) -> String {
    let page = read(&docs().join("reference/compatibility.md"));
    let release = section(
        &page,
        "## Release 0.28.0: changes that need action",
        Some("\n## "),
    );
    one_line(section(release, heading, Some("\n### ")))
}

/// The section of release 0.28.0 sits above the one of release 0.27.0. It gives the action for
/// each change of the cleanup that can break a script, a document reader, or a program that uses a
/// library. The changes are the mark `latest`, the path of `snapshot-schema`, and the field
/// `cluster` of `stale-record`. They also include the hash of the values of a group, and the items
/// of the two libraries.
#[test]
fn the_section_of_release_0_28_0_gives_the_action_for_each_change_of_the_cleanup() {
    let page = read(&docs().join("reference/compatibility.md"));
    let newer = page
        .find("\n## Release 0.28.0: changes that need action\n")
        .expect("the page has no section for release 0.28.0");
    let older = page
        .find("\n## Release 0.27.0: changes that need action\n")
        .expect("the page has no section for release 0.27.0");
    assert!(newer < older, "release 0.28.0 comes above release 0.27.0");

    let release = section(
        &page,
        "## Release 0.28.0: changes that need action",
        Some("\n## "),
    );
    let part = |heading: &str| one_line(section(release, heading, Some("\n### ")));
    let command_line = part("### For a user of the command line or of a document\n");
    for needle in [
        "**`strata packs`: `latest` marks the version that `<name>@latest` records.**",
        "`latest` marks `1.29.0` now.",
        "**The message of `snapshot-schema` names the field inside the cluster of a record.**",
        "`source.cluster.profiles[0].version`",
        "The error document has the new key `field`",
        "**`stale-record`: `cluster` holds the uid of the cluster.**",
        "A script that read the name from `error.cluster` must read `error.name`.",
        "**The policy `valuesHash` of a group with a password in values of no canonical form changes.**",
        "A record of 0.27.0 or before of a group with a password differs in the `policy` tier from a new record of the same group.",
        "**`strata-preflight`: seven rules read a version as `strata` does.**",
    ] {
        assert!(
            command_line.contains(needle),
            "the part of the command line lacks {needle:?}"
        );
    }
    let strata = part("### For a user of the library of strata\n");
    for needle in [
        "`snapshot::cluster_tiers` gives a `Result`.",
        "`error::Error::StaleRecord` has the new field `name: String`.",
        "`ops::PackBuild::latest` is true for the version that `pack:<name>@latest` records, of the versions that the listing keeps.",
    ] {
        assert!(strata.contains(needle), "the library part lacks {needle:?}");
    }
    let preflight = part("### For a user of the library of strata-preflight\n");
    for needle in [
        "`world::Engine`, `world::ENGINES`, and `run::RULES_SCHEMA` are not public.",
        "`semver_numbers`",
    ] {
        assert!(
            preflight.contains(needle),
            "the strata-preflight part lacks {needle:?}"
        );
    }
    // the additions are in the first section of the page
    let library = one_line(section(
        &page,
        "## The Rust library",
        Some("\n## The documents"),
    ));
    for needle in [
        "After 0.27.0, `timestamp` is a public module of `strata`.",
        "After 0.27.0, `render::Format` has `ALL`, `as_str`, and `from_text`",
    ] {
        assert!(
            library.contains(needle),
            "the library part lacks {needle:?}"
        );
    }
}

/// Review of Wave 14, D4, C2, and A7. `latest` of `strata packs` is the version that
/// `pack:<name>@latest` records, of the versions that the listing keeps. The listing and the ref
/// pick from the versions that the registry serves, so only `--cloud` gives two versions. The
/// mark of Palette is on the highest version, and strata marks the same one unless it is a
/// pre-release.
#[test]
fn the_book_states_the_cases_where_latest_is_not_the_version_that_the_ref_records() {
    let command_line = release_0_28_0_part("### For a user of the command line or of a document\n");
    for needle in [
        "The two differ when the highest version is a pre-release and the registry has a version that is not a pre-release.",
        "`latest` marks one of the versions that the listing keeps.",
        "With `--cloud`, the listing keeps the versions that support that cloud.",
        "`strata packs <name> --cloud gcp` marks `1.0.0`, and `pack:<name>@latest` records `2.0.0`.",
        "**`pack:<name>@latest` picks from the versions that the registry serves.**",
        "If `1.30.0` has no tag and `1.29.0` has one, the ref records `1.29.0`. Release 0.27.0 recorded `1.30.0`.",
        "An exact version, such as `pack:<name>@1.30.0`, reads the whole listing as before.",
    ] {
        assert!(
            command_line.contains(needle),
            "the section of release 0.28.0 lacks {needle:?}"
        );
    }
    assert!(
        !command_line.contains("the registry has another version"),
        "two pre-releases are another version, and the marks are on one version"
    );
    let library = release_0_28_0_part("### For a user of the library of strata\n");
    assert!(
        library.contains("`ops::PackBuild::latest` is true for the version that `pack:<name>@latest` records, of the versions that the listing keeps."),
        "the library item lacks the versions that the listing keeps"
    );

    let packs = one_line(&read(&docs().join("commands/packs.md")));
    for needle in [
        "the highest version that is not a pre-release, of the versions that the listing keeps.",
        "A registry that has no version that is not a pre-release gives its highest pre-release.",
        "With `--cloud`, the listing keeps the versions of that cloud.",
        "If `aws` has `2.0.0` and `gcp` has `1.0.0`, `--cloud gcp` marks `1.0.0`, and the ref records `2.0.0`.",
        "The listing and the ref pick from one list: the versions that the registry serves.",
        "If `1.30.0` has no tag and `1.29.0` has one, the mark and the ref give `1.29.0`.",
        "So the highest version is the version that Palette marks. Strata marks the same version, unless that version is a pre-release:",
    ] {
        assert!(packs.contains(needle), "packs.md lacks {needle:?}");
    }
    assert!(
        !packs.contains("So the version that strata marks is the version that Palette marks"),
        "packs.md says that the two marks are one version"
    );
    let palette = one_line(&read(&docs().join("internals/palette/packs.md")));
    assert!(
        palette
            .contains("`@latest` is the highest version of that copy that is not a pre-release."),
        "internals/palette/packs.md lacks the rule of `@latest`"
    );
    assert!(
        !palette.contains("`@latest` is the newest version of that copy"),
        "internals/palette/packs.md says that `@latest` is the newest version"
    );

    let description = schema("packs")
        .pointer("/$defs/PackBuild/properties/latest/description")
        .and_then(serde_json::Value::as_str)
        .map(one_line)
        .expect("the schema of packs describes latest");
    for needle in [
        "of the versions that the listing keeps.",
        "when the registry's copy has no version that is not a pre-release.",
        "So it can record another version when `--cloud` cuts the list.",
    ] {
        assert!(
            description.contains(needle),
            "the description of latest lacks {needle:?}: {description}"
        );
    }
}

/// Review of Wave 14, A13. The paragraph of `strata packs` in "The order of versions" states release
/// 0.25.0. It points at release 0.28.0, which changes `latest`.
#[test]
fn the_older_paragraph_of_strata_packs_points_at_the_change_of_latest_in_release_0_28_0() {
    let page = read(&docs().join("reference/compatibility.md"));
    let order = one_line(section(&page, "### The order of versions", Some("\n### ")));
    let item = section(
        &order,
        "**`strata packs` lists a pre-release below its version.**",
        Some("Three more results"),
    );
    assert!(
        item.contains(
            "Release 0.28.0 changes `latest`: see [the section of release 0.28.0](#release-0280-changes-that-need-action)."
        ),
        "the older paragraph lacks the pointer: {item}"
    );
    assert!(
        page.contains("\n## Release 0.28.0: changes that need action\n"),
        "the pointer needs the heading of the section"
    );
}

/// Review of Wave 14, A11. A value that is not text in an older read error names the older key, as
/// the file holds it. The book says so where it gives the older fields.
#[test]
fn the_book_says_that_an_older_read_error_that_is_not_text_names_the_older_key() {
    let documents = one_line(&read(&docs().join("reference/documents.md")));
    assert!(
        documents.contains("Strata reads them into `errors`, and a value that is not text gives `snapshot-schema` with the older key in `field`, such as `source.cluster.pendingRepaveError`."),
        "documents.md lacks the older key"
    );
}

/// Review of Wave 14, D3 and A8. Only a group with a password in values of no canonical form gets
/// a new `valuesHash`. A snapshot, `verify`, and `compare` of such a group give `serialize` when
/// strata cannot mask the password. Taking the record again gives the same error.
#[test]
fn the_book_states_which_groups_get_a_new_values_hash_and_what_a_password_that_strata_cannot_mask_does()
 {
    let command_line = release_0_28_0_part("### For a user of the command line or of a document\n");
    for needle in [
        "Other values keep their hash. That includes values of no canonical form that hold no password.",
        "A record of 0.27.0 of a group with such values has the same `policy` tier as a new record.",
        "A snapshot, `verify`, and `compare` of such a group give the error `serialize` and exit `2` when strata cannot mask a password in the values.",
        "In 0.27.0, `verify` and `compare` gave exit `0` for such a group.",
        "Taking the record again gives the same error. Quote the template, or change the block scalar in the values of the group.",
    ] {
        assert!(
            command_line.contains(needle),
            "the section of release 0.28.0 lacks {needle:?}"
        );
    }
    let page = one_line(&read(&docs().join("commands/snapshot/other-objects.md")));
    for needle in [
        "Such values with no password keep the hash that strata 0.27.0 gave.",
        "If strata cannot mask a password in such values, a snapshot, `verify`, and `compare` give the error `serialize` and exit `2`.",
        "A record taken again gives the same error. Quote the template, or change the block scalar in the values of the group.",
        "A record of strata 0.27.0 or before of a group with a password has the hash of the text with each password in clear",
    ] {
        assert!(page.contains(needle), "other-objects.md lacks {needle:?}");
    }
}

/// Review of Wave 14, A10. Four changes keep their old id. "The documents" says so, and gives the
/// reason, as it does for the changes of earlier releases.
#[test]
fn the_page_of_the_versions_names_the_four_values_that_change_after_0_27_0_with_no_new_id() {
    let page = read(&docs().join("reference/compatibility.md"));
    let documents = one_line(section(&page, "## The documents", Some("\n## ")));
    for needle in [
        "Four values change after 0.27.0 with no new id.",
        "[Release 0.28.0](#release-0280-changes-that-need-action) gives the action for each one.",
        "In `strata-packs/1`, `latest` marks the version that `pack:<name>@latest` records.",
        "In `strata-preflight/1`, `answer` of the check `iam-role-policy-quota` has the exit code of each command",
        "In `strata-snapshot/1`, `valuesHash` of a cluster group with a password in values of no canonical form covers the masked text.",
        "In `strata-unknown/1` and `strata-report/1`, the fields `palette`, `status`, and `errorCode` are gone.",
    ] {
        assert!(documents.contains(needle), "the rules lack {needle:?}");
    }
    // the paragraph stands before the three new fields of release 0.26.0
    let four = documents.find("Four values change after 0.27.0").unwrap();
    let after = documents
        .find("After 0.26.0, three documents have new fields")
        .unwrap();
    assert!(four < after);
    assert!(
        page.contains("\n## Release 0.28.0: changes that need action\n"),
        "the link of the paragraph has no section"
    );
}

/// Review of Wave 13, A12. The rule of the ids says that a new field keeps the id. The two
/// documents of the unknown values are the exception. The page of the versions says so.
#[test]
fn the_page_of_the_versions_says_that_the_two_documents_of_the_unknown_values_refuse_a_new_field() {
    let page = one_line(&read(&docs().join("reference/compatibility.md")));
    for needle in [
        "`strata-unknown/1` and `strata-report/1` refuse each field that they do not list",
        "A new field of either takes a new id.",
    ] {
        assert!(page.contains(needle), "the page lacks {needle:?}");
    }
    let documents = one_line(&read(&docs().join("reference/documents.md")));
    assert!(
        documents.contains(
            "A new field takes a new id: see [The documents](./compatibility.md#the-documents)."
        ),
        "documents.md does not send the reader to the rule"
    );
}

/// Review of Wave 13, A13: a `match` outside the library breaks on each public type of the send
/// that is `#[non_exhaustive]`. The section of release 0.28.0 names every one of them.
#[test]
fn the_section_of_release_0_28_0_names_each_non_exhaustive_type_of_the_send() {
    let source =
        read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/closed_set/unknown/send.rs"));
    let marked: Vec<&str> = source
        .lines()
        .zip(source.lines().skip(1))
        .filter(|(attribute, _)| attribute.trim() == "#[non_exhaustive]")
        .filter_map(|(_, item)| item.strip_prefix("pub enum ")?.split_whitespace().next())
        .collect();
    assert!(marked.len() >= 2, "the send has marked types: {marked:?}");
    let page = read(&docs().join("reference/compatibility.md"));
    let library = one_line(section(
        section(
            &page,
            "## Release 0.28.0: changes that need action",
            Some("\n## "),
        ),
        "### For a user of the library of strata\n",
        Some("\n### "),
    ));
    for name in marked {
        assert!(
            library.contains(&format!("`closed_set::unknown::send::{name}`")),
            "the section of release 0.28.0 does not name {name}"
        );
    }
}

// ---- the text findings of the review at the end of Wave 12, in Wave 14 ---------------------------

/// The review at the end of Wave 12 named passive sentences and "-ing" forms in the book and in
/// the legend of a status. Each row has the words that went, and the words that came.
#[test]
fn the_passive_sentences_and_the_ing_forms_that_the_review_at_the_end_of_wave_12_named_are_gone() {
    let sentences = [
        (
            "docs/src/automation/merge-gate.md",
            "was edited in place after the record was written",
            "A person edited the profile that the record names in place, after strata wrote the record.",
        ),
        (
            "docs/src/commands/verify.md",
            "since the record was taken",
            "since strata took the record",
        ),
        (
            "docs/src/development/code.md",
            "A record file is read first",
            "Strata reads a record file first",
        ),
        (
            "docs/src/development/build-test.md",
            "The document reads or is refused",
            "Strata reads the document or refuses it with a kind, and writes the same document again.",
        ),
        (
            "docs/src/development/build-test.md",
            "a rule of another tier than `static` is not run",
            "so the benchmark runs only the rules of the tier `static`",
        ),
        (
            "docs/src/development/build-test.md",
            "called a workflow asking for it",
            "called a workflow that asked for it",
        ),
        (
            "docs/src/internals/palette/packs.md",
            "is refused at creation",
            "Palette refuses the create of a profile that holds the requiring pack",
        ),
        (
            "docs/src/internals/palette/packs.md",
            "is pushed beside",
            "When a person pushes `1.0.1` beside `1.0.0`",
        ),
        (
            "docs/src/internals/palette/profiles.md",
            "in the other order is accepted",
            "Palette accepts a `PUT` of a published version",
        ),
        (
            "docs/src/reference/compatibility.md",
            "A field that is false, empty, or zero is omitted",
            "Strata omits a field that is false, empty, or zero",
        ),
        (
            "docs/src/reference/compatibility.md",
            "A 5xx that is not a refusal is not kept",
            "The backend keeps no 5xx that is not a refusal",
        ),
        (
            "docs/src/use-cases/publish-fan-out.md",
            "written against a level below that keeps moving",
            "A job wrote each one against the level below",
        ),
        (
            "docs/src/development/live-runs.md",
            "running for 37 minutes",
            "its EC2 instances ran for 37 minutes",
        ),
        (
            "docs/src/development/live-runs.md",
            "This read as Palette refusing the cluster",
            "This looked like a refusal from Palette, but it was not one.",
        ),
        (
            "src/ops/same_release.rs",
            "pinned to another registry",
            "a pack moved to another registry",
        ),
    ];
    for (file, old, new) in sentences {
        let text = one_line(&repo_file(file));
        assert!(!text.contains(old), "{file} still holds {old:?}");
        assert!(text.contains(new), "{file} lacks {new:?}");
    }
}

/// A table cell is a paragraph for the reader, so it has six sentences at most. The test of Wave 8
/// skips the rows of a table, and the module map of `build-test.md` held cells of 14 sentences.
#[test]
fn no_table_cell_of_the_book_holds_more_than_six_sentences() {
    let mut faults = Vec::new();
    for path in walk(&docs())
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
    {
        for block in blocks_of(&read(path)).iter().filter(|b| b.starts_with('|')) {
            for cell in block.trim_matches('|').split('|') {
                let count = sentences(cell.trim()).len();
                if count > MOST_SENTENCES {
                    faults.push(format!(
                        "{}: {count} sentences: {}",
                        path.display(),
                        cell.trim()
                    ));
                }
            }
        }
    }
    assert_no_faults(&faults);
}

/// The top of the everyday loop said "one step of a day" for rows that are not steps of each day.
/// It said "replaces the nodes" in two senses, and "makes the version" where the intro says
/// "publishes". It said "Nobody works". The chart said that it puts each step in its lane, and it
/// holds one change.
#[test]
fn the_top_of_the_everyday_loop_says_what_a_row_is_and_uses_one_word_for_each_thing() {
    let text = the_loop();
    for (old, new) in [
        (
            "Each row is one step of a day.",
            "Each row is one thing that you do, and what strata tells you then.",
        ),
        (
            "and if the change replaces the nodes. Palette says if it accepts the packs.",
            "and its impact level, such as a repave. Palette says if it accepts the packs.",
        ),
        (
            "| The pipeline makes the version in Palette |",
            "| The pipeline publishes the version |",
        ),
        ("| Nobody works |", "| Each night, with no person at work |"),
        (
            "The chart puts each step in its lane.",
            "The chart shows the steps of one change.",
        ),
        (
            "Palette says if the nodes change",
            "Palette says if the move replaces the nodes",
        ),
        (
            "Write a new cluster:<br/>",
            "Write a new cluster on the release:<br/>",
        ),
    ] {
        let page = one_line(&text);
        assert!(!page.contains(old), "loop.md still holds {old:?}");
        assert!(page.contains(new), "loop.md lacks {new:?}");
    }
}

/// The canonical text of a string moves more hashes than the item of release 0.26.0 names. The
/// general paragraph names each one, and the section of release 0.28.0 corrects the item.
#[test]
fn compatibility_names_each_hash_that_the_canonical_text_moves_and_what_same_release_says() {
    let page = read(&docs().join("reference/compatibility.md"));
    let general = one_line(section(
        &page,
        "One root changes after 0.25.0 with no new id.",
        Some("One saved directory changes after 0.25.0"),
    ));
    for needle in [
        "Each hash that covers such a character changes. These are `valuesHash`, `contentHash`, `variablesHash`, `hash.authored` and `hash.node` of a pack, `contentDigest`, and the `hash` of a tier. So the `root` changes.",
        "The policy tier of a cluster sorts its role bindings, their subjects, and its namespaces by the same text",
        "`same-release` of such a pair gives `different` and exit `1`.",
    ] {
        assert!(general.contains(needle), "the paragraph lacks {needle:?}");
    }
    let command_line = release_0_28_0_part("### For a user of the command line or of a document\n");
    for needle in [
        "**The root item of release 0.26.0 names four hashes. Four more hashes change with it.**",
        "They are `contentDigest`, `hash.authored` and `hash.node` of a pack, and the `hash` of a tier.",
        "gives `different` and exit `1`.",
    ] {
        assert!(
            command_line.contains(needle),
            "the section of release 0.28.0 lacks {needle:?}"
        );
    }
}

/// The list of "The order of versions" omitted three cases that follow from the rule. Two
/// versions can be one. A version that does not parse is above each version that parses.
/// `strata list` uses the same order. The section also says that no live run shows how Palette
/// orders a version with four numbers, so the reader knows whose order it is.
#[test]
fn the_order_of_versions_names_the_equal_versions_and_the_version_that_does_not_parse() {
    let page = read(&docs().join("reference/compatibility.md"));
    let order = one_line(section(
        &page,
        "### The order of versions",
        Some("\n## Release 0.28.0"),
    ));
    for needle in [
        "Two versions that differ only by a `v`, a missing `.0`, or build data are one version.",
        "`1.2` equals `1.2.0`, and `v1.2.3` equals `1.2.3`.",
        "`<name>@latest` gives `profile-name-ambiguous`",
        "A version that does not parse is above each version that parses.",
        "`behind` names no three-number version as newer than the pin `1.2.3.4`.",
        "`strata list` orders the profiles by the same order.",
        "The order of a version that does not parse is the order of strata. No live run shows how Palette orders such a version.",
    ] {
        assert!(order.contains(needle), "the section lacks {needle:?}");
    }
}

/// The fuzz package is the newest code, and the review at the end of Wave 12 found 42 comment
/// sentences in it over the limit. Each sentence of each comment in `fuzz/` has 25 words at most,
/// so the next wave cannot add one.
#[test]
fn each_comment_sentence_in_the_fuzz_package_has_25_words_at_most() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut faults = Vec::new();
    for path in walk(&root.join("fuzz"))
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
    {
        for block in comment_blocks(&read(path), "//") {
            faults.extend(sentence_faults(&path.display().to_string(), &block));
        }
    }
    assert_no_faults(&faults);
}

/// The review at the end of Wave 12 named comments of internal code with a sentence over
/// 25 words. Each sentence of each comment has 25 words at most now.
#[test]
fn the_internal_comments_that_the_review_at_the_end_of_wave_12_named_keep_to_25_words() {
    assert_comments_keep_to_the_limit(&[
        ("tests/ci_scripts.rs", "A `curl` for the GitLab REST API"),
        (
            "tests/common/mod.rs",
            "Assemble a cluster that runs the profile fixtures",
        ),
        (
            "tests/workflows.rs",
            "The build of the public items does not read",
        ),
        (".github/workflows/fuzz.yml", "A crash fails that job"),
        (
            "preflight/src/main.rs",
            "The color of one stream, by the rule of",
        ),
        ("src/api/palette/mod.rs", "The walk ends at a page"),
        (
            "src/reference/version.rs",
            "Of two pre-releases, the first identifier",
        ),
        (
            "scripts/ci/merge-gate.sh",
            "backwards DIFF: the pins that the merge result moves back",
        ),
        ("Cargo.toml", "It is the profile that the tool"),
    ]);
}

/// The review at the end of Wave 12 named modal and phrasal verbs in internal comments. The
/// phrases are gone, and the echo of the fuzz workflow gives one instruction in each sentence.
#[test]
fn the_modal_and_phrasal_verbs_that_the_review_at_the_end_of_wave_12_named_are_gone() {
    for (file, old) in [
        ("Cargo.toml", "would also build"),
        ("tests/verify.rs", "a request would give missing-credential"),
        ("preflight/tests/cli.rs", "where the directory should be"),
        ("tests/common/process.rs", "a file that it will run"),
        ("src/reference/mod.rs", "falls through"),
        ("src/config.rs", "fall through"),
        ("preflight/src/main.rs", "goes out one time"),
        ("tests/ci_scripts.rs", "steps aside"),
        (
            "xtask/src/tenant/clusters.rs",
            "take it out of the consumers",
        ),
        ("src/content/canon.rs", "read back as itself"),
    ] {
        let comments = comments_of(file).join(" ");
        assert!(!comments.contains(old), "{file} still holds {old:?}");
    }
    let fuzz = repo_file(".github/workflows/fuzz.yml");
    assert!(
        fuzz.contains("directory of the target. Run \\`cargo test -p strata-fuzz\\`."),
        "the echo of fuzz.yml does not give two sentences"
    );
}

/// The gate prints some of its text to a user, as the message of `die`. So its comments and its
/// messages keep to the rule of the style: no modal verb.
#[test]
fn the_merge_gate_script_holds_no_modal_verb() {
    let script = repo_file("scripts/ci/merge-gate.sh");
    for modal in ["may", "might", "should", "would"] {
        assert!(
            !script
                .split(|c: char| !c.is_alphanumeric())
                .any(|word| word.eq_ignore_ascii_case(modal)),
            "scripts/ci/merge-gate.sh holds {modal:?}"
        );
    }
}

/// The gate stops with exit 2 for a strata that is older than the script, and the legend of the
/// status `registry-move` has new words. The page of the gate and the section of release 0.28.0
/// say both.
#[test]
fn the_book_says_that_the_gate_stops_for_an_older_strata_and_the_legend_of_registry_move_changed() {
    let gate = one_line(&read(&docs().join("automation/merge-gate.md")));
    assert!(
        gate.contains(
            "It stops when strata is older than the script, because that strata writes no `versionMove`."
        ),
        "the page of the gate does not say it"
    );
    let command_line = release_0_28_0_part("### For a user of the command line or of a document\n");
    for needle in [
        "**`merge-gate.sh` stops with exit `2` when strata is older than the script.**",
        "A local run or a custom job must use the strata of the same release.",
        "**The legend text of the status `registry-move` of `same-release` changes.**",
        "says \"moved to another registry\" in place of \"pinned to another registry\"",
    ] {
        assert!(
            command_line.contains(needle),
            "the section of release 0.28.0 lacks {needle:?}"
        );
    }
    let legend = strata::closed_set::legend::legend::<strata::ops::PairStatus>();
    assert!(
        legend["registry-move"].contains("a pack moved to another registry"),
        "{}",
        legend["registry-move"]
    );
}

/// The doc comments of public items feed the `description` of `strata schema` and of
/// `strata-preflight --schema`. The split of long sentences gave some of them new words. The
/// section of release 0.28.0 names each one, and the schema holds the new words. The test of
/// `preflight/tests/cli.rs` holds the words of `strata-preflight`.
#[test]
fn the_section_of_release_0_28_0_names_each_schema_description_that_has_new_words() {
    for (kind, pointer, new_words) in [
        (
            "behind",
            "/$defs/Pin/properties/newer/description",
            "A version for none of the clouds of the pinned version is not one to move to either.",
        ),
        (
            "consumers",
            "/$defs/ClustersRead/description",
            "It gives an empty list on an error too.",
        ),
        (
            "diff",
            "/$defs/PackDiff/properties/tagRenamed/description",
            "because the tag name differs. The same name, layer, type, version,",
        ),
        (
            "verify-summary",
            "/$defs/PackDiff/properties/tagRenamed/description",
            "because the tag name differs. The same name, layer, type, version,",
        ),
        (
            "packs",
            "/$defs/PackBuild/properties/latest/description",
            "True for the version that `pack:<name>@latest` records, of the versions that the listing keeps.",
        ),
        (
            "packs",
            "/$defs/PackBuild/properties/paletteLatest/description",
            "Palette marks the highest version by the order of semantic versions",
        ),
    ] {
        let document = schema(kind);
        let description = document
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("{kind} has no description at {pointer}"));
        assert!(
            one_line(description).contains(new_words),
            "{kind} {pointer}: {description}"
        );
    }
    let command_line = release_0_28_0_part("### For a user of the command line or of a document\n");
    for needle in [
        "**Some descriptions in `strata schema` and in `strata-preflight --schema` have new words.**",
        "No key or id changes. One type changes: see the item for `fix.owner` below.",
        "`strata schema behind`: the description of `newer` in a pin.",
        "`strata schema consumers`: the description of the definition `ClustersRead`.",
        "`strata schema diff` and `strata schema verify-summary`: the description of `tagRenamed` in a pack.",
        "`strata-preflight --schema report`: the descriptions of the verdicts `not-run` and `skipped`, and of `answer` and `requests` in a check.",
        "`strata schema packs`: the descriptions of `latest` and `paletteLatest` in a pack build.",
        "`strata-preflight --schema report` and `--schema rules`: the description of `owner` in a fix.",
        "`strata-preflight --schema rules`: the descriptions of the checks `palette`, `exec`, and `watch`, and of `check` and `when` in a rule.",
    ] {
        assert!(
            command_line.contains(needle),
            "the section of release 0.28.0 lacks {needle:?}"
        );
    }
}

// ---- the comments that Wave 13 wrote, in Wave 14 ------------------------------------------------

/// A table, a usage text, a list, a fenced block, and a shebang are not sentences. The sweep of
/// the sentence length must not join them with the prose around them.
#[test]
fn a_literal_line_a_list_item_a_fence_and_a_shebang_are_not_joined_into_a_sentence() {
    let script = "#!/usr/bin/env bash\n# The first line of the text.\n# It has two lines.\n#\n\
                  #   script start   do the first thing\n#   script stop    do the second thing\n\
                  # 1. The first step\n# 2. The second step\n# ```\n# a command in a fence\n# ```\n\
                  # The last line.\n";
    assert_eq!(
        comment_blocks(script, "#"),
        [
            "The first line of the text. It has two lines.",
            "script start   do the first thing",
            "script stop    do the second thing",
            "1. The first step",
            "2. The second step",
            "The last line.",
        ]
    );
    let rust = "/// Item one.\n/// - a\n/// - b\n//! Inner.\ncode(); // a trailing remark\n";
    assert_eq!(
        comment_blocks(rust, "//"),
        ["Item one.", "- a", "- b Inner."]
    );
}

/// The directories that Wave 13 wrote. Each file in them is new since release 0.26.0.
const SWEPT_DIRECTORIES: [&str; 7] = [
    "deploy/contract",
    "deploy/report",
    "report-endpoint",
    "src/closed_set/unknown",
    "xtask/facts",
    "xtask/src/contract",
    "xtask/src/facts",
];

/// The files that Wave 13 wrote or changed, outside the directories above.
const SWEPT_FILES: [&str; 49] = [
    "examples/live/contract-report.sh",
    "examples/live/contract.sh",
    "examples/live/github.sh",
    "examples/live/preflight-plan.sh",
    "examples/live/report-send.sh",
    "examples/live/tiers.sh",
    "preflight/src/catalog.rs",
    "preflight/src/probe/backend.rs",
    "preflight/src/probe/tests.rs",
    "preflight/src/report/reproduce.rs",
    "preflight/src/rule/mod.rs",
    "preflight/src/rule/validate.rs",
    "preflight/src/run/tests.rs",
    "preflight/src/world/packs.rs",
    "preflight/src/world/tests.rs",
    "preflight/tests/catalog.rs",
    "preflight/tests/cli.rs",
    "scripts/dev/set-version.sh",
    "src/api/codes.rs",
    "src/cli/mod.rs",
    "src/cli/report.rs",
    "src/closed_set/unknown.rs",
    "src/error/class.rs",
    "src/error/mod.rs",
    "src/model/cluster_state.rs",
    "src/ops/report.rs",
    "src/render/report.rs",
    "src/term/ask.rs",
    "src/text.rs",
    "tests/ci_scripts.rs",
    "tests/cli.rs",
    "tests/common/answer.rs",
    "tests/common/closed_set.rs",
    "tests/common/mod.rs",
    "tests/common/stand_in.rs",
    "tests/contract.rs",
    "tests/dev_scripts.rs",
    "tests/docs.rs",
    "tests/live_scripts.rs",
    "tests/report.rs",
    "tests/report_deploy.rs",
    "tests/routes.rs",
    "tests/send.rs",
    "xtask/Cargo.toml",
    "xtask/src/book.rs",
    "xtask/src/chapters/fields.rs",
    "xtask/src/files.rs",
    "xtask/src/palette.rs",
    "xtask/src/tenant/mod.rs",
];

/// The files that the review of #598 named for a comment that Wave 14 wrote. The sweep of the
/// words reads them beside the files above.
const NAMED_FILES: [&str; 20] = [
    "examples/live/api.sh",
    "fuzz/src/palette.rs",
    "fuzz/src/record.rs",
    "scripts/ci/merge-gate.sh",
    "src/api/raw.rs",
    "src/candidate/pack.rs",
    "src/candidate/pack/served.rs",
    "src/candidate/resolve.rs",
    "src/diff/pack.rs",
    "src/ops/consumers/mod.rs",
    "src/ops/mod.rs",
    "src/render/terminal/mod.rs",
    "src/snapshot/cluster/tests.rs",
    "src/snapshot/policy.rs",
    "src/term/ask.rs",
    "tests/workflows.rs",
    "xtask/Cargo.toml",
    "xtask/src/tenant/registries.rs",
    "preflight/src/jq/comment.rs",
    "preflight/src/confidence.rs",
];

/// A file with comments in the tree of `root`, by the extension that it has.
fn has_comments(path: &Path) -> bool {
    let comment_files = ["rs", "sh", "yml", "yaml", "toml", "service", "timer"];
    let name = path.file_name().and_then(|n| n.to_str());
    !path.to_string_lossy().contains("/target/")
        && (name == Some("Dockerfile")
            || path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| comment_files.contains(&e)))
}

/// The files that Wave 13 wrote or changed: each file of `SWEPT_DIRECTORIES` that has comments,
/// and each file of `SWEPT_FILES`.
fn swept_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<PathBuf> = SWEPT_DIRECTORIES
        .iter()
        .flat_map(|dir| walk(&root.join(dir)))
        .filter(|p| has_comments(p))
        .collect();
    files.extend(SWEPT_FILES.iter().map(|file| root.join(file)));
    assert!(files.len() > 100, "the sweep found {} files", files.len());
    files
}

/// The comment blocks of a file of the tree: `//` in a Rust file, and `#` in any other.
fn comment_blocks_of_path(path: &Path) -> Vec<String> {
    let mark = if path.extension().is_some_and(|e| e == "rs") {
        "//"
    } else {
        "#"
    };
    comment_blocks(&read(path), mark)
}

/// The review at the end of Wave 13 found 112 comment sentences of 26 words or more in the code
/// that Wave 13 wrote. Each sentence of each comment in those files has 25 words at most now, and
/// each block has six sentences at most. The next wave cannot add one without a failure here.
#[test]
fn each_comment_sentence_in_the_files_that_wave_13_wrote_has_25_words_at_most() {
    let mut faults = Vec::new();
    for path in swept_files() {
        for block in comment_blocks_of_path(&path) {
            faults.extend(block_faults(&path.display().to_string(), &block));
        }
    }
    assert_no_faults(&faults);
}

// ---- the text findings of the review at the end of Wave 13, in Wave 14 ---------------------------

/// True when `file` holds `phrase` in its text, or in its comments. A comment that wraps holds its
/// phrase on two lines with a mark between them, so the comments are joined without the marks.
fn holds_phrase(file: &str, phrase: &str) -> bool {
    one_line(&repo_file(file)).contains(phrase) || comments_of(file).join(" ").contains(phrase)
}

/// The review named refused words and phrasal verbs in the comments of internal code. Each row has
/// the words that went, and the words that came.
#[test]
fn the_refused_words_and_the_phrasal_verbs_that_the_review_named_in_internal_comments_are_gone() {
    for (file, old, new) in [
        (
            "report-endpoint/src/merge.rs",
            "What a merge will do",
            "What a merge does, found with the reads alone.",
        ),
        (
            "report-endpoint/src/merge.rs",
            "How many issues the merge will open.",
            "How many issues the merge opens.",
        ),
        (
            "src/closed_set/unknown/hint.rs",
            "Else it would show on every run",
            "Else the hint shows on each run.",
        ),
        (
            "src/closed_set/unknown/hint.rs",
            "would show the hint on every run",
            "because the hint then shows on each run",
        ),
        (
            "xtask/src/contract/tests.rs",
            "there would make the next contract run",
            "there makes the next contract run report a false `type` difference.",
        ),
        (
            "src/closed_set/unknown/tests.rs",
            "use up the day",
            "must not record the time",
        ),
        (
            "preflight/src/probe/tests.rs",
            "a request that goes out is an error",
            "a request that the check sends is an error",
        ),
        (
            "xtask/src/contract/subject.rs",
            "no request goes out for a path",
            "the run sends no request for a path",
        ),
        (
            "src/file.rs",
            "A log goes through here",
            "A log uses this function.",
        ),
        (
            "tests/report.rs",
            "when the lines went out",
            "when the user sent the lines",
        ),
        (
            "src/closed_set/unknown/send/tests.rs",
            "parses back to the same document",
            "parses to the same document",
        ),
        (
            "tests/doc_lints.rs",
            "the lints stay on for each package",
            "the lints stay active for each package, and that no file disables one",
        ),
        (
            "tests/live_scripts.rs",
            "turns the switch off",
            "sets the switch to `false`",
        ),
        (
            "tests/live_scripts.rs",
            "stands in for the send",
            "A stand-in `strata` for the send",
        ),
        (
            "tests/live_scripts.rs",
            "writes down the settings",
            "It records the settings that it got.",
        ),
        (
            "tests/report_deploy.rs",
            "stands in, and writes down each call",
            "a stand-in `gcloud` on the `PATH` records each call",
        ),
        (
            "xtask/src/palette.rs",
            "It stands in for the endpoint",
            "It is a stand-in for the endpoint",
        ),
        (
            "xtask/src/contract/mod.rs",
            "finds out whether",
            "tells whether",
        ),
        (
            "deploy/contract/weekly.sh",
            "turns the result into issues",
            "writes the result as issues",
        ),
        (
            "examples/live/contract.sh",
            "turns the result into issues",
            "writes the result as issues",
        ),
        (
            "deploy/contract/timer.sh",
            "is logged in",
            "the timer runs only while the user has a session",
        ),
        (
            "deploy/contract/timer.sh",
            "no one is logged in",
            "when the user has no session",
        ),
        (
            "deploy/report/deploy.sh",
            "gcloud logged in",
            "a gcloud login",
        ),
        (
            "examples/live/contract-report.sh",
            "gh logged in",
            "a gh login for the repository",
        ),
        (
            "examples/live/report-send.sh",
            "gh logged in",
            "a gh login to read the issue",
        ),
        (
            "examples/live/contract-profiles.sh",
            "kinds stay off",
            "It makes no cluster group, no cluster template, and no registry kind",
        ),
    ] {
        assert!(!holds_phrase(file, old), "{file} still holds {old:?}");
        assert!(holds_phrase(file, new), "{file} lacks {new:?}");
    }
}

/// The review named passive sentences and "-ing" forms in the new sentences of the book. Each row
/// has the words that went, and the words that came.
#[test]
fn the_passive_sentences_and_the_ing_forms_that_the_review_named_in_the_new_book_text_are_gone() {
    for (file, old, new) in [
        (
            "docs/src/reference/report.md",
            "## How a report is made",
            "## How strata makes a report",
        ),
        (
            "docs/src/reference/report.md",
            "Delete the file to be asked again.",
            "To get the question again, delete the file.",
        ),
        (
            "docs/src/reference/report.md",
            "Or the run has `--quiet` or `STRATA_QUIET`.",
            "- The run has `--quiet` or `STRATA_QUIET`.",
        ),
        (
            "docs/src/reference/report.md",
            "a new log is reported whole",
            "so the report of a new log holds each line",
        ),
        (
            "docs/src/development/facts.md",
            "can stop agreeing with each other",
            "can then disagree with each other, or with Palette",
        ),
        (
            "docs/src/development/facts.md",
            "No one knows how the fact is proved.",
            "No one knows how to prove the fact.",
        ),
        (
            "docs/src/development/facts.md",
            "The code has a typing fault",
            "The fact spells the code wrong, or the page lost it.",
        ),
        (
            "docs/src/development/facts.md",
            "An id is used twice",
            "Two facts have one id, or one request.",
        ),
        (
            "docs/src/development/facts.md",
            "The label is created when the repository has none.",
            "The script creates the label when the repository has none.",
        ),
        (
            "docs/src/development/facts.md",
            "when a fact or a release could not be checked",
            "when the run could not check a fact or a release",
        ),
        (
            "docs/src/development/facts.md",
            "The contract script is not shipped",
            "No release ships the contract script",
        ),
        (
            "docs/src/development/report-endpoint.md",
            "the send and the endpoint are tested together",
            "so one test covers the send and the endpoint",
        ),
        (
            "docs/src/reference/documents.md",
            "when `--release` is given",
            "`release`, when the run names `--release`",
        ),
    ] {
        let text = one_line(&repo_file(file));
        assert!(!text.contains(old), "{file} still holds {old:?}");
        assert!(text.contains(new), "{file} lacks {new:?}");
    }
}

/// The paragraph of a `what` in a rule file has six sentences or fewer. A user reads the message
/// of a rule and not its `what`, so only a rule author feels the length. The review named eleven
/// rules whose `what` had one long paragraph. A blank line divides it now.
#[test]
fn the_what_of_each_rule_that_the_review_named_has_six_sentences_or_fewer_in_a_paragraph() {
    let named = [
        ("quotas", "aws-quotas"),
        ("quotas", "gcp-quotas"),
        ("quotas", "azure-vcpu-quota"),
        ("identity", "iam-role-policy-quota"),
        ("identity", "iaas-pod-identity"),
        ("compute", "instance-offering"),
        ("compute", "eks-fargate-private"),
        ("networking", "eks-unsupported-zone"),
        ("networking", "subnet-ips"),
        ("networking", "private-egress"),
        ("networking", "static-subnets"),
        ("kubernetes", "pack-cloud-support"),
    ];
    for (section, id) in named {
        let text = repo_file(&format!("preflight/rules/{section}.yaml"));
        let file: serde_yaml::Value = serde_yaml::from_str(&text).unwrap();
        let what = file["rules"]
            .as_sequence()
            .and_then(|rules| rules.iter().find(|rule| rule["id"] == id))
            .and_then(|rule| rule["what"].as_str())
            .unwrap_or_else(|| panic!("{section}.yaml has no rule {id} with a `what`"));
        let paragraphs: Vec<&str> = what.split("\n\n").collect();
        assert!(paragraphs.len() > 1, "{id}: one paragraph");
        for paragraph in paragraphs {
            let found = sentences(&one_line(paragraph)).len();
            assert!(
                found <= MOST_SENTENCES,
                "{id}: {found} sentences in: {paragraph}"
            );
        }
    }
}

/// The review found gaps between the report of the unknown values and the pages that a team reads
/// to run a pipeline. A job sends the report with one variable, and a script needs the schema.
#[test]
fn the_pages_of_a_pipeline_name_the_setting_of_the_report_the_log_and_the_schema() {
    let setting = "`STRATA_REPORT=1` in a job sends the report at the end of a run that wrote a line \
                   to the log. The log is in the state folder of the runner, so save the folder when \
                   the runner does not stay.";
    let scripts = one_line(&read(&docs().join("automation/scripts.md")));
    assert!(scripts.contains(setting), "scripts.md lacks the setting");
    let use_case = one_line(&read(&docs().join("use-cases/unknown-value.md")));
    assert!(
        use_case.contains(
            "The log is in the state folder of the runner, so save the folder when the runner does \
             not stay."
        ),
        "the use case lacks the log of the runner"
    );
    assert!(
        !use_case.contains("copy the log from its state folder"),
        "a hosted runner is gone after the job"
    );
    for drop_in in [
        "examples/ci/github/strata.yml",
        "examples/ci/gitlab/strata.yml",
    ] {
        let comments = comments_of(drop_in).join(" ");
        assert!(comments.contains("STRATA_REPORT"), "{drop_in}: no setting");
        assert!(
            comments.contains("state folder of the runner"),
            "{drop_in}: no log of the runner"
        );
    }
    for page in ["reference/report.md", "commands/report.md"] {
        let text = one_line(&read(&docs().join(page)));
        assert!(
            text.contains("`strata schema report` prints its JSON Schema."),
            "{page} lacks the schema of the report"
        );
    }
    let documents = one_line(&read(&docs().join("reference/documents.md")));
    assert!(
        documents.contains(
            "`strata schema report` and `strata preflight --schema report` print two different \
             schemas."
        ),
        "documents.md lacks the sentence on the two schemas"
    );
}

/// The table of files of the live example names each script that a recipe runs, and the list of
/// recipes holds `just live-report-send`.
#[test]
fn the_live_example_lists_the_contract_scripts_the_send_script_and_the_recipe_of_the_send() {
    let page = read(&docs().join("development/live-example.md"));
    for script in ["contract.sh", "contract-report.sh", "report-send.sh"] {
        assert!(
            page.contains(&format!("| `{script}` |")),
            "the table of files lacks {script}"
        );
        assert!(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("examples/live")
                .join(script)
                .exists(),
            "{script} is not a live script"
        );
    }
    assert!(
        page.contains(
            "just live-report-send URL                                                      # #397, \
             needs the address of the endpoint and a `gh` login; reads no tenant"
        ),
        "the list of recipes lacks just live-report-send"
    );
}

/// The section of "The command line" says that the message of `api` for a 403 with the code
/// `ProjectUidIsNotEmpty` changed after 0.26.0. Release 0.26.0 listed its own message change in
/// the same way. The library of `strata` has a paragraph "After 0.26.0". Each statement compares
/// with 0.26.0. The section of release 0.27.0 is published, so the section of 0.28.0 holds the
/// correction.
#[test]
fn compatibility_compares_each_change_of_the_wave_with_release_0_26_0() {
    let page = one_line(&read(&docs().join("reference/compatibility.md")));
    for needle in [
        "After 0.26.0, the message of `api` for a 403 with the code `ProjectUidIsNotEmpty` says that \
         the route is of the tenant scope. The kind `api` and the exit code `2` stay.",
        "After 0.26.0, the library of `strata` changes in these ways.",
        "`closed_set::unknown` is new.",
        "Its `Report` and `report::Entry` refuse a field that they do not list.",
        "`ExecCheck` is new. It has the field `palette: Option<rule::PaletteRead>`",
        "`rule::ExecCheck` has the field `also: Vec<String>`",
        "After 0.26.0, three documents have new fields, and each keeps its id.",
        "`strata-preflight-rules/1` has the new keys `refusal_codes` in a check of kind `palette`, and \
         `palette` and `also` in a check of kind `exec`.",
    ] {
        assert!(page.contains(needle), "compatibility.md lacks {needle:?}");
    }
    for old in [
        "`ExecCheck` has the new field `palette",
        "`rule::ExecCheck` has the new field `also",
        "After 0.26.0, two documents have new fields",
    ] {
        assert!(!page.contains(old), "compatibility.md still holds {old:?}");
    }
    let library = release_0_28_0_part("### For a user of the library of strata\n");
    for needle in [
        "**A correction to the section of release 0.27.0.**",
        "The module `closed_set::unknown` is new in 0.27.0",
        "Release 0.26.0 has none of them.",
        "That section says that `Report` and `report::Entry` \"now implement `Deserialize`\".",
    ] {
        assert!(
            library.contains(needle),
            "the section of 0.28.0 lacks {needle:?}"
        );
    }
}

/// The live window of Wave 14 gave results for rows that said "no result yet". The book states
/// each one as a fact, with no run id and no date.
#[test]
fn the_rows_of_the_live_window_of_wave_14_state_their_results() {
    let page = one_line(&read(&docs().join("development/live-runs.md")));
    for needle in [
        "#576: with a cluster up, `just live-contract --with-cluster` exits 0. The run finds 120 \
         facts: 63 `same`, none `differs`, 57 skipped, none that the run cannot check, and 0 \
         unknown values.",
        "No fact of the floor is an `error`.",
        "its reason names `ResourceNotFound` with HTTP 404.",
        "`just contract-report --dry-run runs/contract.json` exits 0 and opens no issue.",
        "#576: the fact `validate-security-groups` is `same`, and it was `same` before #576.",
        "so no live run shows the search past the first region. The tests show the search. No live \
         run shows a first region with no VPC.",
        "the choices `gcp` and `azure` have no way to find a value for `{region}`.",
        "The check `issue` passes. The service found the issue of the value under the label \
         `unknown-value`",
        "it did not run, because the issue of the value was open.",
    ] {
        assert!(page.contains(needle), "live-runs.md lacks {needle:?}");
    }
    for old in [
        "#576, no result yet",
        "No result yet: the endpoint had the label `palette-drift` until #574.",
    ] {
        assert!(!page.contains(old), "live-runs.md still holds {old:?}");
    }
}

// ---- the review of Wave 14: the end-of-wave findings of #589 --------------------------------------

/// A1, A2, A3, A4, A6, and S4: the section of release 0.28.0 names each change of
/// `strata-preflight` that the review found. Each item gives its action.
#[test]
fn the_section_of_release_0_28_0_names_each_change_of_strata_preflight_that_the_review_found() {
    let command_line = release_0_28_0_part("### For a user of the command line or of a document");
    for needle in [
        // A6: a text that is no version reads as it did in 0.27.0
        "A text that is no version reads as it did in 0.27.0. Examples are `1.26.x`, `1.26.15.1`, and `4.10.x`.",
        // A2 and S4: the owner is a closed set, and the description list holds the five items
        "**`strata-preflight`: the `owner` of a fix is one of six values.**",
        "A rule file of `--probes` with another owner stops the run with the error `catalog` and exit `2`.",
        "Use `operator`, and name the team in the text of the fix.",
        "`fix.owner` in `strata-preflight/1` and in `strata-preflight-rules/1` is a list of six values in the schema",
        "`report::Report` refuses a report of 0.27.0 with another owner.",
        "No key or id changes. One type changes",
        "`strata schema packs`: the descriptions of `latest` and `paletteLatest` in a pack build.",
        "the descriptions of the verdicts `not-run` and `skipped`, and of `answer` and `requests` in a check.",
        // A3: the exit code of a deleted cluster
        "**`strata-preflight --watch`: a deleted cluster ends the watch.**",
        "the exit code was `2`",
        "The exit code is `0` when no counted rule failed, as it was for the state `Deleted`.",
        // A4: the words of a count and of the messages of the rules
        "**`strata-preflight`: the words of a count and of the messages of the rules change.**",
        "`summary.verdict`, `subject.source` of a watch, and the last line of `--promotion`",
        "The messages of `aws-quotas`, `gcp-quotas`, and `azure-vcpu-quota` have new words.",
    ] {
        assert!(
            command_line.contains(needle),
            "the part of the command line lacks {needle:?}"
        );
    }
    let preflight = release_0_28_0_part("### For a user of the library of strata-preflight");
    for needle in [
        "Change `is_answer(status)` to `is_answer(status, Reader::Check)`.",
        "Change `unreadable(status)` to `is_answer(status, Reader::World)`.",
        "`strata::model::validation::PaletteRefusal::from_error_body` in its place.",
        "Call `catalog::builtin_ledger()`",
        "`Ledger::parse(catalog::LEDGER, ...)`",
        "`rule::Fix::owner` is a `rule::Owner`",
        "`Owner::as_str` gives the text.",
        "with a definition of the filter in front. So the line runs in the `jq` tool.",
    ] {
        assert!(
            preflight.contains(needle),
            "the strata-preflight part lacks {needle:?}"
        );
    }
}

/// S1: the sentence about the nouns names the whole text of the tool, and the rules write no
/// `(s)`. The page states the new words of the messages (#594).
#[test]
fn compatibility_says_that_the_whole_text_of_the_tool_agrees_each_noun_with_its_count() {
    let page = one_line(&read(&docs().join("reference/compatibility.md")));
    for needle in [
        "After 0.27.0, the text of `strata-preflight` agrees each noun with its count.",
        "It holds for the events of `--verbose` and for the message of each rule too.",
        "A message that wrote `2 key pair(s)` now writes `2 key pairs`.",
        "The messages of 17 rules wrote `(s)` after a noun.",
        "A count of zero reads `no`, as the summary does.",
        "`eks-fargate-private` says `the route table of 2 subnets (a, b) sends 0.0.0.0/0 to igw-1`.",
        "`pvc-lb-leftover` says `1 EBS volume ... is available` or `2 EBS volumes ... are available`.",
        "`leftovers-before-build` says `1 instance ... is still in` and `left it`",
        "`edge-host-duplicate` says `is in the pools a, b`.",
        "`2 | count_of(\"key pair\")` gives `2 key pairs`",
    ] {
        assert!(page.contains(needle), "the page lacks {needle:?}");
    }
    for old in [
        "After 0.27.0, the counts of the summary of `strata-preflight` agree each noun with the count.",
        "The message of a rule is outside this change: it can still write `2 key pair(s)`.",
    ] {
        assert!(!page.contains(old), "the page still holds {old:?}");
    }
}

/// S1: each rule that the page of the compatibility names for the new nouns exists in the
/// catalog.
#[test]
fn the_rules_that_compatibility_names_for_the_new_nouns_are_rules_of_the_catalog() {
    let page = read(&docs().join("reference/compatibility.md"));
    let item = section(
        &page,
        "  - The rules are `aws-region-not-enabled`",
        Some("\n  - Six messages"),
    );
    let ids: Vec<&str> = item
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|word| word.contains('-'))
        .collect();
    assert_eq!(ids.len(), 17, "{ids:?}");
    let catalog: String = [
        "cloud-services",
        "compute",
        "identity",
        "lifecycle",
        "networking",
        "profiles",
        "quotas",
        "storage",
        "teardown",
    ]
    .map(|name| repo_file(&format!("preflight/rules/{name}.yaml")))
    .concat();
    for id in ids {
        assert!(
            catalog.contains(&format!("  - id: {id}\n")),
            "the rule {id} is not in the catalog"
        );
    }
}

/// A3: the page of the watch gives the exit code of a deleted cluster, and the words of its
/// verdict. The output of the page comes from the stand-in (#594).
#[test]
fn the_page_of_the_watch_gives_the_exit_code_and_the_verdict_of_a_deleted_cluster() {
    let page = one_line(&read(&docs().join("commands/preflight/watch.md")));
    for needle in [
        "A deleted cluster gives exit `0` when no counted rule failed.",
        "The last line of the report says that the cluster is gone:",
        "{{#include ../../output/preflight-watch-gone.txt}}",
        "A failed rule or an error keeps its own words, also for a deleted cluster.",
        "A cluster that is not gone gives `OK: each counted check passed`.",
    ] {
        assert!(page.contains(needle), "{page}");
    }
    let output = read(&docs().join("output/preflight-watch-gone.txt"));
    assert_eq!(
        output.lines().last(),
        Some("OK: the cluster is gone; no counted check failed")
    );
    let compat = one_line(&read(&docs().join("reference/compatibility.md")));
    for needle in [
        "It was `OK: each counted check passed`, and it is `OK: the cluster is gone; no counted check failed` now.",
        "A watch with no check that ran says `OK: the cluster is gone; no check ran`.",
        "`report::Assembly` has the new field `cluster_gone: bool`.",
    ] {
        assert!(compat.contains(needle), "the page lacks {needle:?}");
    }
}

/// C4: the page of the compatibility names the seven rules that read a version as `strata` does.
/// It says what the four new ones change (#594).
#[test]
fn compatibility_names_the_four_rules_that_cut_a_version_at_the_dots_and_what_changes() {
    let command_line = release_0_28_0_part("### For a user of the command line or of a document");
    for needle in [
        "`gke-version-not-offered`, `eks-version-not-offered`, `eks-addon-version`, and `maas-os-image`.",
        "The text `v1.32.1` gave the line `v1.32`, and `1.32+build.5` gave `1.32+build`.",
        "Both texts give the line `1.32`, as `1.32.1` did.",
        "`maas-os-image` reads the numbers 22 and 4 as Ubuntu 22.04, so the text `22.4` passes now.",
        "The six rules that cut a text read the first two parts between the dots.",
    ] {
        assert!(
            command_line.contains(needle),
            "the part of the command line lacks {needle:?}"
        );
    }
}

/// S4: the page of the custom probes names the fault of an owner that is not one of the six.
#[test]
fn the_page_of_the_custom_probes_names_the_fault_of_an_owner_that_is_not_one_of_six() {
    let page = one_line(&read(&docs().join("commands/preflight/rules.md")));
    for needle in [
        "A file with a `fix.owner` that is not one of six values fails in the same way.",
        "Use `operator`, and name the team in the text of the fix.",
    ] {
        assert!(page.contains(needle), "the page lacks {needle:?}");
    }
}

// ---- the review of Wave 14: the end-of-wave findings of #590 --------------------------------------

/// S3: the pages of the log do not count the faults of a line. Each page names a field that is
/// missing, a field of the wrong type, and a word that is not a word. The tests of `tests/report.rs`
/// show that each fault gives `log-invalid`.
#[test]
fn the_pages_of_the_log_list_each_fault_of_a_line_and_count_none() {
    let pages = [
        (
            "commands/report.md",
            "These faults make a line not valid:",
            "A field that is missing, or that has the wrong type.",
        ),
        (
            "reference/report.md",
            "A line is not valid in these cases:",
            "A field is missing, or it has the wrong type.",
        ),
    ];
    for (page, lead, item) in pages {
        let text = one_line(&read(&docs().join(page)));
        for needle in [lead, item, "`strata-unknown/1`, or", "is not a short word."] {
            assert!(text.contains(needle), "{page} lacks {needle:?}");
        }
        for old in [
            "Three faults make a line",
            "A line is not valid in three cases",
        ] {
            assert!(!text.contains(old), "{page} still holds {old:?}");
        }
    }
    let errors = one_line(&read(&docs().join("reference/errors.md")));
    assert!(
        errors.contains("a field outside the list, a missing field, or a field of the wrong type."),
        "errors.md lacks the missing field"
    );
}

/// A5: the item of the value says what a report of a client of 0.27.0 gets. It also says what the
/// send of a run does after the update: it skips the line, and `strata report` still stops. The page
/// of the report states the rule of the endpoint.
#[test]
fn compatibility_says_what_a_client_of_0_27_0_gets_for_a_value_that_is_not_one_word() {
    let command_line = release_0_28_0_part("### For a user of the command line or of a document");
    for needle in [
        "The endpoint of 0.27.0 took a report with such a value. The endpoint of 0.28.0 answers 422 \
         for the whole report.",
        "A client of 0.27.0 then marks no line, so it sends the same report at each run.",
        "No other value of that log reaches the endpoint.",
        "After the update, the send of a run skips such a line. It sends the other lines of the log.",
        "The run writes a warning that gives the count of the skipped lines and the number of the first one.",
        "`--quiet` hides the warning, and the exit code stays `0`.",
        "`strata report` still stops with the error `log-invalid` on such a line.",
    ] {
        assert!(
            command_line.contains(needle),
            "the part of the command line lacks {needle:?}"
        );
    }
    let reference = one_line(&read(&docs().join("reference/report.md")));
    assert!(
        reference.contains(
            "It refuses a value that is not one word of 64 printable ASCII characters or fewer, \
             with no space."
        ),
        "report.md lacks the rule of the value for the endpoint"
    );
}

/// A9: a program that called `Watch::palette_release` wrote the field `palette`. So the item does not
/// say that only a person edited such a log.
#[test]
fn compatibility_says_that_a_program_of_the_library_can_have_written_the_field_palette() {
    let command_line = release_0_28_0_part("### For a user of the command line or of a document");
    for needle in [
        "A log or a report of a command of 0.27.0 never held them.",
        "Only a log that a person edited, or the log of a program that called \
         `Watch::palette_release`, has one.",
    ] {
        assert!(
            command_line.contains(needle),
            "the part of the command line lacks {needle:?}"
        );
    }
    for old in [
        "A log or a report of 0.27.0 never held them.",
        "Only a log that a person edited has one.",
    ] {
        assert!(
            !command_line.contains(old),
            "the part of the command line still holds {old:?}"
        );
    }
}

/// D2: the pages of the endpoint state the sweep as the code does it. It drops the hash of a sender
/// and the hash of a report, and a request that a check refuses also runs it. The tests of
/// `report-endpoint/src/server/tests.rs` show the code.
#[test]
fn the_pages_of_the_endpoint_say_that_the_sweep_drops_both_hashes_also_for_a_refused_request() {
    let service = one_line(&read(&docs().join("development/report-endpoint.md")));
    for needle in [
        "The service also drops each sender whose hour is over, and each hash of a report whose day \
         is over. It does this when a request comes, one time in a minute at most. A request that \
         a check refuses also does it.",
        "The service drops an old hash of either kind when a request comes, one time in a minute at \
         most. A request that a check refuses also drops it.",
    ] {
        assert!(
            service.contains(needle),
            "report-endpoint.md lacks {needle:?}"
        );
    }
    let reference = one_line(&read(&docs().join("reference/report.md")));
    assert!(
        reference.contains("The service drops it soon after the day."),
        "report.md lacks the end of the hash of a report"
    );
}

// ---- the review of Wave 14: the send, the endpoint, and report-send.sh keep one text (#593) ------

/// A5: the send skips a line that is not valid, and `strata report` still stops on it. The tests of
/// `tests/send.rs` show the code.
#[test]
fn the_pages_of_the_report_say_that_the_send_skips_a_line_that_is_not_valid() {
    let reference = one_line(&read(&docs().join("reference/report.md")));
    for needle in [
        "The send of a run does not stop on such a line: see [What a run sends](#what-a-run-sends).",
        "A line of the log that is not valid does not stop the send. Strata skips the line and sends \
         the valid lines. The mark counts the valid lines only.",
        "`strata report` still stops on such a line. Delete the line to stop the warning. `--quiet` \
         hides the warning.",
    ] {
        assert!(reference.contains(needle), "report.md lacks {needle:?}");
    }
    let command = one_line(&read(&docs().join("commands/report.md")));
    assert!(
        command.contains(
            "The send of a run skips such a line: see \
             [What a run sends](../reference/report.md#what-a-run-sends)."
        ),
        "the command page lacks the skip of the send"
    );
    let compatibility = release_0_28_0_part("### For a user of the command line or of a document");
    for old in [
        "reads the same log and stops with `log-invalid`",
        "sends no value from then on, and shows no sign",
    ] {
        assert!(
            !compatibility.contains(old),
            "compatibility.md still holds {old:?}"
        );
    }
}

/// D5: the pages state that the endpoint can hold back some values, and what strata marks.
#[test]
fn the_pages_say_what_the_send_marks_when_the_endpoint_holds_back_a_value() {
    let reference = one_line(&read(&docs().join("reference/report.md")));
    for needle in [
        "When the hour has no room for any value of a report, the endpoint answers 429 with the time \
         to wait.",
        "When the hour has room for some values, the endpoint answers 202 and lists the entries that \
         wait. Strata marks the lines before the first line of a value that waits, and it stops.",
        "So a value that the endpoint took after that line counts one more time.",
    ] {
        assert!(reference.contains(needle), "report.md lacks {needle:?}");
    }
    let service = one_line(&read(&docs().join("development/report-endpoint.md")));
    for needle in [
        "Strata reads `waiting`. It marks the lines before the first line of a value that waits, and \
         it stops. So a value that waits comes back with the next send.",
        "the service counts a line that it took one more time.",
    ] {
        assert!(
            service.contains(needle),
            "report-endpoint.md lacks {needle:?}"
        );
    }
    let library = release_0_28_0_part("### For a user of the library of strata");
    for needle in [
        "`closed_set::unknown::send::post` gives `Result<Vec<usize>>`.",
        "`ops::send_unknown_report` gives `Result<ops::Sent>`.",
        "The new type has the fields `marked`, `waiting`, and `skipped`.",
        "it never gives `Error::LogInvalid`.",
        "the function marks the lines before the first line of such a value, and stops.",
    ] {
        assert!(
            library.contains(needle),
            "the library part lacks {needle:?}"
        );
    }
}

/// D8: `code_span` writes no escape, and `code_cell` writes the span for a cell.
#[test]
fn compatibility_names_code_cell_beside_code_span() {
    let library = release_0_28_0_part("### For a user of the library of strata");
    assert!(
        library.contains("`text::code_span` and `text::code_cell` are new."),
        "the library part lacks code_cell"
    );
    assert!(
        library.contains("It escapes nothing, for a title and a sentence."),
        "the library part lacks the rule of code_span"
    );
}

/// D9: the page of the endpoint states what the way under the limit of 1000 issues costs.
#[test]
fn the_page_of_the_endpoint_states_that_removing_the_label_opens_a_second_issue() {
    let service = one_line(&read(&docs().join("development/report-endpoint.md")));
    for needle in [
        "To make room, remove the label from the issue of a value only when no user runs a release \
         that does not know the value.",
        "The service lists its label alone, so it does not find an issue with no label. The next \
         report of that value opens a second issue",
        "So a set and a value have one issue only while the issue has the label.",
    ] {
        assert!(
            service.contains(needle),
            "report-endpoint.md lacks {needle:?}"
        );
    }
    assert!(
        !service
            .contains("Remove the label from the issues of values that a release of strata names."),
        "report-endpoint.md still advises the removal with no condition"
    );
}

/// A12: the page of the move names the values that get a new issue after the update.
#[test]
fn the_page_of_the_move_says_that_a_value_in_another_form_gets_a_new_issue() {
    let service = one_line(&read(&docs().join("development/report-endpoint.md")));
    for needle in [
        "An issue of a value that this release writes in another form stays as it is.",
        "This release sends a `?` for each such character, and it ends a long value with `...`.",
        "The next report of such a value opens a new issue, and the first issue gets no more counts.",
    ] {
        assert!(
            service.contains(needle),
            "report-endpoint.md lacks {needle:?}"
        );
    }
    let section = service
        .split("### Move the issues to the label of the service")
        .nth(1)
        .and_then(|rest| rest.split("### Make the two accounts").next())
        .expect("the page has the section of the move");
    assert!(
        section.contains("An issue of a value that this release writes in another form"),
        "the note is not in the section of the move"
    );
}

/// C5: the page names the manifest that holds the repository.
#[test]
fn the_page_of_the_endpoint_names_the_root_manifest_for_the_repository() {
    let service = one_line(&read(&docs().join("development/report-endpoint.md")));
    assert!(
        service.contains("| `GITHUB_REPOSITORY` | the repository of the root `Cargo.toml` |"),
        "report-endpoint.md names another manifest"
    );
    assert!(!service.contains("report-endpoint/Cargo.toml"));
}

// ---- the review of Wave 14: the end-of-wave findings of #598 -----------------------------------

/// Two comments that are descriptions in the JSON Schema of a document, with words that the scan
/// refuses. A new word in one changes the schema, and `compatibility.md` names such a change. So
/// the scan skips a block that holds one of these phrases.
const SCHEMA_DESCRIPTIONS: [(&str, &str); 2] = [
    ("preflight/src/rule/mod.rs", "so that it can go up"),
    ("preflight/src/rule/mod.rs", "is filled in the report"),
];

/// S6: the comments of the swept files and of the named files use no refused word, no phrasal
/// verb, and no idiom. This file holds those words in its own data, so the scan skips it.
#[test]
fn the_comments_of_the_swept_files_use_no_refused_word_no_phrasal_verb_and_no_idiom() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let own = root.join("tests/docs.rs");
    let mut files = swept_files();
    files.extend(NAMED_FILES.iter().map(|file| root.join(file)));
    let mut faults = Vec::new();
    for path in files.into_iter().filter(|p| *p != own) {
        let place = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        for block in comment_blocks_of_path(&path) {
            let schema = SCHEMA_DESCRIPTIONS
                .iter()
                .any(|(file, phrase)| *file == place && block.contains(phrase));
            if !schema {
                faults.extend(wording_faults(&place, &block));
            }
        }
    }
    assert_no_faults(&faults);
}

/// S5: the sentences and the blocks that the review named in the book keep to the limits of "The
/// style rules". A sentence has 25 words at most, and 20 in a step of a procedure. A paragraph,
/// a list item, or a table row has six sentences at most. The fix also removes a passive clause.
#[test]
fn the_text_of_the_book_that_the_review_of_wave_14_named_keeps_to_the_limits() {
    let blocks = [
        (
            "commands/snapshot/other-objects.md",
            "A record of strata 0.27.0 or before",
        ),
        (
            "commands/snapshot/other-objects.md",
            "The hash covers the mask, so a new password",
        ),
        ("development/live-example.md", "| `github.sh` |"),
        ("development/live-example.md", "| `lib.sh` |"),
        (
            "internals/palette/limits.md",
            "A region that AWS refuses for the VPC route",
        ),
        (
            "internals/preflight.md",
            "A rule file has no shared jq definition",
        ),
        (
            "reference/preflight-rules.md",
            "`count_of` writes the input count",
        ),
        (
            "reference/preflight-rules.md",
            "Write the noun in the singular.",
        ),
        (
            "reference/preflight-rules.md",
            "`packFacts` holds three fields",
        ),
        (
            "reference/preflight-rules.md",
            "`state` is the `system_state` of the pack",
        ),
    ];
    let mut faults = Vec::new();
    for (page, anchor) in blocks {
        let text = read(&docs().join(page));
        faults.extend(block_faults(page, &block(&text, anchor)));
    }

    // a step of the procedure has 20 words at most in a sentence
    let page = "development/report-endpoint.md";
    let step = block(
        &read(&docs().join(page)),
        "Move the issues of an older service",
    );
    faults.extend(word_faults(
        page,
        &sentences(&step),
        MOST_WORDS_IN_A_PROCEDURE,
    ));

    // the paragraph of the watch has long text before the sentence, so only the sentence counts
    let page = "use-cases/stop-a-build.md";
    let text = read(&docs().join(page));
    let paragraph = block(&text, "If Palette deletes the cluster");
    let found = sentences(&paragraph);
    let sentence = found
        .iter()
        .find(|s| s.contains("If Palette deletes the cluster"))
        .expect("the sentence of the deleted cluster");
    faults.extend(word_faults(
        page,
        std::slice::from_ref(sentence),
        MOST_WORDS,
    ));
    assert_no_faults(&faults);

    let build_test = one_line(&read(&docs().join("development/build-test.md")));
    assert!(!build_test.contains("the record that was read"));
    assert!(build_test.contains("the record that strata read"));
}

/// S6 and S7: the words that the review named in the comments are gone, and the words that came in
/// their place are there. A row has the file, the old words, and the new words. The scan above
/// keeps the old words out of the swept files. This test names the places that the review found.
#[test]
fn the_comments_that_the_review_of_wave_14_named_say_what_the_code_does_in_plain_words() {
    let rows = [
        (
            "report-endpoint/src/merge.rs",
            "How many issues the merge will change.",
            "How many issues the merge changes.",
        ),
        (
            "src/snapshot/cluster.rs",
            "so the error would name the parent",
            "so the error names the parent",
        ),
        (
            "tests/common/mod.rs",
            "so a run that sends would post there",
            "so a run that sends posts there",
        ),
        (
            "tests/common/mod.rs",
            "gives the input back",
            "returns the input",
        ),
        ("tests/send.rs", "would have sent it", "has a line to send"),
        (
            "tests/send.rs",
            "request that would go to another address",
            "request for another address",
        ),
        (
            "tests/workflows.rs",
            "would name a number",
            "names a number",
        ),
        (
            "src/ops/report.rs",
            "after the report went out",
            "after strata sent the report",
        ),
        (
            "xtask/src/contract/mod.rs",
            "goes out once for each choice",
            "once for each choice",
        ),
        (
            "xtask/src/contract/subject.rs",
            "goes out, and so does the `?`",
            "drops the `?` when no pair stays",
        ),
        (
            "xtask/src/contract/tests.rs",
            "the request of the choice `aws` went out",
            "the run sent the request of the choice `aws`",
        ),
        (
            "report-endpoint/src/lib.rs",
            "The values that the limits hold back wait.",
            "The limits make some values wait.",
        ),
        (
            "report-endpoint/src/server.rs",
            "whose values the limits hold back",
            "whose values the limits make wait",
        ),
        (
            "tests/dev_scripts.rs",
            "A `just` that stands in for the recipe",
            "A stand-in `just` for the recipe",
        ),
        (
            "tests/dev_scripts.rs",
            "writes down the setting",
            "records the setting",
        ),
        (
            "xtask/src/facts/cloud/tests.rs",
            "no fact states is written out",
            "no fact states uses the field `route`",
        ),
        (
            "xtask/src/facts/model.rs",
            "states is written out",
            "has its method and path in the field `route`",
        ),
        (
            "preflight/tests/catalog.rs",
            "goes down as the catalog grows. It never goes up.",
            "falls as the catalog grows. It never rises.",
        ),
        ("tests/live_scripts.rs", "passes over", "skips"),
        (
            "xtask/src/facts/subjects.rs",
            "or goes on from it",
            "or continues from it",
        ),
        (
            "xtask/src/facts/subjects/tests.rs",
            "a text that goes on in the middle of a segment",
            "a path that extends the route inside a segment",
        ),
        // S7: the passive clauses that the review named
        (
            "fuzz/src/record.rs",
            "A record that is compared with itself",
            "A record that strata compares with itself",
        ),
        (
            "fuzz/src/record.rs",
            "no pack is added, removed, or changed",
            "both records hold the same packs",
        ),
        (
            "fuzz/src/record.rs",
            "the record that it was read as",
            "the record that the read gives",
        ),
        (
            "fuzz/src/palette.rs",
            "is read by every route that fits it",
            "Every route that fits it reads a document",
        ),
        (
            "src/api/raw.rs",
            "Variable values are omitted.",
            "The result omits variable values.",
        ),
        (
            "src/error/mod.rs",
            "was given something else",
            "got something else",
        ),
        (
            "src/ops/consumers/mod.rs",
            "(whose uid and project are used;",
            "(strata uses its uid",
        ),
        (
            "src/ops/consumers/mod.rs",
            "Each one is read in its own project",
            "Strata reads each one in its own project",
        ),
        (
            "src/term/ask.rs",
            "an answer that cannot be read is a no",
            "an answer that strata cannot read is a no",
        ),
        (
            "tests/ci_scripts.rs",
            "The two lists are written in two files",
            "Two files hold the two lists",
        ),
        (
            "tests/ci_scripts.rs",
            "any comment body it is handed",
            "any comment body that the script gives it",
        ),
        (
            "xtask/src/book.rs",
            "that this run's output was written into",
            "that this run's output goes into",
        ),
        (
            "xtask/src/contract/tests.rs",
            "is skipped before the run needs a subject",
            "The run skips a fact with no document of the stand-in",
        ),
        (
            "xtask/src/facts/cloud.rs",
            "fixture that cannot be read",
            "fixture that the function cannot read",
        ),
        // S7: the "-ing" forms that the review named
        (
            "report-endpoint/tests/book.rs",
            "closing an issue",
            "a closed issue",
        ),
        ("tests/dev_scripts.rs", "Serving it", "A serve of it"),
        (
            "xtask/src/book.rs",
            "Nothing matching is an error",
            "A pattern that matches no line is an error",
        ),
        (
            "xtask/src/book.rs",
            "stops being printed",
            "no longer prints",
        ),
        // S7: the messages of more than 25 words
        (
            "xtask/src/facts/sets.rs",
            "has no such value: add it",
            "has no such value. Add it to the type, or take it from the list.",
        ),
        (
            "xtask/src/facts/sets.rs",
            "lacks it: add it",
            "lacks it. Add it to the list",
        ),
        (
            "preflight/tests/catalog.rs",
            "does not: with a fifth copy",
            "does not. With a fifth copy",
        ),
    ];
    for (file, old, new) in rows {
        assert!(!holds_phrase(file, old), "{file} still holds {old:?}");
        assert!(holds_phrase(file, new), "{file} lacks {new:?}");
    }
}

/// True when `text` starts a numbered item of a help comment: `1. text`, or `1  text`.
fn starts_help_item(text: &str) -> bool {
    let number = text.chars().take_while(char::is_ascii_digit).count();
    list_marker(text) > 0 || (number > 0 && text[number..].starts_with("  "))
}

/// The paragraphs of the comment at the top of a script, from its second line to the line `set -`.
/// The help of the script prints that comment. A blank comment line ends a paragraph, and so does
/// the start of a numbered item.
fn help_paragraphs(script: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current = String::new();
    for line in repo_file(script).lines().skip(1) {
        let Some(text) = line.strip_prefix('#') else {
            break;
        };
        let text = text.trim();
        if (text.is_empty() || starts_help_item(text)) && !current.is_empty() {
            paragraphs.push(std::mem::take(&mut current));
        }
        if !text.is_empty() {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(text);
        }
    }
    paragraphs.push(current);
    paragraphs
}

/// S7: a paragraph of the comment at the top of a script has six sentences at most. The scripts
/// below print that comment as their help, so a user reads each paragraph.
#[test]
fn each_paragraph_of_the_help_comment_of_the_scripts_that_the_review_named_has_six_sentences_or_fewer()
 {
    let mut faults = Vec::new();
    for script in [
        "deploy/contract/weekly.sh",
        "examples/live/contract-report.sh",
        "examples/live/contract.sh",
        "examples/live/preflight-plan.sh",
        "examples/live/report-send.sh",
        "scripts/ci/merge-gate.sh",
    ] {
        for paragraph in help_paragraphs(script) {
            let found = sentences(&paragraph).len();
            if found > MOST_SENTENCES {
                faults.push(format!("{script}: {found} sentences: {paragraph}"));
            }
        }
    }
    assert_no_faults(&faults);
}

/// S8: the walkthrough of the preflight quotes no line of the output of strata that `just
/// docs-check` does not read. The last line of the output names the two failed checks, and the
/// page says so with no quote.
#[test]
fn the_walkthrough_of_the_preflight_quotes_no_line_of_the_report_that_the_book_does_not_include() {
    let output = read(&docs().join("output/preflight-strict.txt"));
    assert_eq!(
        output.lines().last(),
        Some("FAIL: 2 checks failed: worker-cpu-floor, palette-validate")
    );
    let page = one_line(&read(&docs().join("development/preflight-walkthrough.md")));
    for old in [
        "`FAIL: 2 checks failed`",
        "`ran: POST /v1/spectroclusters/aws/validate -> HTTP 200`",
    ] {
        assert!(!page.contains(old), "the walkthrough still quotes {old}");
    }
    assert!(page.contains("The last line names the two failed checks"));
}

/// The outputs of the book do not name the folder of the run that wrote them: the runner writes
/// `~` for its home folder. A name with the number of a process changes at each run.
#[test]
fn no_output_of_the_book_names_the_folder_of_the_runner() {
    let outputs = docs().join("output");
    let mut found = Vec::new();
    for file in walk(&outputs) {
        if read(&file).contains("strata-book-") {
            found.push(file.display().to_string());
        }
    }
    assert!(found.is_empty(), "the folder of the runner is in {found:?}");
    let report = read(&outputs.join("unknown-value-report-invalid.txt"));
    assert!(report.contains("strata: ~/.local/state/strata/strata-unknown.jsonl: line 1 is not"));
}

/// S9: a page includes the output of each step that the wave added to what a user sees. A row has
/// the page, the output, and the words that the output must hold. Each output is a step of a
/// chapter of the runner.
#[test]
fn each_step_that_the_wave_added_to_what_a_user_sees_has_an_output_in_the_book() {
    let rows = [
        // a pre-release above a release: the two marks are on two versions
        (
            "commands/packs.md",
            "packs-pre-release",
            "latest in Palette",
        ),
        (
            "commands/packs.md",
            "packs-pre-release-json",
            "\"version\":\"1.22.0-rc.1\",\"latest\":false,\"paletteLatest\":true",
        ),
        (
            "commands/packs.md",
            "packs-pre-release-json",
            "\"version\":\"1.21.1\",\"latest\":true,\"paletteLatest\":null",
        ),
        // the values of a group with no canonical form
        (
            "commands/snapshot/other-objects.md",
            "group-template-values",
            "{\"values\":null,\"valuesHash\":\"",
        ),
        (
            "commands/snapshot/other-objects.md",
            "group-template-verify",
            "is unchanged since",
        ),
        // the error of a field inside the cluster of a record
        (
            "reference/errors.md",
            "tiers-edited-verify",
            "source.cluster.profiles[0].version: invalid type: null, expected a string",
        ),
        // the error of a line of the log that a person edited
        (
            "commands/report.md",
            "unknown-value-report-invalid",
            "line 1 is not a line of strata-unknown/1",
        ),
        // a rule that runs two commands, and the second one fails
        (
            "reference/preflight-rules.md",
            "preflight-second-command",
            "-> exit 0; exit 254",
        ),
        // the three ends of a watch
        (
            "commands/preflight/watch.md",
            "preflight-watch-error",
            "OK: each counted check passed",
        ),
        (
            "commands/preflight/watch.md",
            "preflight-watch-is-deleted",
            "OK: the cluster is gone; no counted check failed",
        ),
        (
            "commands/preflight/watch.md",
            "preflight-watch-gone",
            "OK: the cluster is gone; no counted check failed",
        ),
        // the gate with a strata that is older than the script
        (
            "automation/merge-gate.md",
            "merge-gate-older-strata",
            "merge-gate: strata is older than this script: it writes no versionMove",
        ),
    ];
    for (page, output, words) in rows {
        let text = read(&docs().join("output").join(format!("{output}.txt")));
        assert!(text.contains(words), "{output}.txt lacks {words:?}: {text}");
        let include = format!("{{{{#include ../output/{output}.txt}}}}");
        let nested = include.replace("../output", "../../output");
        let source = read(&docs().join(page));
        assert!(
            source.contains(&include) || source.contains(&nested),
            "{page} does not include {output}.txt"
        );
    }
}

/// S9: the hash of the values of a group with no canonical form is the hash of the masked text,
/// as the page says. The page shows the record that the runner took of such a group.
#[test]
fn the_hash_that_the_book_shows_for_values_with_no_canonical_form_covers_the_masked_text() {
    use sha2::{Digest, Sha256};
    let text = read(&docs().join("output/group-template-values.txt"));
    let masked = "database:\n  password: '********'\nhost: {{ .spectro.var.host }}";
    let hash = hex::encode(Sha256::digest(masked.as_bytes()));
    assert!(
        text.contains(&format!("\"valuesHash\":\"{hash}\"")),
        "{text}"
    );
    assert!(!text.contains("s3cr3t"));
}

/// S9: the mark of Palette follows the highest version of a pack in the stand-in, as the page of
/// the packs says. The version `1.22.0-rc.1` is above `1.21.1`, so the output marks `1.22.0-rc.1`
/// as the latest of Palette, and `1.21.1` as the latest of strata.
#[test]
fn the_page_of_the_packs_gives_the_pre_release_above_the_release_as_the_case() {
    let page = one_line(&read(&docs().join("commands/packs.md")));
    for needle in [
        "The highest version is a pre-release, such as `1.22.0-rc.1` above `1.21.1`: `@latest` skips it, and Palette marks the highest version.",
        "The JSON gives `latest` on the version that strata picks, and `paletteLatest` on the version that Palette marks.",
    ] {
        assert!(page.contains(needle), "packs.md lacks {needle:?}");
    }
    let example = read(&docs().join("output/packs-pre-release.txt"));
    let lines: Vec<&str> = example.lines().collect();
    assert!(
        lines[2].trim_start().starts_with("1.22.0-rc.1 ")
            && lines[2].ends_with("latest in Palette")
    );
    assert!(lines[3].trim_start().starts_with("1.21.1 ") && !lines[3].contains("Palette"));
    let live = repo_file("examples/live/packs.sh");
    assert!(
        !live.contains("so the newest version of a pack is"),
        "packs.sh says what the output refutes"
    );
}

/// S9: the table of the colors gives the color of each cluster state, as `paint_state` paints it.
#[test]
fn the_table_of_colors_gives_the_color_of_each_cluster_state() {
    use strata::closed_set::ClosedSet;
    use strata::model::cluster_state::ClusterState;
    use strata::term::color::{ColorMode, Paint, Stream};
    let on = Paint::resolve(ColorMode::Always, Stream::Stdout);
    let page = read(&docs().join("reference/formats.md"));
    let table = section(&page, "### Colors", Some("Strata decides the color"));
    let row_of = |color: &str| {
        table
            .lines()
            .find(|l| l.starts_with(&format!("| {color} |")))
            .unwrap_or_else(|| panic!("the table has no row for {color}"))
    };
    let states = ClusterState::ALL
        .iter()
        .map(|s| s.as_str())
        .chain([strata::ops::UNKNOWN_STATE]);
    for state in states {
        let painted = strata::render::paint_state(&on, state);
        let color = [
            ("green", on.green(state)),
            ("yellow", on.yellow(state)),
            ("red", on.red(state)),
        ]
        .into_iter()
        .find(|(_, text)| *text == painted)
        .map(|(name, _)| name)
        .unwrap_or_else(|| panic!("{state} has no color"));
        assert!(
            row_of(color).contains(&format!("`{state}`")),
            "the row {color} lacks the state {state}"
        );
    }
    assert!(
        one_line(table).contains("A state that strata does not know has no color."),
        "the table lacks the state with no color"
    );
}

/// S9: the comment of the recipe `live-contract` names each option of the contract run that the
/// help of the run lists. `just --list` prints the last comment line before a recipe.
#[test]
fn the_comment_of_the_recipe_live_contract_names_each_option_of_the_run() {
    let justfile = repo_file("justfile");
    let comment = justfile
        .lines()
        .take_while(|line| !line.starts_with("live-contract "))
        .last()
        .expect("the recipe live-contract has a comment");
    for option in ["--no-cloud", "--with-cluster", "--state"] {
        assert!(
            comment.contains(option),
            "the comment lacks {option}: {comment}"
        );
    }
    assert!(
        comment.contains("--with-cluster holds the floor"),
        "{comment}"
    );
}

/// S10: `CONTRIBUTING.md` states the rule of the commit messages that the repository keeps: one
/// line, no prefix, and the release commit as the one exception. `just version` writes that commit.
#[test]
fn contributing_asks_for_a_commit_message_with_no_prefix_and_names_the_release_commit() {
    let page = one_line(&repo_file("CONTRIBUTING.md"));
    for needle in [
        "One line in Simplified Technical English (ASD-STE100), with no prefix such as `feat:` or `fix:`.",
        "The squash merge takes the title of the pull request as the subject, so the title follows the same rule.",
        "The release commit `chore: release X.Y.Z`, which `just version` writes, is the one exception.",
    ] {
        assert!(page.contains(needle), "CONTRIBUTING.md lacks {needle:?}");
    }
    assert!(!page.contains("Conventional Commits"));
    assert!(
        repo_file("scripts/dev/set-version.sh")
            .contains("git commit -q -m \"chore: release $new\"")
    );
}
