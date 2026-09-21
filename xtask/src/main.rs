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
//! ```

mod book;
mod chapters;
mod checks;
mod palette;
mod tenant;

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let names: Vec<&str> = args.iter().map(String::as_str).collect();
    match names.as_slice() {
        ["serve", root, port_file] => match palette::serve(Path::new(root), Path::new(port_file)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&format!("serve: {e}")),
        },
        ["docs-examples"] => examples(book::Mode::Write),
        ["docs-examples", "--check"] => examples(book::Mode::Check),
        ["playground", name] => {
            let (chapter, seed) = match name.rsplit_once('-') {
                Some((head, tail)) if tail.len() == 1 && tail.chars().all(|c| c.is_ascii_digit()) => {
                    (head.to_string(), tail.parse().unwrap_or(1))
                }
                _ => ((*name).to_string(), 1),
            };
            if !chapters::names().contains(&chapter.as_str()) {
                eprintln!(
                    "docs-examples: no chapter '{chapter}'. The chapters are: {}",
                    chapters::names().join(" ")
                );
                return ExitCode::from(2);
            }
            examples(book::Mode::Serve { chapter, seed })
        }
        _ => fail("usage: xtask [docs-examples [--check] | playground CHAPTER[-N] | serve ROOT PORT_FILE]"),
    }
}

/// Run every chapter, and write or check `docs/src/output/`.
fn examples(mode: book::Mode) -> ExitCode {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits in the repository")
        .to_path_buf();
    let strata = std::env::var("STRATA_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| repo.join("target/release/strata"));
    if !strata.is_file() {
        return fail(&format!(
            "no strata binary at {}. Run: just build",
            strata.display()
        ));
    }
    if !repo
        .join("examples/profiles/raw/aws-stack@1.1.1/profile.json")
        .is_file()
    {
        return fail("the examples are not built. Run: just examples");
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
