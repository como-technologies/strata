//! strata: record, compare, and verify Spectro Cloud Palette cluster profile versions as
//! immutable, layered releases.
//!
//! A profile is a stack of layers (packs). Every layer gets three sub-hashes so a report names the
//! kind of change: identity (which pack build the layer references), upstream (the digest Palette
//! stamps on the pack ref), and authored (values, manifests, params). node = sha256(identity +
//! upstream + authored); root = sha256(variablesHash, [key, node] in profile order). A snapshot
//! is the record of one published version; compare reads two records; verify checks a record
//! against the live profile.

mod api;
mod canon;
mod color;
mod diff;
mod render;
mod snapshot;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::color::{ColorMode, Paint};
use crate::snapshot::Snapshot;

pub const TOOL_NAME: &str = "strata";
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const TOOL_IMPL: &str = "rust";

const SUBCOMMANDS: &[&str] = &["compare", "snapshot", "verify", "schema", "help"];

#[derive(Parser)]
#[command(
    name = TOOL_NAME,
    version,
    about = "Record, compare, and verify Palette cluster profile versions as immutable, layered releases",
    long_about = "A Palette cluster profile is a stack of layers. Strata records each published version as a\n\
snapshot, compares two versions and says what kind of change happened in each layer, and verifies\n\
that a published version still matches its record.\n\n\
Each pack gets three sub-hashes: IDENTITY (name, layer, type, tag, version, packUid, registryUid),\n\
UPSTREAM (the digest on the pack ref) and AUTHORED (canonical values YAML, manifests, params).\n\
A profile's root hash covers its variables and its packs in order; same root, same release.\n\n\
Refs: a profile uid, <name>@<version>, or a snapshot file written by `snapshot`.\n\
Environment: PALETTE_API_KEY (or SPECTROCLOUD_APIKEY), PALETTE_PROJECT_UID (optional),\n\
PALETTE_API (default https://api.spectrocloud.com). Missing values are read from ~/.palette.env.\n\n\
Exit status: 0 identical (verify: unchanged), 1 different (verify: drifted), 2 error."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Compare two profiles (the default when the first argument is not a subcommand)
    Compare(CompareArgs),
    /// Fetch one profile and write its snapshot: Merkle root, per-pack hashes, canonical contents
    Snapshot(SnapshotArgs),
    /// Re-fetch a snapshot's profile; exit 1 if the published version was edited in place
    Verify(VerifyArgs),
    /// Print the JSON Schema of the snapshot or the diff document
    Schema {
        #[arg(value_enum)]
        kind: SchemaKind,
    },
}

#[derive(Args)]
struct OutputArgs {
    /// Output format: terminal (colored text), markdown (PR comment), json (machine-readable)
    #[arg(long, value_enum, default_value = "terminal")]
    format: Format,
    /// Shorthand for --format json
    #[arg(long)]
    json: bool,
    /// Color the terminal report; auto colors only when stdout is a terminal and honors NO_COLOR and CLICOLOR_FORCE
    #[arg(long, value_enum, default_value = "auto", num_args = 0..=1, default_missing_value = "always")]
    color: ColorMode,
    /// Same as --color=never
    #[arg(long)]
    no_color: bool,
    /// Compact JSON on one line instead of pretty-printed
    #[arg(long)]
    compact: bool,
    /// Suppress progress messages on stderr
    #[arg(short, long)]
    quiet: bool,
}

impl OutputArgs {
    fn format(&self) -> Format {
        if self.json {
            Format::Json
        } else {
            self.format
        }
    }
    fn paint(&self) -> Paint {
        Paint::resolve(if self.no_color {
            ColorMode::Never
        } else {
            self.color
        })
    }
}

#[derive(Args)]
struct CompareArgs {
    /// Profile uid, <name>@<version>, or snapshot file
    a: String,
    /// Profile uid, <name>@<version>, or snapshot file
    b: String,
    /// Also print unified diffs of raw values and changed manifests
    #[arg(long)]
    full: bool,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct SnapshotArgs {
    /// Profile uid or <name>@<version> (omit with --from-raw)
    r#ref: Option<String>,
    /// Write the snapshot here instead of stdout
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Also save the raw API responses under this directory
    #[arg(long)]
    raw_dir: Option<PathBuf>,
    /// Build from raw API responses saved earlier (profile.json, manifests.json, variables.json), no network
    #[arg(long)]
    from_raw: Option<PathBuf>,
    /// Compact JSON on one line instead of pretty-printed
    #[arg(long)]
    compact: bool,
    /// Suppress progress messages on stderr
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Args)]
struct VerifyArgs {
    /// Snapshot file written earlier by `snapshot`
    snapshot: PathBuf,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Terminal,
    Markdown,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum SchemaKind {
    Snapshot,
    Diff,
}

fn main() -> ExitCode {
    let mut argv: Vec<String> = std::env::args().collect();
    let first = argv.get(1).map(String::as_str).unwrap_or("");
    let is_global_flag = matches!(first, "-h" | "--help" | "-V" | "--version");
    if argv.len() > 1 && !SUBCOMMANDS.contains(&first) && !is_global_flag {
        argv.insert(1, "compare".to_string());
    }
    let cli = Cli::parse_from(argv);
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{TOOL_NAME}: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.cmd {
        Cmd::Compare(a) => cmd_compare(&a),
        Cmd::Snapshot(a) => cmd_snapshot(&a),
        Cmd::Verify(a) => cmd_verify(&a),
        Cmd::Schema { kind } => cmd_schema(kind),
    }
}

pub fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn log(quiet: bool, msg: &str) {
    if !quiet && std::env::var_os("STRATA_QUIET").is_none() {
        eprintln!("{TOOL_NAME}: {msg}");
    }
}

/// Resolve a ref to a snapshot: a snapshot file is loaded, anything else is fetched.
fn snapshot_for(reference: &str, quiet: bool) -> Result<Snapshot> {
    let path = Path::new(reference);
    if path.is_file() {
        return Snapshot::load(path);
    }
    let client = api::Client::from_env()?;
    let uid = client.resolve_uid(reference)?;
    let raw = client.fetch_raw(&uid, None, quiet)?;
    Snapshot::from_raw(&raw, reference, &now_utc())
}

fn emit(d: &diff::Diff, sa: &Snapshot, sb: &Snapshot, out: &OutputArgs, full: bool) -> Result<()> {
    let text = match out.format() {
        Format::Json => {
            let mut s = if out.compact {
                serde_json::to_string(d)?
            } else {
                serde_json::to_string_pretty(d)?
            };
            s.push('\n');
            s
        }
        Format::Terminal => {
            let p = out.paint();
            let mut s = render::terminal(d, &p);
            if full {
                s.push_str(&render::full_diffs(sa, sb, d, &p, false));
            }
            s
        }
        Format::Markdown => {
            let mut s = render::markdown(d);
            if full {
                s.push_str(&render::full_diffs(sa, sb, d, &Paint::off(), true));
            }
            s
        }
    };
    print!("{text}");
    Ok(())
}

fn cmd_compare(a: &CompareArgs) -> Result<ExitCode> {
    let sa = snapshot_for(&a.a, a.out.quiet)?;
    let sb = snapshot_for(&a.b, a.out.quiet)?;
    let d = diff::compute(&sa, &sb, &now_utc(), None);
    emit(&d, &sa, &sb, &a.out, a.full)?;
    Ok(if d.same {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn cmd_snapshot(a: &SnapshotArgs) -> Result<ExitCode> {
    let (raw, reference) = if let Some(dir) = &a.from_raw {
        let reference = a.r#ref.clone().unwrap_or_else(|| dir.display().to_string());
        (api::load_raw(dir)?, reference)
    } else {
        let reference = a
            .r#ref
            .clone()
            .ok_or_else(|| anyhow!("snapshot needs a profile ref or --from-raw DIR"))?;
        let client = api::Client::from_env()?;
        let uid = client.resolve_uid(&reference)?;
        (
            client.fetch_raw(&uid, a.raw_dir.as_deref(), a.quiet)?,
            reference,
        )
    };
    let snap = Snapshot::from_raw(&raw, &reference, &now_utc())?;
    let text = snap.to_json(a.compact)?;
    match &a.output {
        Some(path) => {
            std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
            log(
                a.quiet,
                &format!("wrote {} (root {})", path.display(), &snap.root[..12]),
            );
        }
        None => print!("{text}"),
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_verify(a: &VerifyArgs) -> Result<ExitCode> {
    let recorded = Snapshot::load(&a.snapshot)?;
    let uid = recorded.source.uid.clone();
    if uid.is_empty() {
        return Err(anyhow!("snapshot has no source.uid to verify against"));
    }
    let (name, ver, at) = (
        recorded.source.name.clone(),
        recorded.source.version.clone(),
        recorded.source.fetched_at.clone(),
    );
    log(
        a.out.quiet,
        &format!("verifying {name}@{ver} ({uid}) against live; recorded {at}"),
    );
    let client = api::Client::from_env()?;
    let raw = client.fetch_raw(&uid, None, a.out.quiet)?;
    let checked_at = now_utc();
    let live = Snapshot::from_raw(&raw, &uid, &checked_at)?;
    let same = recorded.root == live.root;
    let verify = diff::Verify {
        status: if same {
            diff::VerifyStatus::Unchanged
        } else {
            diff::VerifyStatus::Drifted
        },
        uid: uid.clone(),
        recorded_at: at.clone(),
        checked_at: checked_at.clone(),
    };
    let d = diff::compute(&recorded, &live, &checked_at, Some(verify));
    emit(&d, &recorded, &live, &a.out, false)?;
    let p = a.out.paint();
    match a.out.format() {
        Format::Json => {}
        Format::Terminal => {
            if same {
                println!(
                    "{}: published version {name}@{ver} is unchanged since {at}",
                    p.bgreen("OK")
                );
            } else {
                println!(
                    "{}: published version {name}@{ver} differs from the copy recorded {at}",
                    p.bred("DRIFT")
                );
            }
        }
        Format::Markdown => {
            if same {
                println!("\n**OK**: published version `{name}@{ver}` is unchanged since {at}");
            } else {
                println!("\n**DRIFT**: published version `{name}@{ver}` differs from the copy recorded {at}");
            }
        }
    }
    Ok(if same {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn cmd_schema(kind: SchemaKind) -> Result<ExitCode> {
    let schema = match kind {
        SchemaKind::Snapshot => schemars::schema_for!(Snapshot),
        SchemaKind::Diff => schemars::schema_for!(diff::Diff),
    };
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(ExitCode::SUCCESS)
}
