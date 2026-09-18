//! The strata command line: argument parsing, printing, and exit codes. Everything else is in the
//! library.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::OnceLock;

use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};

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

/// Every command, in the order that `strata --help` lists them. A first argument that is not one
/// of these makes `strata <A> <B>` a compare.
const COMMANDS: &[&str] = &[
    "candidate",
    "compare",
    "consumers",
    "help",
    "list",
    "packs",
    "same-release",
    "schema",
    "snapshot",
    "verify",
];

#[derive(Parser)]
#[command(
    name = TOOL_NAME,
    version,
    about = "Record, compare, and check Palette profile versions, clusters, and pack versions",
    override_usage = "strata <COMMAND>\n       strata <A> <B>"
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
            Cmd::Packs(a) => a.out.format() == Format::Json,
            Cmd::Snapshot(_) | Cmd::Candidate(_) | Cmd::Schema { .. } => false,
        }
    }
}

#[derive(Subcommand)]
// In alphabetical order, which is the order `strata --help` lists them in.
enum Cmd {
    /// Write a candidate file from a profile version or a record
    Candidate(CandidateArgs),
    /// Compare two profile versions, records, or clusters
    Compare(CompareArgs),
    /// List the clusters that use a profile version
    Consumers(ConsumersArgs),
    /// List the profile versions, clusters, and projects in the scope
    List(ListArgs),
    /// List the versions a registry offers for a pack
    Packs(PacksArgs),
    /// Make sure that two or more records are one release
    SameRelease(SameReleaseArgs),
    /// Print the JSON Schema of a document or of the candidate file
    Schema {
        /// Which document, or the candidate file
        #[arg(value_enum)]
        kind: SchemaKind,
    },
    /// Record a profile version or a cluster in a snapshot file
    Snapshot(SnapshotArgs),
    /// Make sure that each record agrees with Palette now
    Verify(VerifyArgs),
}

#[derive(Args)]
struct OutputArgs {
    /// The report format: terminal, markdown, or json
    #[arg(
        long,
        value_enum,
        default_value = "terminal",
        hide_possible_values = true
    )]
    format: FormatArg,
    /// The same as --format json
    #[arg(long)]
    json: bool,
    /// Color the report: auto, always, or never
    #[arg(long, value_enum, default_value = "auto", num_args = 0..=1,
          default_missing_value = "always", hide_possible_values = true)]
    color: ColorArg,
    /// The same as --color=never
    #[arg(long)]
    no_color: bool,
    /// Write the JSON on one line
    #[arg(long)]
    compact: bool,
    /// No progress messages
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
    /// The first side: a ref or a snapshot file
    a: String,
    /// The second side, in the same forms as the first
    b: String,
    /// Also print the unified diffs of the values and the manifests
    #[arg(long)]
    full: bool,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct SnapshotArgs {
    /// What to record: a ref, or the source with --from-raw
    r#ref: Option<String>,
    /// Write to this file, not to standard output
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Also save the raw API answers in this directory
    #[arg(long)]
    raw_dir: Option<PathBuf>,
    /// Read the raw answers of --raw-dir, with no network
    #[arg(long)]
    from_raw: Option<PathBuf>,
    /// Read a candidate file, before the version is in Palette
    #[arg(long, conflicts_with_all = ["from_raw", "raw_dir", "ref"])]
    from_candidate: Option<PathBuf>,
    /// Write the JSON on one line
    #[arg(long)]
    compact: bool,
    /// No progress messages
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Args)]
struct CandidateArgs {
    /// A profile version, by ref or by record
    r#ref: String,
    /// Write to this file, not to standard output
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Write the values and the manifests to files in this directory
    #[arg(long, requires = "output")]
    files_dir: Option<PathBuf>,
    /// No progress messages
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Args)]
struct VerifyArgs {
    /// Snapshot files; quote a pattern such as 'releases/**/*.json'
    #[arg(required = true)]
    snapshots: Vec<String>,
    /// Check the records in this project, by uid
    #[arg(long)]
    project: Option<String>,
    /// The same, by project name
    #[arg(long, conflicts_with = "project")]
    project_name: Option<String>,
    /// One line for each record and a summary, also for one file
    #[arg(long)]
    summary: bool,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct SameReleaseArgs {
    /// Two or more refs, in promotion order
    #[arg(required = true, num_args = 2..)]
    records: Vec<String>,
    /// Pass a pair that differs only in this way
    #[arg(long, value_enum)]
    allow: Option<Allow>,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct ConsumersArgs {
    /// A profile version, by ref or by record
    r#ref: String,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct PacksArgs {
    /// The pack name, as the registry holds it
    name: String,
    /// Only this registry, by name or uid
    #[arg(long, value_name = "NAME")]
    registry: Option<String>,
    /// Keep the versions of this cloud, such as aws
    #[arg(long, value_name = "TYPE")]
    cloud: Option<String>,
    /// Only the newest version of each registry
    #[arg(long)]
    latest: bool,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct ListArgs {
    /// What to list; with none, profiles and clusters
    #[arg(value_enum)]
    kinds: Vec<KindArg>,
    /// List this project, by uid; '' is tenant scope
    #[arg(long)]
    project: Option<String>,
    /// The same, by project name
    #[arg(long, conflicts_with = "project")]
    project_name: Option<String>,
    /// Also read each version for its packs and consumers
    #[arg(long)]
    packs: bool,
    /// Keep the versions with a pack whose name holds TEXT; sets --packs
    #[arg(long, value_name = "TEXT")]
    pack: Option<String>,
    /// Also read each cluster for the profiles it applies
    #[arg(long)]
    applied: bool,
    /// Keep the clusters whose name or uid holds TEXT; sets --applied
    #[arg(long, value_name = "TEXT")]
    cluster: Option<String>,
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
    /// Packs from another registry, with the same tag, version, values, and manifests
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
    Packs,
    Candidate,
}

/// The command line. The help screen is the one a terminal expects: the description, the usage,
/// the commands, the flags, and one line that says where to read more. The refs, the settings and
/// the exit status are in the book, and in the help of each command.
fn command() -> clap::Command {
    // clap's own help flag carries long text, and its long layout gives each option two lines.
    // This one says a line and prints the short layout. disable_help_flag reaches the
    // subcommands, so each one gets the same flag.
    let help_flag = || {
        clap::Arg::new("help")
            .short('h')
            .long("help")
            .action(clap::ArgAction::HelpShort)
            .help("Print help")
    };
    let mut cmd = Cli::command()
        .disable_help_flag(true)
        .disable_version_flag(true)
        .arg(help_flag())
        .arg(
            clap::Arg::new("version")
                .short('V')
                .long("version")
                .action(clap::ArgAction::Version)
                .help("Print version"),
        )
        .after_help("Use \"strata <command> --help\" for more information about a command.");
    for name in COMMANDS {
        if *name == "help" {
            continue;
        }
        cmd = cmd.mut_subcommand(name, |s| s.arg(help_flag()));
    }
    cmd
}

/// True when `arg` names a command, so `strata <A> <B>` does not become `strata compare`.
fn is_command(arg: &str) -> bool {
    COMMANDS.contains(&arg)
}

fn main() -> ExitCode {
    let mut argv: Vec<String> = std::env::args().collect();
    let first = argv.get(1).map(String::as_str).unwrap_or("");
    let is_global_flag = matches!(first, "-h" | "--help" | "-V" | "--version");
    if argv.len() > 1 && !is_command(first) && !is_global_flag {
        argv.insert(1, "compare".to_string());
    }
    let matches = command().get_matches_from(argv);
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
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
        Cmd::Packs(a) => cmd_packs(&a),
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
                Ref::Pack {
                    registry,
                    name,
                    version,
                } => strata::candidate::pack_snapshot(
                    &palette,
                    registry.as_deref(),
                    name,
                    version,
                    text,
                    &now,
                    &progress,
                )?,
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
    if let Ref::Cluster { .. } | Ref::Pack { .. } = reference {
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
        applied: a.applied,
        cluster: a.cluster.clone(),
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

fn cmd_packs(a: &PacksArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let options = ops::PackOptions {
        registry: a.registry.clone(),
        cloud: a.cloud.clone(),
        latest: a.latest,
    };
    let listing = ops::registry_packs(
        &a.name,
        &options,
        |scope| connect(progress, scope),
        &now_utc(),
        &progress,
    )?;
    print!("{}", a.out.renderer().render_packs(&listing)?);
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
        SchemaKind::Packs => schemars::schema_for!(ops::PackListing),
        SchemaKind::Candidate => schemars::schema_for!(strata::candidate::Candidate),
    };
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_names_every_command_the_binary_has() {
        let mut built = command();
        built.build();
        let mut commands: Vec<&str> = built.get_subcommands().map(|c| c.get_name()).collect();
        commands.sort_unstable();
        let mut listed: Vec<&str> = COMMANDS.to_vec();
        listed.sort_unstable();
        assert_eq!(listed, commands, "COMMANDS and the binary disagree");
    }

    #[test]
    fn the_help_screen_is_the_one_a_terminal_expects() {
        let help = command().render_help().to_string();
        let at = |text: &str| {
            help.find(text)
                .unwrap_or_else(|| panic!("no {text:?} in:\n{help}"))
        };
        assert!(at("Usage:") < at("Commands:"), "the usage comes first");
        assert!(at("Commands:") < at("Options:"), "then the commands");
        assert!(
            at("Options:") < at("Use \"strata <command> --help\""),
            "and one line at the end says where to read more"
        );
        for name in COMMANDS {
            assert!(help.contains(&format!("  {name} ")), "{name} is not listed");
        }
        // the screen fits a terminal, and every option is one line
        assert!(
            help.lines().count() < 30,
            "the help screen grew to {} lines:\n{help}",
            help.lines().count()
        );
        assert!(!help.contains("Merkle"));
    }
}
