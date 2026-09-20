//! The rules the book must keep, which no other check can see.
//!
//! A rule lands here when a walk of the pages finds the same defect twice.
//! `docs/examples/checklist.md` is the procedure; this is what stops the drift.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use regex::Regex;

/// Every rule. Returns the messages, so the caller decides what to do with them.
pub fn check(repo: &Path) -> Vec<String> {
    let mut bad = Vec::new();
    bad.extend(release_tags(repo));
    bad.extend(links_resolve(repo));
    bad.extend(follow_along_first(repo));
    bad
}

/// Every markdown page of the book.
fn pages(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            out.extend(pages(&path));
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
    out
}

fn shown(repo: &Path, page: &Path) -> String {
    page.strip_prefix(repo)
        .unwrap_or(page)
        .display()
        .to_string()
}

/// A release tag that a reader copies -- an install command, or a workflow ref -- must be this
/// release. A tag from the release before sends them to the wrong binary, and nothing else
/// notices.
///
/// Two files keep older tags on purpose, and are not read here: `development/live-runs.md`
/// records what each live run used, and `development/release.md` states that rule. Prose that
/// names an older release for what it first carried is a fact about that release, so only the
/// shapes below count.
fn release_tags(repo: &Path) -> Vec<String> {
    let manifest = std::fs::read_to_string(repo.join("Cargo.toml")).unwrap_or_default();
    let version = manifest
        .lines()
        .find_map(|l| l.strip_prefix("version = "))
        .map(|v| format!("v{}", v.trim_matches(['"', ' '])))
        .unwrap_or_default();
    let copyable = Regex::new(
        r"(?:--version|strata-version:|STRATA_VERSION:|v=)\s*(v\d+\.\d+\.\d+)|uses:.*@(v\d+\.\d+\.\d+)",
    )
    .expect("a pattern");
    let mut bad = Vec::new();
    for page in pages(&repo.join("docs/src")) {
        let name = shown(repo, &page);
        if name.ends_with("development/live-runs.md") || name.ends_with("development/release.md") {
            continue;
        }
        let text = std::fs::read_to_string(&page).unwrap_or_default();
        for (number, line) in text.lines().enumerate() {
            for found in copyable.captures_iter(line) {
                let tag = found
                    .get(1)
                    .or_else(|| found.get(2))
                    .map(|m| m.as_str())
                    .unwrap_or_default();
                if tag != version {
                    bad.push(format!(
                        "{name}:{} names the release {tag}, and this crate is {version}; \
                         a reader copies this line",
                        number + 1
                    ));
                }
            }
        }
    }
    bad
}

/// A link into another page names a heading of that page. A heading that is renamed leaves every
/// link to it pointing at nothing, and the built book gives no warning.
fn links_resolve(repo: &Path) -> Vec<String> {
    let link = Regex::new(r"\]\(([^)\s#]+\.md)#([a-z0-9-]+)\)").expect("a pattern");
    let mut bad = Vec::new();
    for page in pages(&repo.join("docs/src")) {
        let text = std::fs::read_to_string(&page).unwrap_or_default();
        let here = page.parent().expect("a page sits in a directory");
        for found in link.captures_iter(&text) {
            let (target, anchor) = (&found[1], &found[2]);
            let path = here.join(target);
            let Ok(path) = path.canonicalize() else {
                bad.push(format!(
                    "{} links to {target}, which is not a page",
                    shown(repo, &page)
                ));
                continue;
            };
            let headings = anchors(&std::fs::read_to_string(&path).unwrap_or_default());
            if !headings.contains(anchor) {
                bad.push(format!(
                    "{} links to {target}#{anchor}, and that page has no such heading",
                    shown(repo, &page)
                ));
            }
        }
    }
    bad
}

/// The anchor mdBook makes for each heading: lower case, spaces to dashes, and the punctuation
/// that is not a dash dropped.
fn anchors(text: &str) -> BTreeSet<String> {
    text.lines()
        .filter_map(|l| {
            l.trim_start_matches('#')
                .strip_prefix(' ')
                .filter(|_| l.starts_with('#'))
        })
        .map(|heading| {
            heading
                .trim()
                .to_lowercase()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-')
                .map(|c| if c == ' ' { '-' } else { c })
                .collect()
        })
        .collect()
}

/// A reader follows a use case from the top. The line that says how to serve the tenant must come
/// before the first command, or the first thing the page asks for is a server it has not named.
fn follow_along_first(repo: &Path) -> Vec<String> {
    let mut bad = Vec::new();
    for page in pages(&repo.join("docs/src/use-cases")) {
        let text = std::fs::read_to_string(&page).unwrap_or_default();
        let name = shown(repo, &page);
        let Some(block) = text.find("{{#include ../output/") else {
            continue;
        };
        match text.find("**Follow along.**") {
            None => bad.push(format!(
                "{name} has output blocks and no '**Follow along.**' line: a reader cannot run it"
            )),
            Some(follow) if follow > block => bad.push(format!(
                "{name} asks for a command before its '**Follow along.**' line"
            )),
            Some(_) => {}
        }
    }
    bad
}
