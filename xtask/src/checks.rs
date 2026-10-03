//! The rules the book must keep, which no other check can see.
//!
//! A rule lands here when a walk of the pages finds the same defect twice.
//! `docs/examples/checklist.md` is the procedure; this is what stops the drift.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use regex::Regex;

/// Every rule. Returns the messages, so the caller decides what to do with them.
///
/// `chapters` maps the name of each chapter that the runner defined to its number of states.
pub fn check(repo: &Path, chapters: &BTreeMap<String, u32>) -> Vec<String> {
    let mut bad = Vec::new();
    bad.extend(release_tags(repo));
    bad.extend(links_resolve(repo));
    bad.extend(follow_along_first(repo));
    bad.extend(command_flags(repo));
    bad.extend(playgrounds(repo, chapters));
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

/// A command page shows the `--help` of its command, which `just docs-examples` writes, so the
/// options on the page are those of the binary. Its text also names each option of that help, so
/// a new option gets a sentence and not only a line in the help block. `--help` needs none.
///
/// A command page is a file of `docs/src/commands/` itself. A page under `commands/<command>/`
/// is a part of a long command page, split by what the command reads: it shows no help, and
/// the command page names each option in its own table, so the check reads the command page
/// alone.
fn command_flags(repo: &Path) -> Vec<String> {
    let mut bad = Vec::new();
    let flag = Regex::new(r"--[a-z][a-z0-9-]*").expect("the flag pattern");
    let dir = repo.join("docs/src/commands");
    let command_pages = pages(&dir)
        .into_iter()
        .filter(|page| page.parent() == Some(dir.as_path()));
    for page in command_pages {
        let Some(command) = page.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if command == "index" {
            continue;
        }
        let name = shown(repo, &page);
        let text = std::fs::read_to_string(&page).unwrap_or_default();
        let include = format!("{{{{#include ../output/help-{command}.txt}}}}");
        if !text.contains(&include) {
            bad.push(format!(
                "{name} does not include the --help of `strata {command}`: {include}"
            ));
            continue;
        }
        let help =
            std::fs::read_to_string(repo.join(format!("docs/src/output/help-{command}.txt")))
                .unwrap_or_default();
        // the text of the page, without the help block itself
        let prose: String = text
            .lines()
            .filter(|l| !l.contains(&include))
            .collect::<Vec<_>>()
            .join("\n");
        let named = |f: &str| {
            prose.match_indices(f).any(|(i, _)| {
                !prose[i + f.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-')
            })
        };
        let mut flags: Vec<&str> = flag.find_iter(&help).map(|m| m.as_str()).collect();
        flags.sort_unstable();
        flags.dedup();
        for f in flags {
            if f != "--help" && !named(f) {
                bad.push(format!(
                    "{name} does not name {f}, an option of `strata {command} --help`"
                ));
            }
        }
    }
    bad
}

/// Each `just playground NAME` of the book and the README names a chapter of the runner, and a
/// state that the chapter has. A reader who copies a wrong name gets an error and no tenant.
fn playgrounds(repo: &Path, chapters: &BTreeMap<String, u32>) -> Vec<String> {
    let mut files = pages(&repo.join("docs/src"));
    files.push(repo.join("README.md"));
    files
        .iter()
        .flat_map(|page| {
            let text = std::fs::read_to_string(page).unwrap_or_default();
            playground_faults(&shown(repo, page), &text, chapters)
        })
        .collect()
}

/// The messages for each `just playground NAME` in `text` that the runner cannot serve. `name`
/// is the page, for the messages.
fn playground_faults(name: &str, text: &str, chapters: &BTreeMap<String, u32>) -> Vec<String> {
    let command = Regex::new(r"just playground ([a-z0-9][a-z0-9-]*)").expect("a pattern");
    let mut bad = Vec::new();
    for (number, line) in text.lines().enumerate() {
        for found in command.captures_iter(line) {
            let given = &found[1];
            let (chapter, seed) = crate::book::playground(given);
            match chapters.get(chapter) {
                None => bad.push(format!(
                    "{name}:{} gives `just playground {given}`, and the runner has no chapter \
                     '{chapter}'",
                    number + 1
                )),
                Some(&states) if seed == 0 || seed > states => bad.push(format!(
                    "{name}:{} gives `just playground {given}`, and the chapter '{chapter}' has \
                     {states} states",
                    number + 1
                )),
                Some(_) => {}
            }
        }
    }
    bad
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chapters of a small runner: `list` with one state, and `loop` with four.
    fn chapters() -> BTreeMap<String, u32> {
        BTreeMap::from([("list".to_string(), 1), ("loop".to_string(), 4)])
    }

    #[test]
    fn a_page_that_names_a_chapter_and_a_state_that_it_has_passes() {
        let text = "Run `just playground list`, then `just playground loop-4`.\n\
                    just playground          # the tenant of the chapter \"list\"\n\
                    just playground CHAPTER\n";
        assert_eq!(
            playground_faults("page.md", text, &chapters()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_page_that_names_a_chapter_that_the_runner_does_not_define_fails() {
        let text = "Serve it.\n\nRun `just playground os-image-1`.\n";
        assert_eq!(
            playground_faults("page.md", text, &chapters()),
            vec![
                "page.md:3 gives `just playground os-image-1`, and the runner has no chapter \
                 'os-image'"
            ]
        );
    }

    #[test]
    fn a_page_that_names_a_state_past_the_last_state_of_its_chapter_fails() {
        let text = "just playground loop-5\njust playground loop-0\n";
        assert_eq!(
            playground_faults("page.md", text, &chapters()),
            vec![
                "page.md:1 gives `just playground loop-5`, and the chapter 'loop' has 4 states",
                "page.md:2 gives `just playground loop-0`, and the chapter 'loop' has 4 states",
            ]
        );
    }
}
