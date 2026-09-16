//! The strata command line: argument parsing, printing, and exit codes. Everything else is in the
//! library.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};

use strata::api::{HttpApi, Palette, Retry, RetryPolicy};
use strata::color::{ColorMode, Paint};
use strata::config::Config;
use strata::error::{Error, Result};
use strata::progress::Progress;
use strata::reference::Ref;
use strata::render::{self, Format};
use strata::snapshot::Snapshot;
use strata::{diff, ops, TOOL_NAME};

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
Refs: a profile uid, <name>@<version>, <name>@latest (the highest version in the scope), or a\n\
snapshot file written by `snapshot`.\n\
Environment: PALETTE_API_KEY (or SPECTROCLOUD_APIKEY), PALETTE_PROJECT_UID (optional),\n\
PALETTE_API (default https://api.spectrocloud.com), STRATA_RETRIES (default 3; retries on HTTP 5xx,\n\
429 and transport errors, with backoff). Missing values are read from ~/.palette.env.\n\n\
Exit status: 0 identical (verify: unchanged), 1 different (verify: drifted), 2 error."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

impl Cli {
    /// True when the command will write JSON, so an error must be JSON too.
    fn json_selected(&self) -> bool {
        match &self.cmd {
            Cmd::Compare(a) => a.out.format() == Format::Json,
            Cmd::Verify(a) => a.out.format() == Format::Json,
            Cmd::Snapshot(_) | Cmd::Schema { .. } => false,
        }
    }
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
    format: FormatArg,
    /// Shorthand for --format json
    #[arg(long)]
    json: bool,
    /// Color the terminal report; auto colors only when stdout is a terminal and honors NO_COLOR and CLICOLOR_FORCE
    #[arg(long, value_enum, default_value = "auto", num_args = 0..=1, default_missing_value = "always")]
    color: ColorArg,
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
            self.format.into()
        }
    }
    fn paint(&self) -> Paint {
        Paint::resolve(if self.no_color {
            ColorMode::Never
        } else {
            self.color.into()
        })
    }
    fn renderer(&self) -> Box<dyn render::Renderer> {
        render::renderer(self.format(), self.paint(), self.compact)
    }
    fn progress(&self) -> Progress {
        progress(self.quiet)
    }
}

#[derive(Args)]
struct CompareArgs {
    /// Profile uid, <name>@<version> or <name>@latest, or snapshot file
    a: String,
    /// Profile uid, <name>@<version> or <name>@latest, or snapshot file
    b: String,
    /// Also print unified diffs of raw values and changed manifests
    #[arg(long)]
    full: bool,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct SnapshotArgs {
    /// Profile uid, <name>@<version> or <name>@latest (omit with --from-raw)
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
enum FormatArg {
    Terminal,
    Markdown,
    Json,
}

impl From<FormatArg> for Format {
    fn from(f: FormatArg) -> Format {
        match f {
            FormatArg::Terminal => Format::Terminal,
            FormatArg::Markdown => Format::Markdown,
            FormatArg::Json => Format::Json,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ColorArg {
    Auto,
    Always,
    Never,
}

impl From<ColorArg> for ColorMode {
    fn from(c: ColorArg) -> ColorMode {
        match c {
            ColorArg::Auto => ColorMode::Auto,
            ColorArg::Always => ColorMode::Always,
            ColorArg::Never => ColorMode::Never,
        }
    }
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
    let json = cli.json_selected();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            if json {
                eprintln!("{}", serde_json::json!({ "error": e.to_json() }));
            } else {
                eprintln!("{TOOL_NAME}: {e}");
            }
            ExitCode::from(e.exit_code())
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

fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn progress(quiet: bool) -> Progress {
    Progress::new(quiet || std::env::var_os("STRATA_QUIET").is_some())
}

fn connect(progress: Progress) -> Result<Palette<Retry<HttpApi>>> {
    let config = Config::from_env()?;
    let policy = RetryPolicy::new(config.retries);
    Ok(Palette::new(Retry::new(
        HttpApi::new(config),
        policy,
        progress,
    )))
}

fn exit_for(same: bool) -> ExitCode {
    if same {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn cmd_compare(a: &CompareArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let now = now_utc();
    let sa = ops::snapshot_for(&Ref::parse(&a.a), || connect(progress), &now, &progress)?;
    let sb = ops::snapshot_for(&Ref::parse(&a.b), || connect(progress), &now, &progress)?;
    let d = ops::compare(&sa, &sb, &now);
    print!("{}", a.out.renderer().render(&d, &sa, &sb, a.full)?);
    Ok(exit_for(d.same))
}

fn cmd_snapshot(a: &SnapshotArgs) -> Result<ExitCode> {
    let progress = progress(a.quiet);
    let now = now_utc();
    let snap = match &a.from_raw {
        Some(dir) => {
            let reference = a.r#ref.clone().unwrap_or_else(|| dir.display().to_string());
            ops::snapshot_from_dir(dir, &reference, &now)?
        }
        None => {
            let text = a.r#ref.as_deref().ok_or_else(|| Error::Transport {
                path: String::new(),
                message: "snapshot needs a profile ref or --from-raw DIR".to_string(),
            })?;
            let reference = Ref::parse(text);
            let palette = connect(progress)?;
            let uid = match &reference {
                Ref::Snapshot(_) | Ref::Uid(_) => text.to_string(),
                Ref::NameVersion { name, version } => palette.resolve(name, version)?,
            };
            let raw = palette.fetch_profile(&uid, &progress)?;
            if let Some(dir) = &a.raw_dir {
                raw.save_dir(dir)?;
            }
            Snapshot::from_raw(&raw, text, &now)?
        }
    };
    let text = snap.to_json(a.compact)?;
    match &a.output {
        Some(path) => {
            std::fs::write(path, text).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            progress.note(&format!(
                "wrote {} (root {})",
                path.display(),
                &snap.root[..12]
            ));
        }
        None => print!("{text}"),
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_verify(a: &VerifyArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let recorded = Snapshot::load(&a.snapshot)?;
    let palette = connect(progress)?;
    let v = ops::verify(&recorded, &palette, &now_utc(), &progress)?;
    let renderer = a.out.renderer();
    print!("{}", renderer.render(&v.diff, &recorded, &v.live, false)?);
    if let Some(line) = v
        .diff
        .verify
        .as_ref()
        .and_then(|ver| renderer.verdict(ver, &recorded.source))
    {
        println!("{line}");
    }
    Ok(exit_for(v.unchanged))
}

fn cmd_schema(kind: SchemaKind) -> Result<ExitCode> {
    let schema = match kind {
        SchemaKind::Snapshot => schemars::schema_for!(Snapshot),
        SchemaKind::Diff => schemars::schema_for!(diff::Diff),
    };
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(ExitCode::SUCCESS)
}
