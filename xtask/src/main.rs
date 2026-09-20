//! Development tooling for strata. It builds no part of the release: the `strata` crate ships
//! alone, and this one runs only from a `just` recipe.
//!
//! ```text
//! xtask serve ROOT PORT_FILE     serve ROOT as the Palette API, and write the port to PORT_FILE
//! xtask backlog FILE DIR         one line for each section of FILE, with its body in DIR
//! ```

mod backlog;
mod palette;

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
        ["backlog", file, dir] => match backlog::split(Path::new(file), Path::new(dir)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&format!("backlog: {e}")),
        },
        _ => fail("usage: xtask serve ROOT PORT_FILE | xtask backlog FILE DIR"),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("xtask: {message}");
    ExitCode::from(2)
}
