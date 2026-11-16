//! Development tooling for strata. It builds no part of the release: the `strata` crate ships
//! alone, and nothing here reaches a published binary.
//!
//! Run a command with `cargo xtask <command>`, which the alias in `.cargo/config.toml` gives, or
//! through the `just` recipe that wraps it.
//!
//! ```text
//! xtask docs-examples            write docs/src/output/
//! xtask docs-examples --check    exit 1 when docs/src/output/ is not current
//! xtask playground CHAPTER[-N]   build that chapter's tenant, serve it, and wait
//! xtask serve ROOT PORT_FILE     serve ROOT as the Palette API, and write the port to PORT_FILE
//! xtask facts                    print the list of the facts of Palette as JSON
//! xtask facts --check            exit 1 when the list does not agree with the book and the scripts
//! xtask contract [OPTIONS]       ask a real tenant the requests of the facts, and compare the answers
//!                                with the stand-in; `xtask contract --help` names the options
//! xtask harvest DIR              write the tenant of every chapter to DIR, for xtask contract --stand-in
//! xtask comment-words BASE       exit 1 when a comment sentence that the change since BASE adds has
//!                                more than 25 words
//! ```

mod book;
mod chapters;
mod checks;
mod comment_words;
// The read of the code spans of a text of the book. The check of the facts reads the code spans of
// a cell with it, and so do the tests. So the check and the tests read a code span in one way. The
// check uses no other helper of it.
#[path = "../../tests/common/code_span.rs"]
#[allow(dead_code)]
mod code_span;
// The routes and the answers of Palette that the tests of strata serve. xtask uses the items below
// of it, and no other item. So the tests, the stand-in, and the book read one text.
//
// The stand-in builds the body of each error with `error_body`. The tests of the contract use it
// too. The stand-in builds each denial with `access_denied_message`, `project_denied_message`,
// `operation_forbidden_message`, and `EACH_SCOPE`. The chapter `list-denied` uses the first two.
//
// The stand-in gives the role bindings of a cluster to the user of a key with `rbacs_answer`. The
// chapter `cluster-tiers` and the tests of `palette/rbacs.rs` use `system_rbacs`.
//
// The stand-in serves the variables of a cluster with the masks of Palette. It uses
// `cluster_route_variable` and `is_masked_variable`, as the cluster fixtures of the tests do. Its
// tests serve the variables of a profile with the masks and `profile_route_variable`.
//
// The tests of the contract and of the facts use `TAG_FILTER_LISTING`, the route of the saved tag
// filters. The stand-in, the test of its download route, and the chapter `fields` build the answer
// of the pack listing with `pack_listing_answer`.
#[path = "../../tests/common/palette.rs"]
#[allow(dead_code)]
mod common_palette;
mod contract;
mod facts;
mod files;
// The git settings of the tests of strata. The runner of the book gives them to each step, so the
// tests and the chapters keep one copy of the settings.
#[path = "../../tests/common/git.rs"]
mod git;
mod palette;
// The lock of the tests of strata. The runner of the book takes it when it starts a process. A test
// takes it when it writes a script. So the lock covers each process of a test. The tool writes no
// script, so it does not call `executable`.
#[path = "../../tests/common/process.rs"]
#[allow(dead_code)]
mod process;
// The match of a route of Palette with a pattern. The checks of the facts and of the stand-in use
// it, and so does the change test of the catalog of strata-preflight. So the two keep one match.
#[path = "../../tests/common/route.rs"]
mod route;
// The count of the sentences and the words of a text, as "The style rules" give it. The check of
// the comment sentences of a change counts with it, and so do the tests of the book. So the two
// count a word in one way. The check does not join the lines of a block with `comment_blocks`.
#[path = "../../tests/common/sentences.rs"]
#[allow(dead_code)]
mod sentences;
#[cfg(test)]
mod size;
// The read of a table of the book. The check of the facts reads a row with it, and so do the
// tests. So the check and the tests read a row in one way. The check uses no other helper of it.
#[path = "../../tests/common/table.rs"]
#[allow(dead_code)]
mod table;
mod tenant;
// The readers of a TOML file of the tests of strata. The checks read the version of `Cargo.toml`
// with them, so the tests and the checks keep one reader. The checks read no table.
#[path = "../../tests/common/toml.rs"]
#[allow(dead_code)]
mod toml;
// The macro that ties the list of a test to each variant of an enum. The test of each edit of the
// stand-in uses it. So do the tests of the errors of strata and the test of `Check` in
// `strata-preflight`. So the lists keep one rule.
#[cfg(test)]
#[path = "../../tests/common/variants.rs"]
mod variants;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let names: Vec<&str> = args.iter().map(String::as_str).collect();
    match names.as_slice() {
        ["serve", root, port_file] => match palette::serve(Path::new(root), Path::new(port_file)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&format!("serve: {e}")),
        },
        ["facts"] => print_facts(),
        ["facts", "--check"] => check_facts(),
        ["contract", rest @ ..] => contract::command(rest),
        ["harvest", dir] => contract::harvest_command(dir),
        ["comment-words", base] => comment_words(base),
        ["docs-examples"] => examples(book::Mode::Write),
        ["docs-examples", "--check"] => examples(book::Mode::Check),
        ["playground", name] => {
            let (chapter, seed) = book::playground(name);
            examples(book::Mode::Serve {
                chapter: chapter.to_string(),
                seed,
            })
        }
        _ => fail(
            "usage: xtask [docs-examples [--check] | facts [--check] | contract [OPTIONS] | harvest DIR | comment-words BASE | playground CHAPTER[-N] | serve ROOT PORT_FILE]",
        ),
    }
}

/// The root of the repository.
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits in the repository")
        .to_path_buf()
}

/// Print the list of the facts as JSON.
fn print_facts() -> ExitCode {
    match facts::print(&repository()) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(bad) => fail(&bad.join("\nxtask: ")),
    }
}

/// Compare the list with the book and the scripts, without a run of the book.
fn check_facts() -> ExitCode {
    let bad = facts::check(&repository(), None);
    for message in &bad {
        eprintln!("xtask: {message}");
    }
    ExitCode::from(u8::from(!bad.is_empty()))
}

/// Name each comment sentence of more than 25 words that the change since `base` adds, and count
/// the sentences that it adds. Exit 1 when it names one.
fn comment_words(base: &str) -> ExitCode {
    let found = match comment_words::check(&repository(), base) {
        Ok(found) => found,
        Err(e) => return fail(&format!("comment-words: {e}")),
    };
    for fault in &found.faults {
        eprintln!("xtask: {fault}");
    }
    eprintln!(
        "xtask: comment-words: the change since {base} adds or changes {} comment sentences, and {} have more than {} words",
        found.sentences,
        found.faults.len(),
        sentences::MOST_WORDS
    );
    ExitCode::from(u8::from(!found.faults.is_empty()))
}

/// Run every chapter, and write or check `docs/src/output/`.
fn examples(mode: book::Mode) -> ExitCode {
    let repo = repository();
    let strata = std::env::var("STRATA_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| repo.join("target/release/strata"));
    if !strata.is_file() {
        return fail(&format!(
            "no strata binary at {}. Run: just build",
            strata.display()
        ));
    }
    if let Err(message) = book::built_examples(&repo) {
        return fail(&message);
    }
    // a run that writes or checks the output runs the steps, and a step runs the observer
    if matches!(mode, book::Mode::Write | book::Mode::Check)
        && let Err(message) = book::built_observer(&repo)
    {
        return fail(&message);
    }
    let xtask = std::env::current_exe().expect("this binary has a path");
    let mut runner = match book::Runner::start(&repo, &strata, &xtask, mode) {
        Ok(r) => r,
        Err(e) => return fail(&format!("{e}")),
    };
    chapters::run(&mut runner);
    runner.finish()
}

fn fail(message: &str) -> ExitCode {
    eprintln!("xtask: {message}");
    ExitCode::from(2)
}
