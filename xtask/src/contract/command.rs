//! The command line of the contract run: `xtask contract`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use strata::api::{connect_from_env, connect_with};
use strata::config::Config;
use strata::term::progress::Progress;
use strata_preflight::probe::LiveBackend;

use super::cloud::{Lab, run as run_clouds};
use super::releases::{self, GITHUB};
use super::run;
use super::stand_in::{StandIn, harvest};
use crate::book::built_examples;
use crate::facts::subjects::floor;
use crate::{facts, fail, repository};

/// The usage line, and what the options do.
const USAGE: &str =
    "usage: xtask contract [--stand-in DIR] [--out FILE] [--state FILE] [--github URL] [--no-cloud] [--with-cluster]

  --stand-in DIR  read the documents of the stand-in from DIR, one directory for each chapter;
                  without it, build the tenant of every chapter of the book (needs: just examples)
  --out FILE      write the document strata-contract/1 to FILE; without it, to standard output
  --state FILE    read the latest release of the Palette SDK and of the Terraform provider from
                  GitHub, compare each tag with the tag in FILE, and keep the new tags in FILE
  --github URL    the address of the GitHub API (default https://api.github.com)
  --no-cloud      send no call of the tool of a cloud (aws, gcloud, az); without it, the run sends the
                  read-only calls of the facts of the kind cloud in the lab account
  --with-cluster  the tenant holds a cluster: a fact that takes its subject from a cluster and that the
                  run skips is an error, and the exit code is 2

The tenant is the one that PALETTE_API, PALETTE_API_KEY, and ~/.palette.env name. The run sends a fact
of the tenant scope with no project, as strata does. The lab account of a cloud is the one that the
tool uses. STRATA_CONTRACT_<NAME> gives {name} in a call, such as the project.
Exit 0 when each fact is the same, 1 when a fact differs or a value is unknown, 2 when the run could
not check a fact, or checked none.";

/// The options of the run.
pub(super) struct Options {
    pub(super) stand_in: Option<PathBuf>,
    pub(super) out: Option<PathBuf>,
    pub(super) state: Option<PathBuf>,
    pub(super) github: String,
    pub(super) no_cloud: bool,
    pub(super) with_cluster: bool,
}

impl Options {
    pub(super) fn parse(args: &[&str]) -> Result<Options, String> {
        let mut options = Options {
            stand_in: None,
            out: None,
            state: None,
            github: GITHUB.to_string(),
            no_cloud: false,
            with_cluster: false,
        };
        let mut args = args.iter();
        while let Some(flag) = args.next() {
            let mut value = || {
                args.next()
                    .map(|v| (*v).to_string())
                    .ok_or_else(|| format!("{flag} needs a value"))
            };
            match *flag {
                "--stand-in" => options.stand_in = Some(PathBuf::from(value()?)),
                "--out" => options.out = Some(PathBuf::from(value()?)),
                "--state" => options.state = Some(PathBuf::from(value()?)),
                "--github" => options.github = value()?.trim_end_matches('/').to_string(),
                "--no-cloud" => options.no_cloud = true,
                "--with-cluster" => options.with_cluster = true,
                other => return Err(format!("unknown option {other}")),
            }
        }
        Ok(options)
    }
}

/// Run the contract run with the arguments `args` after the word `contract`.
///
/// The exit code is 0 when each fact the run checked is the same, and 1 when a fact differs or a
/// value is unknown. It is 2 when the run could not check, or the arguments are wrong.
pub fn command(args: &[&str]) -> ExitCode {
    if args == ["--help"] {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let options = match Options::parse(args) {
        Ok(options) => options,
        Err(message) => return fail(&format!("contract: {message}\n{USAGE}")),
    };
    match contract(&options) {
        Ok(code) => ExitCode::from(code),
        Err(message) => fail(&format!("contract: {message}")),
    }
}

pub(super) fn contract(options: &Options) -> Result<u8, String> {
    let repo = repository();
    let facts = facts::load(&repo).map_err(|bad| bad.join("\nxtask: "))?;
    let stand_in = stand_in(&repo, options.stand_in.as_deref())?;
    let config = Config::from_env().map_err(|e| e.to_string())?;
    let palette = connect_from_env(Progress::silent()).map_err(|e| e.to_string())?;
    let checked_at = strata::timestamp::now_utc();
    // the second client names no project, for the routes that refuse one
    let tenant = connect_with(config.clone().without_project(), Progress::silent());
    let mut document = run(
        &facts,
        &stand_in,
        palette.api(),
        tenant.api(),
        &config.base_url,
        &checked_at,
    );
    if !options.no_cloud {
        let rules = facts::cloud_rules().map_err(|bad| bad.join("\nxtask: "))?;
        let backend = LiveBackend::new(Some(palette.api()), false);
        document.add(run_clouds(&facts, &rules, &backend, &Lab::from_env()));
    }
    if options.with_cluster {
        document.hold_floor(&floor(&facts));
    }
    let mut state_error = None;
    if let Some(state) = &options.state {
        match releases::read(&options.github, state) {
            Ok(releases) => document.releases = releases,
            Err(e) => state_error = Some(e),
        }
    }
    let text = serde_json::to_string_pretty(&document).map_err(|e| e.to_string())?;
    match &options.out {
        Some(file) => {
            std::fs::write(file, text + "\n").map_err(|e| format!("{}: {e}", file.display()))?
        }
        None => println!("{text}"),
    }
    eprintln!("contract: {}", document.summary.line());
    for release in document.releases.iter().filter(|r| r.new) {
        eprintln!(
            "contract: {} has a new release: {}",
            release.repo,
            release.tag.as_deref().unwrap_or_default()
        );
    }
    match state_error {
        Some(e) => Err(format!("the state of the releases: {e}")),
        None => Ok(document.exit_code()),
    }
}

/// Write the tenant of every chapter of the book to the directory `dir`, one directory for each
/// chapter, as `xtask contract --stand-in DIR` reads them. It gives the exit code of the command.
pub fn harvest_command(dir: &str) -> ExitCode {
    let repo = repository();
    let written = built_examples(&repo).and_then(|()| {
        let xtask = std::env::current_exe().map_err(|e| e.to_string())?;
        harvest(&repo, &xtask, Path::new(dir)).map_err(|e| e.to_string())
    });
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => fail(&format!("harvest: {message}")),
    }
}

/// The documents of the stand-in: the directory `given`, or the tenants that a harvest of the book
/// builds in a directory of its own.
fn stand_in(repo: &Path, given: Option<&Path>) -> Result<StandIn, String> {
    if let Some(dir) = given {
        return StandIn::read(dir);
    }
    built_examples(repo)?;
    let into = std::env::temp_dir().join(format!("strata-contract-{}", std::process::id()));
    // a directory of an earlier run with this process id is no harvest of this run
    let _ = std::fs::remove_dir_all(&into);
    let xtask = std::env::current_exe().map_err(|e| e.to_string())?;
    let built = harvest(repo, &xtask, &into).map_err(|e| e.to_string());
    let stand_in = StandIn::read(&into);
    // the directory is in the temporary directory, so a failed removal costs only disk space
    let _ = std::fs::remove_dir_all(&into);
    built?;
    stand_in
}
