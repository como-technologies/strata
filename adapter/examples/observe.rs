//! Calls the platform port of knurl by hand, over HTTP, through the strata adapter.
//!
//! It reads the same settings as the `strata` command: `PALETTE_API_KEY`, `PALETTE_API`, and
//! `PALETTE_PROJECT` or `PALETTE_PROJECT_UID`, from the environment or from `~/.palette.env`.
//! So it runs against a tenant, or against `just playground <chapter>`. It reads, and it never
//! writes.
//!
//! ```text
//! observe unit <leaf|intermediate> <name> <version> [digest]   what knurl sees of a published unit
//! observe instance <name>                             what knurl sees of a cluster
//! observe pinned-by <name> <version>                  the clusters that use a profile version
//! observe verify <name> <version>                     record a profile version, then verify it
//! observe identity <name> <version>                   the digest of a definition, and of its publish
//! ```
//!
//! The digest of `unit` chooses, where a name and a version are two archives in two kinds of
//! registry. With no digest, that is a failure that names each one.
//!
//! Exit 0 for an answer, 1 when `verify` says changed or `identity` says different, and 2 for a
//! failure.

use std::future::Future;
use std::process::ExitCode;

use knurl_core::{ContentDigest, InstanceName, Level, UnitName, UnitRef, Verification, Version};
use knurl_ports::{PlatformRead, PlatformReadExt, PortError};
use strata::api::{HttpApi, Palette, Retry, RetryPolicy};
use strata::config::Config;
use strata::progress::Progress;
use strata_knurl::{StrataPlatform, definition_of};

/// A client as the `strata` command makes one. `project` is the project of a cluster that
/// `pinned-by` reads; `None` is the project of the settings.
fn connect(project: Option<&str>) -> strata::Result<Palette<Retry<HttpApi>>> {
    let mut config = Config::from_env()?;
    match project {
        Some(uid) => config.project_uid = Some(uid.to_string()).filter(|uid| !uid.is_empty()),
        None => {
            if let (None, Some(name)) = (&config.project_uid, config.project_name.clone()) {
                let mut tenant = config.clone();
                tenant.project_uid = None;
                config.project_uid = Some(client(tenant).project_uid_named(&name)?);
            }
        }
    }
    Ok(client(config))
}

fn client(config: Config) -> Palette<Retry<HttpApi>> {
    let policy = RetryPolicy::from_config(&config);
    Palette::new(Retry::new(HttpApi::new(config), policy, Progress::silent()))
}

/// Runs a future on a runtime of this thread. The adapter moves each read to a blocking task,
/// and the future waits for it.
fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime of this thread")
        .block_on(future)
}

fn unit(level: &str, name: &str, version: &str, digest: Option<&str>) -> Result<UnitRef, String> {
    let level = match level {
        "leaf" => Level::Leaf,
        "intermediate" => Level::Intermediate,
        other => {
            return Err(format!(
                "the level is leaf or intermediate, and not '{other}'"
            ));
        }
    };
    let digest = digest
        .map(|text| text.parse::<ContentDigest>().map_err(|e| e.to_string()))
        .transpose()?;
    Ok(UnitRef {
        level,
        name: UnitName::new(name).map_err(|e| e.to_string())?,
        version: Version::new(version).map_err(|e| e.to_string())?,
        digest,
    })
}

fn run(args: &[&str]) -> Result<ExitCode, String> {
    let platform = StrataPlatform::new(connect);
    let port = |error: PortError| format!("{:?} failure: {error}", error.class());
    match args {
        ["unit", level, name, version] | ["unit", level, name, version, _] => {
            let digest = args.get(4).copied();
            let seen = block_on(platform.observe_unit(&unit(level, name, version, digest)?)).map_err(port)?;
            println!("unit         {} {} ({:?})", seen.name, seen.version, seen.level);
            println!("exact        {}", seen.exact);
            println!("content      {}", seen.content);
            for (name, pin) in &seen.pins {
                println!("pin          {name} {} {}", pin.version, pin.digest);
            }
            println!("observed_at  {}", seen.observed_at.0);
        }
        ["instance", name] => {
            let name = InstanceName::new(*name).map_err(|e| e.to_string())?;
            let seen = block_on(platform.observe_instance(&name)).map_err(port)?;
            println!("instance     {}", seen.name);
            println!("state        {}", seen.state.0);
            println!("health       {:?}", seen.health);
            for (name, copy) in &seen.pins {
                println!("holds        {name} {}", copy.version);
                println!("  exact      {}", copy.exact);
                println!("  content    {}", copy.content);
            }
        }
        ["pinned-by", name, version] => {
            let holders = block_on(platform.pinned_by(&unit("intermediate", name, version, None)?)).map_err(port)?;
            println!("pinned by    {} instance(s)", holders.len());
            holders.iter().for_each(|holder| println!("  {holder}"));
        }
        ["verify", name, version] => {
            let recorded = block_on(platform.observe_unit(&unit("intermediate", name, version, None)?)).map_err(port)?;
            println!("recorded     {}", recorded.exact);
            let answer = block_on(platform.verify(&recorded)).map_err(port)?;
            println!("verify       {answer:?}");
            if answer != Verification::Unchanged {
                return Ok(ExitCode::from(1));
            }
        }
        ["identity", name, version] => {
            let address = unit("intermediate", name, version, None)?;
            let published = platform.snapshot(&address).map_err(port)?;
            let definition = definition_of(&published).map_err(port)?;
            let composed = block_on(platform.identity_of(&definition)).map_err(port)?;
            let observed = block_on(platform.observe_unit(&address)).map_err(port)?;
            println!("definition   {} pin(s), in the order of the layers", definition.pins.len());
            println!("composed     {composed}");
            println!("published    {}", observed.content);
            if composed != observed.content {
                println!("DIFFERENT: the publish is not what the definition gives");
                return Ok(ExitCode::from(1));
            }
            println!("SAME: the definition gives the content of the published version");
        }
        _ => return Err("usage: observe unit <leaf|intermediate> <name> <version> [digest] | instance <name> | pinned-by <name> <version> | verify <name> <version> | identity <name> <version>".into()),
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run(&args).unwrap_or_else(|message| {
        eprintln!("observe: {message}");
        ExitCode::from(2)
    })
}
