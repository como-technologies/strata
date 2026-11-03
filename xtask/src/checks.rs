//! The rules the book must keep, which no other check can see.
//!
//! A rule lands here when a walk of the pages finds the same defect twice.
//! `docs/examples/checklist.md` is the procedure; this is what stops the drift.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use regex::Regex;

use crate::contract::StandIn;
use crate::files::{book, output_dir, pages};

/// The use cases of the book, from the root of the repository.
const USE_CASES: &str = "use-cases";

/// An output that a step wrote, with the chapter that wrote it and the state of that chapter then.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OutputState {
    /// The name of the output file, without `.txt`.
    pub output: String,
    pub chapter: String,
    /// The state that the output belongs to: `just playground CHAPTER-N` serves state N.
    pub state: u32,
}

/// Every rule. Returns the messages, so the caller decides what to do with them.
///
/// `chapters` maps the name of each chapter that the runner defined to its number of states.
/// `states` names the chapter and the state of each output that the runner wrote. `routes` holds
/// the route of each document that the stand-in served, and `stand_in` holds the documents.
pub fn check(
    repo: &Path,
    chapters: &BTreeMap<String, u32>,
    states: &BTreeSet<OutputState>,
    routes: &BTreeSet<String>,
    stand_in: &StandIn,
) -> Vec<String> {
    let mut bad = Vec::new();
    bad.extend(release_tags(repo));
    bad.extend(links_resolve(repo));
    bad.extend(follow_along_first(repo));
    bad.extend(command_flags(repo));
    bad.extend(playgrounds(repo, chapters));
    bad.extend(seeds(repo, states));
    bad.extend(cited_run_ids(repo));
    bad.extend(crate::facts::check(repo, Some(routes)));
    bad.extend(fields_held(repo, stand_in));
    bad
}

/// Each field that a GET fact reads is in a document of the stand-in. The contract run compares
/// the type of the field in the answer of a real tenant with the types in those documents. A
/// field that no document holds has no type to compare, and the run then reports a difference
/// for each tenant that gives the field.
fn fields_held(repo: &Path, stand_in: &StandIn) -> Vec<String> {
    // the check of the facts reports a list that strata cannot read
    crate::facts::load(repo)
        .map(|facts| stand_in.unheld_fields(&facts))
        .unwrap_or_default()
}

fn shown(repo: &Path, page: &Path) -> String {
    page.strip_prefix(repo)
        .unwrap_or(page)
        .display()
        .to_string()
}

/// The text of the file at `path`, or None and a message in `bad` that names the file. A page that
/// a check cannot read must not pass.
fn read(repo: &Path, path: &Path, bad: &mut Vec<String>) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) => {
            bad.push(format!("{}: cannot read the file: {e}", shown(repo, path)));
            None
        }
    }
}

/// A release tag that a reader copies -- an install command, a workflow ref, or the `tag` of a
/// git dependency -- must be this release. A tag from the release before sends them to the wrong
/// binary, and nothing else notices.
///
/// Two files keep older tags on purpose, and are not read here: `development/live-runs.md`
/// records what each live run used, and `development/release.md` states that rule. Prose that
/// names an older release for what it first carried is a fact about that release, so only the
/// shapes below count.
fn release_tags(repo: &Path) -> Vec<String> {
    let mut bad = Vec::new();
    let Some(manifest) = read(repo, &repo.join("Cargo.toml"), &mut bad) else {
        return bad;
    };
    let Some(version) = manifest
        .lines()
        .find_map(|l| l.strip_prefix("version = "))
        .map(|v| format!("v{}", v.trim_matches(['"', ' '])))
    else {
        return vec!["Cargo.toml has no version line".to_string()];
    };
    let copyable = Regex::new(
        r#"(?:--version|strata-version:|STRATA_VERSION:|v=)\s*(v\d+\.\d+\.\d+)|uses:.*@(v\d+\.\d+\.\d+)|\btag = "(v\d+\.\d+\.\d+)""#,
    )
    .expect("a pattern");
    for page in pages(&book(repo)) {
        let name = shown(repo, &page);
        if name.ends_with("development/live-runs.md") || name.ends_with("development/release.md") {
            continue;
        }
        let Some(text) = read(repo, &page, &mut bad) else {
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            for found in copyable.captures_iter(line) {
                // each match fills one of the three groups, so the empty tag does not occur
                let tag = found
                    .iter()
                    .skip(1)
                    .flatten()
                    .next()
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
    for page in pages(&book(repo)) {
        let Some(text) = read(repo, &page, &mut bad) else {
            continue;
        };
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
            let Some(target_text) = read(repo, &path, &mut bad) else {
                continue;
            };
            let headings = anchors(&target_text);
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
pub(crate) fn anchors(text: &str) -> BTreeSet<String> {
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
    for page in pages(&book(repo).join(USE_CASES)) {
        let Some(text) = read(repo, &page, &mut bad) else {
            continue;
        };
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
    let dir = book(repo).join("commands");
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
        let Some(text) = read(repo, &page, &mut bad) else {
            continue;
        };
        let include = format!("{{{{#include ../output/help-{command}.txt}}}}");
        if !text.contains(&include) {
            bad.push(format!(
                "{name} does not include the --help of `strata {command}`: {include}"
            ));
            continue;
        }
        let help_file = output_dir(repo).join(format!("help-{command}.txt"));
        let Some(help) = read(repo, &help_file, &mut bad) else {
            continue;
        };
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
    let mut files = pages(&book(repo));
    files.push(repo.join("README.md"));
    let mut bad = Vec::new();
    for page in &files {
        if let Some(text) = read(repo, page, &mut bad) {
            bad.extend(playground_faults(&shown(repo, page), &text, chapters));
        }
    }
    bad
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

/// The pattern of an include of an output in a page. Its group is the name of the output.
fn include_pattern() -> Regex {
    Regex::new(r"\{\{#include \.\./output/([a-z0-9-]+)\.txt").expect("a pattern")
}

/// A page that includes an output from past the first `serve_here` of its chapter must name the
/// seed that reaches it. `just playground CHAPTER` serves state 1, so a page that shows a later
/// state and says only `CHAPTER` sends the reader to a tenant where its blocks cannot run.
fn seeds(repo: &Path, states: &BTreeSet<OutputState>) -> Vec<String> {
    let include = include_pattern();
    let mut bad = Vec::new();
    for page in pages(&book(repo).join(USE_CASES)) {
        if let Some(text) = read(repo, &page, &mut bad) {
            bad.extend(seed_faults(&shown(repo, &page), &text, &include, states));
        }
    }
    bad
}

/// The messages for each output in `text` that a later state of its chapter reaches, and that
/// `text` gives no seed for. `name` is the page, for the messages, and `include` finds the outputs.
fn seed_faults(
    name: &str,
    text: &str,
    include: &Regex,
    states: &BTreeSet<OutputState>,
) -> Vec<String> {
    let mut wanted: Vec<(&str, u32)> = include
        .captures_iter(text)
        .flat_map(|found| {
            let output = found.get(1).map_or("", |m| m.as_str());
            states
                .iter()
                .filter(move |s| s.output == output)
                .map(|s| (s.chapter.as_str(), s.state))
        })
        .filter(|(_, state)| *state > 1)
        .collect();
    wanted.sort();
    wanted.dedup();
    wanted
        .into_iter()
        .filter_map(|(chapter, state)| {
            let seed = format!("{chapter}-{state}");
            (!text.contains(&seed)).then(|| {
                format!(
                    "{name} shows state {state} of the chapter '{chapter}' and never names the \
                     seed '{seed}'; a reader on 'just playground {chapter}' cannot run it"
                )
            })
        })
        .collect()
}

/// A live result is a fact, with no run id and no date. A reader cannot find a run id, and a date
/// tells them nothing about what strata does now.
fn cited_run_ids(repo: &Path) -> Vec<String> {
    let mut bad = Vec::new();
    for page in pages(&book(repo)) {
        if let Some(text) = read(repo, &page, &mut bad) {
            let name = shown(repo, &page);
            bad.extend(run_ids(&text).into_iter().map(|id| {
                format!(
                    "{name} cites the run id {id}. State what the run established, in the \
                     present tense, and put the check in development/live-runs.md"
                )
            }));
        }
    }
    bad
}

/// Every run id in a text: eight digits for the date, `T`, six digits for the time, and `Z`.
fn run_ids(text: &str) -> Vec<String> {
    let id = Regex::new(r"\d{8}T\d{6}Z").expect("a pattern");
    id.find_iter(text).map(|m| m.as_str().to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_check_of_the_book_reports_a_field_that_no_document_of_the_stand_in_holds() {
        // a cluster listing with no entry holds none of the fields that the fact reads
        let temp = tempfile::tempdir().unwrap();
        let tenant = temp.path();
        let listing = tenant.join("tenant/v1/dashboard/spectroclusters");
        std::fs::create_dir_all(&listing).unwrap();
        std::fs::write(listing.join("meta.json"), "[]").unwrap();
        let mut stand_in = StandIn::default();
        stand_in.add_tenant(tenant);
        let repo = crate::repository();
        let bad = fields_held(&repo, &stand_in);
        assert!(
            bad.contains(
                &"fact `cluster-listing`: no document of the stand-in holds the field `metadata.name`"
                    .to_string()
            ),
            "{bad:?}"
        );
        assert!(
            fields_held(&repo, &StandIn::default()).is_empty(),
            "a stand-in with no document has no field to hold"
        );
    }

    #[test]
    fn each_check_reports_a_page_that_it_cannot_read() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path();
        let use_cases = book(repo).join(USE_CASES);
        std::fs::create_dir_all(&use_cases).unwrap();
        std::fs::write(repo.join("Cargo.toml"), "version = \"1.0.0\"\n").unwrap();
        std::fs::write(repo.join("README.md"), "strata\n").unwrap();
        // bytes that are not UTF-8, so the read of the page fails
        let broken = use_cases.join("broken.md");
        std::fs::write(&broken, [0xff, 0xfe]).unwrap();
        let bad = check(
            repo,
            &chapters(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            &StandIn::default(),
        );
        // release_tags, links_resolve, follow_along_first, playgrounds, seeds, and cited_run_ids each
        // read the page
        let unread = bad
            .iter()
            .filter(|m| m.starts_with(&format!("{}: cannot read the file", shown(repo, &broken))))
            .count();
        assert_eq!(unread, 6, "{bad:?}");
    }

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

    /// The outputs of a small runner: `o1` is state 3 of `loop`, and `o2` is state 1 of it.
    fn states() -> BTreeSet<OutputState> {
        let output = |output: &str, state: u32| OutputState {
            output: output.to_string(),
            chapter: "loop".to_string(),
            state,
        };
        BTreeSet::from([output("o1", 3), output("o2", 1)])
    }

    #[test]
    fn a_page_that_shows_a_later_state_and_names_its_seed_passes() {
        let text = "just playground loop-3\n{{#include ../output/o1.txt}}\n";
        assert_eq!(
            seed_faults("page.md", text, &include_pattern(), &states()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_page_that_shows_the_first_state_needs_no_seed() {
        let text = "just playground loop\n{{#include ../output/o2.txt}}\n";
        assert_eq!(
            seed_faults("page.md", text, &include_pattern(), &states()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_page_that_shows_a_later_state_and_names_no_seed_fails() {
        let text = "just playground loop\n{{#include ../output/o1.txt}}\n";
        assert_eq!(
            seed_faults("page.md", text, &include_pattern(), &states()),
            vec![
                "page.md shows state 3 of the chapter 'loop' and never names the seed 'loop-3'; \
                 a reader on 'just playground loop' cannot run it"
            ]
        );
    }

    #[test]
    fn a_git_dependency_that_names_another_release_fails_and_this_release_passes() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path();
        let docs = book(repo);
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(repo.join("Cargo.toml"), "version = \"1.0.0\"\n").unwrap();
        std::fs::write(
            docs.join("current.md"),
            "strata = { git = \"ssh://git@example.com/a.git\", tag = \"v1.0.0\" }\n",
        )
        .unwrap();
        std::fs::write(
            docs.join("stale.md"),
            "strata = { git = \"ssh://git@example.com/a.git\", tag = \"v0.9.0\" }\n",
        )
        .unwrap();
        let bad = release_tags(repo);
        assert_eq!(
            bad,
            [
                "docs/src/stale.md:1 names the release v0.9.0, and this crate is v1.0.0; \
                 a reader copies this line"
            ]
        );
    }

    #[test]
    fn a_page_that_cites_a_run_id_fails_and_a_page_with_none_passes() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path();
        let docs = book(repo);
        std::fs::create_dir_all(docs.join("sub")).unwrap();
        std::fs::write(docs.join("clean.md"), "A date is 2026-09-30.\n").unwrap();
        std::fs::write(
            docs.join("sub/cited.md"),
            "Run 20260930T120000Z shows it.\n",
        )
        .unwrap();
        let bad = cited_run_ids(repo);
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(
            bad[0].starts_with("docs/src/sub/cited.md cites the run id 20260930T120000Z."),
            "{bad:?}"
        );
    }

    #[test]
    fn a_run_id_of_any_year_is_found() {
        assert_eq!(
            run_ids("Run 20270105T083000Z and run 20301231T235959Z show it."),
            ["20270105T083000Z", "20301231T235959Z"]
        );
        assert!(run_ids("A date is 2027-01-05T08:30:00Z.").is_empty());
    }

    #[test]
    fn a_page_that_cites_a_run_id_of_2027_fails() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path();
        let docs = book(repo);
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("later.md"), "Run 20270105T083000Z shows it.\n").unwrap();
        let bad = cited_run_ids(repo);
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(
            bad[0].starts_with("docs/src/later.md cites the run id 20270105T083000Z."),
            "{bad:?}"
        );
    }
}
