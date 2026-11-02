//! What the list is compared with: the book, the live scripts, the rules of strata-preflight,
//! and the files of strata. A check reads these values, so a test can give it any of them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::model::{DIRECTORY, Fact, load};
use crate::checks::anchors;
use crate::files::{book, files_with_ext, pages};

/// The page that lists the requests of strata, under `docs/src/`.
pub const API_PAGE: &str = "reference/palette-api.md";

/// The directory of the pages that state the facts of Palette, under `docs/src/`.
pub const FACT_PAGES: &str = "internals/palette";

/// The directory of the live scripts, from the root of the repository.
const LIVE: &str = "examples/live";

/// The directory of the rules of strata-preflight, from the root of the repository.
const RULES: &str = "preflight/rules";

/// The page and the heading that list the checks of a script, when it is not the section
/// `## <script>` of `development/live-runs.md`.
const CHECK_TABLES: &[(&str, &str, &str)] = &[(
    "verify.sh",
    "development/live-example.md",
    "## The checks of verify.sh",
)];

/// The page that lists the checks of the scripts, and the heading of a script in it.
const CHECKS_PAGE: &str = "development/live-runs.md";

/// Everything that the checks of the list compare with, and the list itself.
#[derive(Debug, Default)]
pub struct Sources {
    /// The facts, in the order of the files.
    pub facts: Vec<Fact>,
    /// The routes that the stand-in serves, when the caller has a run of the book to give.
    pub served: Option<BTreeSet<String>>,
    /// The shape of each request that a table of `reference/palette-api.md` lists.
    pub requests: BTreeSet<String>,
    /// The text of each page of the book, by the path under `docs/src/`.
    pub texts: BTreeMap<String, String>,
    /// The anchor of each heading of each page, by the path under `docs/src/`.
    pub anchors: BTreeMap<String, BTreeSet<String>>,
    /// The `##` headings of the pages in `internals/palette/`, each as `page#anchor`.
    pub sections: BTreeSet<String>,
    /// The name of each check that a live script calls, by the name of the script.
    pub checks: BTreeMap<String, BTreeSet<String>>,
    /// The name of each check that has a row in the book, by the name of the script.
    pub rows: BTreeMap<String, BTreeSet<String>>,
    /// The files of strata that exist, from the root of the repository.
    pub files: BTreeSet<String>,
    /// The id of each rule of strata-preflight.
    pub rule_ids: BTreeSet<String>,
    /// The rules that send each request of Palette, by the shape of the request.
    pub rule_requests: BTreeMap<String, BTreeSet<String>>,
}

impl Sources {
    /// Read the list and what it is compared with, in the repository at `repo`. The messages are
    /// about the files of the list that cannot be read.
    pub fn read(repo: &Path, served: Option<BTreeSet<String>>) -> (Sources, Vec<String>) {
        let (facts, bad) = load(&repo.join(DIRECTORY));
        let docs = book(repo);
        let mut sources = Sources {
            served,
            requests: api_requests(&read_text(&docs.join(API_PAGE))),
            ..Sources::default()
        };
        for page in pages(&docs) {
            let Ok(relative) = page.strip_prefix(&docs) else {
                continue;
            };
            let relative = relative.to_string_lossy().into_owned();
            let text = read_text(&page);
            if relative.starts_with(&format!("{FACT_PAGES}/")) {
                sources.sections.extend(
                    section_headings(&text)
                        .into_iter()
                        .map(|a| format!("{relative}#{a}")),
                );
            }
            sources.anchors.insert(relative.clone(), anchors(&text));
            sources.texts.insert(relative, text);
        }
        sources.checks = live_checks(repo);
        sources.rows = book_rows(&docs);
        sources.files = facts
            .iter()
            .flat_map(|f| &f.code)
            .filter(|path| repo.join(path).is_file())
            .cloned()
            .collect();
        (sources.rule_ids, sources.rule_requests) = rule_requests(repo);
        sources.facts = facts;
        (sources, bad)
    }
}

/// The text of a file, or an empty text for a file that cannot be read. A check reports the
/// missing thing that the text would have held.
fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// The shape of a request: the method, and the path with each placeholder as `{}` and with no
/// query. The book writes `{uid}` and `<name>`, the rules of preflight write `{{ .expr }}`, and
/// the shape is the same for all three.
pub fn shape(request: &str) -> String {
    static PLACEHOLDER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\{\{.*?\}\}|\{[^}]*\}|<[^>]*>").expect("a pattern"));
    let (method, path) = request.split_once(' ').unwrap_or(("", request));
    let path = path.split('?').next().unwrap_or(path);
    format!("{method} {}", PLACEHOLDER.replace_all(path, "{}"))
}

/// The shape of each request in the first column of the tables of `page`: a code span that
/// starts with `GET ` or `POST `.
pub(super) fn api_requests(page: &str) -> BTreeSet<String> {
    let span = Regex::new(r"`((?:GET|POST) /[^`]*)`").expect("a pattern");
    page.lines()
        .filter(|line| line.starts_with('|'))
        .filter_map(|line| line.trim_start_matches('|').split('|').next())
        .flat_map(|cell| {
            span.captures_iter(cell)
                .map(|c| shape(&c[1]))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The anchor of each `##` heading of `page`.
pub(super) fn section_headings(page: &str) -> BTreeSet<String> {
    let level_two: String = page
        .lines()
        .filter(|line| line.starts_with("## "))
        .map(|line| format!("{line}\n"))
        .collect();
    anchors(&level_two)
}

/// The checks that a script calls: the first word after `check` at the start of a line.
pub(super) fn calls_of(script: &str) -> BTreeSet<String> {
    let call = Regex::new(r#"(?m)^\s*check\s+"?([A-Za-z0-9_$][A-Za-z0-9_.${}-]*)"?\s"#)
        .expect("a pattern");
    call.captures_iter(script)
        .map(|c| c[1].to_string())
        .collect()
}

/// The checks that each script of `examples/live/` calls.
fn live_checks(repo: &Path) -> BTreeMap<String, BTreeSet<String>> {
    files_with_ext(&repo.join(LIVE), "sh")
        .into_iter()
        .filter_map(|script| {
            let name = script.file_name()?.to_string_lossy().into_owned();
            Some((name, calls_of(&read_text(&script))))
        })
        .collect()
}

/// The check names in the first column of the tables of the section `heading` of `page`, up to
/// the next `##` heading.
pub(super) fn section_rows(page: &str, heading: &str) -> BTreeSet<String> {
    let name = Regex::new(r"`([^`]+)`").expect("a pattern");
    let mut inside = false;
    let mut rows = BTreeSet::new();
    for line in page.lines() {
        if line.starts_with("## ") {
            inside = line.trim_end() == heading;
        } else if inside && line.starts_with('|') {
            let cell = line
                .trim_start_matches('|')
                .split('|')
                .next()
                .unwrap_or_default();
            rows.extend(name.captures_iter(cell).map(|c| c[1].to_string()));
        }
    }
    rows
}

/// The rows of the book for each script: the section of the script in `development/live-runs.md`,
/// or the table that [`CHECK_TABLES`] names.
pub(super) fn book_rows(docs: &Path) -> BTreeMap<String, BTreeSet<String>> {
    let runs = read_text(&docs.join(CHECKS_PAGE));
    let script = Regex::new(r"(?m)^## (\S+\.sh)\s*$").expect("a pattern");
    let mut rows: BTreeMap<String, BTreeSet<String>> = script
        .captures_iter(&runs)
        .map(|c| {
            (
                c[1].to_string(),
                section_rows(&runs, &format!("## {}", &c[1])),
            )
        })
        .collect();
    for (name, page, heading) in CHECK_TABLES {
        rows.insert(
            (*name).to_string(),
            section_rows(&read_text(&docs.join(page)), heading),
        );
    }
    rows
}

/// The id of each rule of strata-preflight, and the rules that send each request of Palette, by
/// the shape of the request. A rule sends one when its check is of the kind `palette`.
pub(super) fn rule_requests(repo: &Path) -> (BTreeSet<String>, BTreeMap<String, BTreeSet<String>>) {
    let mut ids = BTreeSet::new();
    let mut requests: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for file in files_with_ext(&repo.join(RULES), "yaml") {
        let Ok(section) = serde_yaml::from_str::<serde_yaml::Value>(&read_text(&file)) else {
            continue;
        };
        for rule in section["rules"].as_sequence().into_iter().flatten() {
            let Some(id) = rule["id"].as_str() else {
                continue;
            };
            ids.insert(id.to_string());
            let check = &rule["check"];
            if check["kind"].as_str() == Some("palette")
                && let Some(path) = check["path"].as_str()
            {
                let method = check["method"].as_str().unwrap_or("GET").to_uppercase();
                requests
                    .entry(shape(&format!("{method} {path}")))
                    .or_default()
                    .insert(id.to_string());
            }
        }
    }
    (ids, requests)
}
