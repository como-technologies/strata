//! The strata command line: argument parsing, printing, and exit codes. Everything else is in the
//! library.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::OnceLock;

use clap::{Args, Parser, Subcommand, ValueEnum};

use strata::api::{HttpApi, Palette, Retry, RetryPolicy};
use strata::color::{ColorMode, Paint};
use strata::config::Config;
use strata::error::{Error, Result};
use strata::ops::{ListKind, ListOptions, Policy};
use strata::progress::Progress;
use strata::reference::Ref;
use strata::render::{self, Format};
use strata::snapshot::Snapshot;
use strata::{candidate, diff, ops, TOOL_NAME};

const SUBCOMMANDS: &[&str] = &[
    "compare",
    "snapshot",
    "candidate",
    "verify",
    "same-release",
    "consumers",
    "list",
    "schema",
    "help",
];

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
Refs: a profile uid, <name>@<version>, <name>@latest (the highest version in the scope),\n\
cluster:<uid or name> or cluster:<uid or name>/<profile> (what a cluster runs), or a snapshot file written by\n\
`snapshot`. `list` shows the profile and cluster refs in the scope. A cluster that runs a recorded version has that record's root. A snapshot records the\n\
project it was taken in, and verify checks it there unless --project says otherwise.\n\
Environment: PALETTE_API_KEY (or SPECTROCLOUD_APIKEY), PALETTE_PROJECT_UID or the project name\n\
PALETTE_PROJECT (optional; the uid wins),\n\
PALETTE_API (default https://api.spectrocloud.com), STRATA_RETRIES (default 3; retries on HTTP 5xx,\n\
429 and transport errors, with backoff), STRATA_MAX_RETRY_AFTER (default 300; a longer Retry-After\n\
fails at once). Missing values are read from ~/.palette.env.\n\n\
Exit status: 0 identical (verify: unchanged, same-release: same), 1 different (verify: drifted,\n\
same-release: not the same), 2 error."
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
            Cmd::SameRelease(a) => a.out.format() == Format::Json,
            Cmd::Consumers(a) => a.out.format() == Format::Json,
            Cmd::List(a) => a.out.format() == Format::Json,
            Cmd::Snapshot(_) | Cmd::Candidate(_) | Cmd::Schema { .. } => false,
        }
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// Compare two profiles (the default when the first argument is not a subcommand)
    Compare(CompareArgs),
    /// Fetch one profile and write its snapshot: Merkle root, per-pack hashes, canonical contents
    Snapshot(SnapshotArgs),
    /// Write a profile version or record as a candidate file, which `snapshot --from-candidate`
    /// turns back into the same root
    Candidate(CandidateArgs),
    /// Re-fetch each record's profile; exit 1 if a published version was edited in place. Several
    /// files or a pattern print one line per record and a summary
    Verify(VerifyArgs),
    /// Check that two or more records are the same release, comparing each to the next in order
    SameRelease(SameReleaseArgs),
    /// List the clusters that use a profile version, with name, project, and state; exit 1 when any do
    Consumers(ConsumersArgs),
    /// List the profile versions and clusters in the scope as refs, and the projects the key can see
    List(ListArgs),
    /// Print the JSON Schema of a document: snapshot, diff, verify-summary, chain, consumers, list, or the candidate file
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
    /// Profile uid, <name>@<version> or <name>@latest, cluster:<uid or name>[/<profile>], or snapshot file
    a: String,
    /// Profile uid, <name>@<version> or <name>@latest, cluster:<uid or name>[/<profile>], or snapshot file
    b: String,
    /// Also print unified diffs of raw values and changed manifests
    #[arg(long)]
    full: bool,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct SnapshotArgs {
    /// Profile uid, <name>@<version> or <name>@latest, or cluster:<uid or name>[/<profile>]. With
    /// --from-raw it only names the source, and selects the profile of a cluster directory
    r#ref: Option<String>,
    /// Write the snapshot here instead of stdout
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Also save the raw API responses under this directory
    #[arg(long)]
    raw_dir: Option<PathBuf>,
    /// Build from raw API responses saved earlier (profile.json or cluster.json, with manifests.json, variables.json, and scope.json when present), no network
    #[arg(long)]
    from_raw: Option<PathBuf>,
    /// Build from a candidate file before the version exists in Palette; packs are looked up in their registries. See `strata schema candidate`
    #[arg(long, conflicts_with_all = ["from_raw", "raw_dir", "ref"])]
    from_candidate: Option<PathBuf>,
    /// Compact JSON on one line instead of pretty-printed
    #[arg(long)]
    compact: bool,
    /// Suppress progress messages on stderr
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Args)]
struct CandidateArgs {
    /// Profile uid, <name>@<version> or <name>@latest, or a profile record
    r#ref: String,
    /// Write the candidate file here instead of stdout
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Write each layer's values and each manifest to a file under this directory, referenced
    /// from the candidate file
    #[arg(long, requires = "output")]
    files_dir: Option<PathBuf>,
    /// Suppress progress messages on stderr
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Args)]
struct VerifyArgs {
    /// Snapshot files written earlier by `snapshot`. A quoted pattern such as 'releases/**/*.json'
    /// is expanded by strata. One file prints the full report; more, or a pattern, print one line
    /// per record and a summary
    #[arg(required = true)]
    snapshots: Vec<String>,
    /// Check every record in this project instead of the project each record was taken in
    #[arg(long)]
    project: Option<String>,
    /// Like --project, with the project's name
    #[arg(long, conflicts_with = "project")]
    project_name: Option<String>,
    /// Print one line per record and a summary even for one file, as for several; a script then
    /// reads one document shape whatever it matched
    #[arg(long)]
    summary: bool,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct SameReleaseArgs {
    /// Two or more refs in the order to compare: snapshot files, uids, or <name>@<version>
    #[arg(required = true, num_args = 2..)]
    records: Vec<String>,
    /// Accept one kind of difference between consecutive records
    #[arg(long, value_enum)]
    allow: Option<Allow>,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct ConsumersArgs {
    /// Profile uid, <name>@<version> or <name>@latest, or a profile record (its uid is fetched live)
    r#ref: String,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct ListArgs {
    /// What to list: profiles, clusters, projects. Without one: profiles and clusters, or only
    /// profiles with --pack
    #[arg(value_enum)]
    kinds: Vec<KindArg>,
    /// List profiles and clusters in this project instead of the configured one. Projects are
    /// always listed at tenant scope
    #[arg(long)]
    project: Option<String>,
    /// Like --project, with the project's name
    #[arg(long, conflicts_with = "project")]
    project_name: Option<String>,
    /// Read each profile version for its type, consumer count, and packs; one request each
    #[arg(long)]
    packs: bool,
    /// Keep the profile versions with a pack whose name contains TEXT, ignoring case; implies --packs
    #[arg(long, value_name = "TEXT")]
    pack: Option<String>,
    #[command(flatten)]
    out: OutputArgs,
}

/// What `list` can list.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum KindArg {
    Profiles,
    Clusters,
    Projects,
}

impl From<KindArg> for ListKind {
    fn from(k: KindArg) -> ListKind {
        match k {
            KindArg::Profiles => ListKind::Profiles,
            KindArg::Clusters => ListKind::Clusters,
            KindArg::Projects => ListKind::Projects,
        }
    }
}

/// Differences `same-release` can be told to accept.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Allow {
    /// Packs re-pinned from another registry with the same tag, version, values, and manifests
    RegistryMove,
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
    VerifySummary,
    Chain,
    Consumers,
    List,
    Candidate,
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
        Cmd::Candidate(a) => cmd_candidate(&a),
        Cmd::Verify(a) => cmd_verify(&a),
        Cmd::SameRelease(a) => cmd_same_release(&a),
        Cmd::Consumers(a) => cmd_consumers(&a),
        Cmd::List(a) => cmd_list(&a),
        Cmd::Schema { kind } => cmd_schema(kind),
    }
}

fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn progress(quiet: bool) -> Progress {
    Progress::new(quiet || std::env::var_os("STRATA_QUIET").is_some())
}

/// A client from the environment; `project` replaces `PALETTE_PROJECT_UID` or `PALETTE_PROJECT`
/// when given, and an empty one is tenant scope.
fn connect(progress: Progress, project: Option<&str>) -> Result<Palette<Retry<HttpApi>>> {
    let mut config = Config::from_env()?;
    match project {
        Some(p) => config.project_uid = Some(p.to_string()).filter(|p| !p.is_empty()),
        None => {
            if let Some(name) = config.project_name.clone() {
                config.project_uid = Some(project_uid_named(&config, &name, progress)?);
            }
        }
    }
    Ok(client(config, progress))
}

fn client(config: Config, progress: Progress) -> Palette<Retry<HttpApi>> {
    let policy = RetryPolicy::from_config(&config);
    Palette::new(Retry::new(HttpApi::new(config), policy, progress))
}

/// The uid of the project named `name`, looked up at tenant scope once per run.
fn project_uid_named(config: &Config, name: &str, progress: Progress) -> Result<String> {
    static RESOLVED: OnceLock<(String, String)> = OnceLock::new();
    if let Some((_, uid)) = RESOLVED.get().filter(|(n, _)| n == name) {
        return Ok(uid.clone());
    }
    let mut tenant = config.clone();
    tenant.project_uid = None;
    progress.note(&format!("looking up project {name}"));
    let uid = client(tenant, progress).project_uid_named(name)?;
    let _ = RESOLVED.set((name.to_string(), uid.clone()));
    Ok(uid)
}

/// `--project`, or the uid of the project `--project-name` names.
fn project_arg(
    progress: Progress,
    uid: &Option<String>,
    name: &Option<String>,
) -> Result<Option<String>> {
    match name {
        Some(n) => Ok(Some(project_uid_named(&Config::from_env()?, n, progress)?)),
        None => Ok(uid.clone()),
    }
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
    let sa = ops::snapshot_for(
        &Ref::parse(&a.a),
        || connect(progress, None),
        &now,
        &progress,
    )?;
    let sb = ops::snapshot_for(
        &Ref::parse(&a.b),
        || connect(progress, None),
        &now,
        &progress,
    )?;
    let d = ops::compare(&sa, &sb, &now);
    print!("{}", a.out.renderer().render(&d, &sa, &sb, a.full)?);
    Ok(exit_for(d.same))
}

fn cmd_snapshot(a: &SnapshotArgs) -> Result<ExitCode> {
    let progress = progress(a.quiet);
    let now = now_utc();
    let snap = match (&a.from_candidate, &a.from_raw) {
        (Some(file), _) => {
            ops::snapshot_from_candidate(file, || connect(progress, None), &now, &progress)?
        }
        (None, Some(dir)) => {
            let reference = a.r#ref.clone().unwrap_or_else(|| dir.display().to_string());
            ops::snapshot_from_dir(dir, &reference, &now)?
        }
        (None, None) => {
            let text = a.r#ref.as_deref().ok_or_else(|| Error::Transport {
                path: String::new(),
                message: "snapshot needs a profile ref or --from-raw DIR".to_string(),
            })?;
            let reference = Ref::parse(text);
            let palette = connect(progress, None)?;
            let snap = match &reference {
                Ref::Cluster { uid, profile } => {
                    let raw = palette.fetch_cluster(uid, &progress)?;
                    if let Some(dir) = &a.raw_dir {
                        raw.save_dir(dir)?;
                    }
                    Snapshot::from_cluster(&raw, profile.as_deref(), text, &now)?
                }
                Ref::Snapshot(_) | Ref::Uid(_) | Ref::NameVersion { .. } => {
                    let uid = match &reference {
                        Ref::NameVersion { name, version } => palette.resolve(name, version)?,
                        _ => text.to_string(),
                    };
                    let raw = palette.fetch_profile(&uid, &progress)?;
                    if let Some(dir) = &a.raw_dir {
                        raw.save_dir(dir)?;
                    }
                    Snapshot::from_raw(&raw, text, &now)?
                }
            };
            snap.with_project(palette.project_uid())
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

fn is_pattern(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

/// The record files named by the arguments, with patterns expanded by strata so a quoted glob
/// works from any shell or scheduler. A pattern that matches nothing is an error.
fn expand_records(args: &[String]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for arg in args {
        if !is_pattern(arg) {
            out.push(PathBuf::from(arg));
            continue;
        }
        let matches: Vec<PathBuf> = glob::glob(arg)
            .map_err(|e| Error::Pattern {
                pattern: arg.clone(),
                message: format!("is not a valid pattern: {}", e.msg),
            })?
            .filter_map(std::result::Result::ok)
            .filter(|p| p.is_file())
            .collect();
        if matches.is_empty() {
            return Err(Error::Pattern {
                pattern: arg.clone(),
                message: "matched no files".to_string(),
            });
        }
        out.extend(matches);
    }
    Ok(out)
}

fn cmd_candidate(a: &CandidateArgs) -> Result<ExitCode> {
    let progress = progress(a.quiet);
    let reference = Ref::parse(&a.r#ref);
    if let Ref::Cluster { .. } = reference {
        return Err(Error::ProfileRefRequired {
            reference: a.r#ref.clone(),
        });
    }
    let snap = ops::snapshot_for(
        &reference,
        || connect(progress, None),
        &now_utc(),
        &progress,
    )?;
    let c = candidate::Candidate::from_snapshot(&snap)?;
    match &a.output {
        Some(path) => {
            candidate::write(&c, path, a.files_dir.as_deref())?;
            progress.note(&format!(
                "wrote {} (root {})",
                path.display(),
                &snap.root[..12]
            ));
        }
        None => print!("{}", candidate::to_yaml(&c)?),
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_verify(a: &VerifyArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let paths = expand_records(&a.snapshots)?;
    let project = project_arg(progress, &a.project, &a.project_name)?;
    let bulk = a.summary || a.snapshots.len() > 1 || a.snapshots.iter().any(|s| is_pattern(s));
    if bulk {
        let s = ops::verify_many(
            &paths,
            project.as_deref(),
            |scope| connect(progress, scope),
            &now_utc(),
            &progress,
        );
        print!("{}", a.out.renderer().render_verify_summary(&s)?);
        return Ok(ExitCode::from(s.exit_code()));
    }
    let recorded = Snapshot::load(&paths[0])?;
    let scope = project
        .as_deref()
        .or(Some(recorded.source.project_uid.as_str()).filter(|p| !p.is_empty()));
    let palette = connect(progress, scope)?;
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

fn cmd_same_release(a: &SameReleaseArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let now = now_utc();
    let mut records = Vec::with_capacity(a.records.len());
    for r in &a.records {
        let snap = ops::snapshot_for(&Ref::parse(r), || connect(progress, None), &now, &progress)?;
        records.push((r.clone(), snap));
    }
    let policy = match a.allow {
        Some(Allow::RegistryMove) => Policy::RegistryMove,
        None => Policy::Exact,
    };
    let c = ops::same_release(&records, policy, &now);
    print!("{}", a.out.renderer().render_chain(&c)?);
    Ok(ExitCode::from(c.exit_code()))
}

fn cmd_consumers(a: &ConsumersArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let r = ops::consumers(
        &Ref::parse(&a.r#ref),
        |scope| connect(progress, scope),
        &now_utc(),
        &progress,
    )?;
    print!("{}", a.out.renderer().render_consumers(&r)?);
    Ok(ExitCode::from(r.exit_code()))
}

fn cmd_list(a: &ListArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let options = ListOptions {
        kinds: a.kinds.iter().map(|&k| k.into()).collect(),
        project: project_arg(progress, &a.project, &a.project_name)?,
        packs: a.packs,
        pack: a.pack.clone(),
    };
    let l = ops::list(
        &options,
        |scope| connect(progress, scope),
        &now_utc(),
        &progress,
    )?;
    print!("{}", a.out.renderer().render_list(&l)?);
    Ok(ExitCode::SUCCESS)
}

fn cmd_schema(kind: SchemaKind) -> Result<ExitCode> {
    let schema = match kind {
        SchemaKind::Snapshot => schemars::schema_for!(Snapshot),
        SchemaKind::Diff => schemars::schema_for!(diff::Diff),
        SchemaKind::VerifySummary => schemars::schema_for!(ops::VerifySummary),
        SchemaKind::Chain => schemars::schema_for!(ops::Chain),
        SchemaKind::Consumers => schemars::schema_for!(ops::ConsumersReport),
        SchemaKind::List => schemars::schema_for!(ops::Listing),
        SchemaKind::Candidate => schemars::schema_for!(strata::candidate::Candidate),
    };
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(ExitCode::SUCCESS)
}
