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
//! ```

mod book;
mod chapters;
mod checks;
mod contract;
mod facts;
mod files;
mod palette;
// The lock of the tests of strata. The runner of the book takes it when it starts a process. A test
// takes it when it writes a script. So the lock covers each process of a test. The tool writes no
// script, so it does not call `executable`.
#[path = "../../tests/common/process.rs"]
#[allow(dead_code)]
mod process;
#[cfg(test)]
mod size;
mod tenant;

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
            "usage: xtask [docs-examples [--check] | facts [--check] | contract [OPTIONS] | harvest DIR | playground CHAPTER[-N] | serve ROOT PORT_FILE]",
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
