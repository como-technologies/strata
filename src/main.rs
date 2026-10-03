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
use strata::{TOOL_NAME, candidate, diff, ops};

/// Every command, in the order that `strata --help` lists them. A first argument that is not one
/// of these makes `strata <A> <B>` a compare.
const COMMANDS: &[&str] = &[
    "behind",
    "candidate",
    "compare",
    "consumers",
    "help",
    "list",
    "packs",
    "preflight",
    "same-release",
    "schema",
    "snapshot",
    "verify",
];

/// The binary that `strata preflight` runs. It is a package of its own, `strata-preflight`, so the
/// strata crate stays free of its dependencies; a release ships both binaries side by side.
const PREFLIGHT_BIN: &str = "strata-preflight";

#[derive(Parser)]
#[command(
    name = TOOL_NAME,
    version,
    about = "Record, compare, and check Palette profile versions, the clusters and other objects that run them, and pack versions",
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
            Cmd::Behind(a) => a.out.format() == Format::Json,
            Cmd::Preflight(a) => {
                a.args.iter().any(|x| x == "--json" || x == "--format=json")
                    || a.args
                        .windows(2)
                        .any(|w| w[0] == "--format" && w[1] == "json")
            }
            Cmd::Snapshot(a) => a.format() == Format::Json,
            Cmd::Candidate(_) | Cmd::Schema { .. } => false,
        }
    }
}

#[derive(Subcommand)]
// In alphabetical order, which is the order `strata --help` lists them in.
enum Cmd {
    /// Report the pack versions that records pin, and the newer versions the registries offer
    Behind(BehindArgs),
    /// Write a candidate file from a profile version or a record
    Candidate(CandidateArgs),
    /// Compare two profile versions, records, clusters, edge hosts, cluster groups, app profiles,
    /// or pack versions
    Compare(CompareArgs),
    /// List the clusters, edge hosts, cluster templates, and app deployments that use a profile
    /// version
    Consumers(ConsumersArgs),
    /// List the profile versions, clusters, projects, and saved tag filters in the scope
    List(ListArgs),
    /// List the versions a registry offers for a pack
    Packs(PacksArgs),
    /// Check that Palette can build a cluster, before the build; runs strata-preflight
    Preflight(PreflightArgs),
    /// Make sure that two or more records are one release
    SameRelease(SameReleaseArgs),
    /// Print the JSON Schema of a document or of the candidate file
    Schema {
        /// Which document, or the candidate file
        #[arg(value_enum)]
        kind: SchemaKind,
    },
    /// Record a profile version, a cluster, an edge host, a cluster group, an app profile, or a
    /// pack version in a snapshot file, or each cluster of a tag: or filter: set in a directory
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
    #[arg(long, conflicts_with = "format")]
    json: bool,
    /// Color the report: auto, always, or never
    #[arg(long, value_enum, default_value = "auto", num_args = 0..=1,
          default_missing_value = "always", hide_possible_values = true)]
    color: ColorArg,
    /// The same as --color=never
    #[arg(long, conflicts_with = "color")]
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
    /// Ask Palette if the change repaves the cluster side; one side must be a cluster
    #[arg(long)]
    repave: bool,
    /// Ask Palette if it accepts the packs of the change on the cluster side; one side must be a
    /// cluster
    #[arg(long)]
    validate: bool,
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
    #[arg(long, conflicts_with = "raw_dir")]
    from_raw: Option<PathBuf>,
    /// Read a candidate file, before the version is in Palette
    #[arg(long, conflicts_with_all = ["from_raw", "raw_dir", "ref"])]
    from_candidate: Option<PathBuf>,
    /// Ask Palette whether it takes the packs: of a candidate as a new profile, or of a profile
    /// version as an update of itself. A refusal exits 2 and writes no record
    #[arg(long)]
    validate: bool,
    /// Write the JSON on one line
    #[arg(long)]
    compact: bool,
    /// For a cluster: record the digest of the value Palette gives each macro the content uses
    #[arg(long, conflicts_with_all = ["from_raw", "from_candidate"])]
    macros: bool,
    /// For a cluster: record the shape tier, the machine pools and the cloud part, with one more
    /// request
    #[arg(long, conflicts_with_all = ["from_raw", "from_candidate"])]
    shape: bool,
    /// For tag:KEY=VALUE or filter:NAME: write one record for each cluster into this directory,
    /// as <cluster name>.json
    #[arg(long, value_name = "DIR", conflicts_with_all = ["output", "raw_dir", "from_raw", "from_candidate"])]
    dir: Option<PathBuf>,
    /// With --dir: remove each cluster record of the directory whose cluster the query no
    /// longer selects
    #[arg(long, requires = "dir")]
    prune: bool,
    /// With --dir: the report format: terminal, markdown, or json
    #[arg(long, value_enum, requires = "dir", hide_possible_values = true)]
    format: Option<FormatArg>,
    /// With --dir: the same as --format json
    #[arg(long, conflicts_with = "format", requires = "dir")]
    json: bool,
    /// No progress messages
    #[arg(short, long)]
    quiet: bool,
}

impl SnapshotArgs {
    /// The format of the report of a set. A record is always JSON.
    fn format(&self) -> Format {
        if self.json {
            Format::Json
        } else {
            self.format.map_or(Format::Terminal, Format::from)
        }
    }
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
    /// For a record that drifted, read who changed it, and when, from the audit log of Palette
    #[arg(long)]
    who: bool,
    /// Accept a pack with no upstream digest: exit 0 and not 1, and still name it
    #[arg(long)]
    allow_no_digest: bool,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct SameReleaseArgs {
    /// Two or more refs, in promotion order
    #[arg(required = true, num_args = 2..)]
    records: Vec<String>,
    /// Pass a pair that differs only in this way; give it more than one time for more
    #[arg(long, value_enum)]
    allow: Vec<Allow>,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct BehindArgs {
    /// Record files; quote a pattern such as 'releases/**/*.json'
    #[arg(required = true)]
    records: Vec<String>,
    /// The JSON of a previous run: each pin names the versions that arrived since
    #[arg(long, value_name = "FILE")]
    since: Option<PathBuf>,
    /// Do not compare each pin that is behind with the newest version, for the impact
    #[arg(long)]
    no_impact: bool,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct ConsumersArgs {
    /// A profile version or an app profile version, by ref or by record
    r#ref: String,
    /// Read the consumer list that Palette computes, and not each cluster of the fleet. Faster,
    /// but it does not find a consumer that the list omits
    #[arg(long)]
    list_only: bool,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
#[command(disable_help_flag = true)]
struct PreflightArgs {
    /// The arguments of strata-preflight, as `strata-preflight --help` lists them
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<std::ffi::OsString>,
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
    /// Keep the clusters with this label, KEY=VALUE or KEY; give it again for each label.
    /// Palette searches every project the key can read
    #[arg(long, value_name = "KEY=VALUE")]
    tag: Vec<String>,
    /// Keep the clusters that a saved tag filter selects, by name or uid
    #[arg(long, value_name = "NAME")]
    filter: Option<String>,
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
    Filters,
}

impl From<KindArg> for ListKind {
    fn from(k: KindArg) -> ListKind {
        match k {
            KindArg::Profiles => ListKind::Profiles,
            KindArg::Clusters => ListKind::Clusters,
            KindArg::Projects => ListKind::Projects,
            KindArg::Filters => ListKind::Filters,
        }
    }
}

/// Differences `same-release` can be told to accept.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Allow {
    /// Packs from another registry, with the same tag, version, values, and manifests
    RegistryMove,
    /// A pack with no upstream digest, whose archive the root cannot show
    NoDigest,
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
    Behind,
    Snapshot,
    Diff,
    VerifySummary,
    Chain,
    Consumers,
    List,
    Packs,
    Set,
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
        .after_help(concat!(
            "Use \"strata <command> --help\" for more information about a command.\n",
            "The book: https://legendary-train-pzq9ojj.pages.github.io/"
        ));
    for name in COMMANDS {
        // `strata preflight --help` is the help of strata-preflight, so it goes through
        if *name == "help" || *name == "preflight" {
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
        // `strata <A> <B>` is a compare, so a command name that is not first would become a ref
        // and fail as an unexpected argument of `compare`. Say what is wrong instead.
        if let Some(name) = argv[2..].iter().find(|a| is_command(a)) {
            command()
                .error(
                    clap::error::ErrorKind::InvalidSubcommand,
                    format!("'{name}' is a command, so it comes first: strata {name} ..."),
                )
                .exit();
        }
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
        Cmd::Behind(a) => cmd_behind(&a),
        Cmd::List(a) => cmd_list(&a),
        Cmd::Packs(a) => cmd_packs(&a),
        Cmd::Preflight(a) => cmd_preflight(&a),
        Cmd::Schema { kind } => cmd_schema(kind),
    }
}

/// `strata preflight ...` runs the `strata-preflight` binary with the same arguments: the one
/// beside this binary first, then the one on the PATH. Its exit status is the exit status.
fn cmd_preflight(a: &PreflightArgs) -> Result<ExitCode> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(PREFLIGHT_BIN)))
        .filter(|p| p.is_file());
    let program = beside.unwrap_or_else(|| PathBuf::from(PREFLIGHT_BIN));
    let status = std::process::Command::new(&program)
        .args(&a.args)
        .status()
        .map_err(|e| Error::Usage {
            message: format!(
                "cannot run {PREFLIGHT_BIN} ({e}): install it beside strata, or on the PATH; \
                 the installer of a release puts both in one directory"
            ),
        })?;
    Ok(match status.code() {
        Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(2)),
        None => ExitCode::from(2),
    })
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
            warn_on_scope_conflict(&config, progress);
        }
    }
    Ok(client(config, progress))
}

/// `PALETTE_PROJECT_UID` and `PALETTE_PROJECT` were both set, so the name decided nothing. Say so
/// when the two name two projects, because every answer that follows is about the uid's project
/// and the caller asked for the name's.
///
/// This costs one listing, and only when both are set. It says nothing when the two agree, which
/// is the common setting: the playground prints both lines, and so do many `~/.palette.env`
/// files. A name that resolves to nothing is a warning too, and not an error: a stale variable
/// that decides nothing must not fail a command that asked about something else.
///
/// A quiet run skips the listing as well as the warning. The warning is the only reason to make
/// that request, and with `--json` a quiet run writes one JSON object to stderr and nothing else,
/// which a caller reads.
fn warn_on_scope_conflict(config: &Config, progress: Progress) {
    if progress.is_quiet() {
        return;
    }
    let (Some(uid), Some(name)) = (&config.project_uid, &config.shadowed_project_name) else {
        return;
    };
    match project_uid_named(config, name, progress) {
        Ok(named) if &named == uid => {}
        Ok(named) => progress.warn(&format!(
            "PALETTE_PROJECT_UID is {uid} and PALETTE_PROJECT is '{name}', which is {named}. \
             A uid has priority, so this answers for {uid}. Clear one of the two: \
             `PALETTE_PROJECT_UID= PALETTE_PROJECT={name}` asks for the project you named."
        )),
        Err(e) => progress.warn(&format!(
            "PALETTE_PROJECT is '{name}', which no project matches ({e}). \
             PALETTE_PROJECT_UID is {uid}, and a uid has priority, so this answers for {uid}."
        )),
    }
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
    let (sa, sb) = ops::snapshot_pair(
        &Ref::parse(&a.a),
        &Ref::parse(&a.b),
        || connect(progress, None),
        &now,
        &progress,
    )?;
    let mut d = ops::compare(&sa, &sb, &now);
    if a.repave || a.validate {
        // the cluster side names its project; the questions go to that project
        let scope = [&sa, &sb]
            .into_iter()
            .find(|s| s.source.cluster.is_some())
            .map(|s| s.source.project_uid.clone())
            .filter(|p| !p.is_empty());
        let palette = connect(progress, scope.as_deref())?;
        if a.repave {
            d.palette_repave = Some(ops::palette_repave(
                &palette,
                &sa,
                &sb,
                d.summary.max_impact,
            )?);
        }
        if a.validate {
            d.palette_validation = Some(ops::palette_validation(&palette, &sa, &sb)?);
        }
    }
    print!("{}", a.out.renderer().render(&d, &sa, &sb, a.full)?);
    Ok(exit_for(d.nothing_differs()))
}

fn cmd_snapshot(a: &SnapshotArgs) -> Result<ExitCode> {
    let progress = progress(a.quiet);
    let now = now_utc();
    if a.validate && (a.from_raw.is_some() || a.dir.is_some()) {
        return Err(Error::Usage {
            message: "--validate asks Palette about the packs of one profile version or \
                      candidate. --from-raw sends no request, and --dir records clusters"
                .to_string(),
        });
    }
    // a set of clusters is one record for each, in a directory
    let set = a.r#ref.as_deref().map(Ref::parse);
    match (&set, &a.dir) {
        (Some(Ref::ClusterSet { tags, filter }), Some(dir)) => {
            let query = ops::ClusterQuery {
                tags: tags.clone(),
                filter: filter.clone(),
            };
            let report = ops::snapshot_set(
                &query,
                dir,
                a.prune,
                |scope| connect(progress, scope),
                &now,
                &progress,
            )?;
            // the lines of a set have no color, so the report takes no --color
            let renderer = render::renderer(a.format(), Paint::off(), a.compact);
            print!("{}", renderer.render_set(&report)?);
            progress.note(&render::set_counts_line(&report));
            return Ok(ExitCode::from(report.exit_code()));
        }
        (Some(Ref::ClusterSet { .. }), None) => {
            return Err(Error::Usage {
                message: "a tag: or filter: ref names a set of clusters: give --dir DIR, and \
                          strata writes one record for each"
                    .to_string(),
            });
        }
        (_, Some(_)) => {
            return Err(Error::Usage {
                message: "--dir takes a tag: or filter: ref, which names a set of clusters"
                    .to_string(),
            });
        }
        _ => {}
    }
    let snap = match (&a.from_candidate, &a.from_raw) {
        (Some(file), _) => ops::snapshot_from_candidate(
            file,
            a.validate,
            || connect(progress, None),
            &now,
            &progress,
        )?,
        (None, Some(dir)) => {
            let reference = a.r#ref.clone().unwrap_or_else(|| dir.display().to_string());
            ops::snapshot_from_dir(dir, &reference, &now)?
        }
        (None, None) => {
            let text = a.r#ref.as_deref().ok_or_else(|| Error::Usage {
                message: "snapshot needs a profile ref or --from-raw DIR".to_string(),
            })?;
            let reference = Ref::parse(text);
            if a.macros && !matches!(reference, Ref::Cluster { .. }) {
                return Err(Error::Usage {
                    message: "--macros needs a cluster ref: a cluster resolves the macros, and \
                              a profile does not"
                        .to_string(),
                });
            }
            if a.validate && matches!(reference, Ref::Draft { .. }) {
                return Err(Error::Usage {
                    message: format!(
                        "--validate takes a profile version or --from-candidate, not a draft \
                         ({reference}): give the candidate of the draft, from `strata candidate \
                         {reference}`, to --from-candidate"
                    ),
                });
            }
            if a.validate && !matches!(reference, Ref::Uid(_) | Ref::NameVersion { .. }) {
                return Err(Error::Usage {
                    message: "--validate takes a profile version or --from-candidate: Palette \
                              checks the packs of a profile, and not of a cluster, a cluster \
                              group, an app profile, or one pack"
                        .to_string(),
                });
            }
            if a.shape && !matches!(reference, Ref::Cluster { .. }) {
                return Err(Error::Usage {
                    message: "--shape needs a cluster ref: a profile has no machine pools"
                        .to_string(),
                });
            }
            let flagged = matches!(
                reference,
                Ref::Cluster { pending: true, .. }
                    | Ref::Draft { .. }
                    | Ref::EdgeHost { .. }
                    | Ref::ClusterGroup { .. }
                    | Ref::AppProfile { .. }
            );
            if a.raw_dir.is_some() && flagged {
                return Err(Error::Usage {
                    message: "--raw-dir takes a profile or a cluster: a record read back from \
                              the directory does not say pending, draft, edge host, cluster \
                              group, or app profile"
                        .to_string(),
                });
            }
            let palette = connect(progress, None)?;
            let snap = match &reference {
                Ref::Cluster {
                    uid,
                    profile,
                    pending,
                } => {
                    let mut raw = palette.fetch_cluster(uid, &progress)?;
                    if *pending {
                        raw = palette.with_pending(raw)?;
                    }
                    if a.macros {
                        let uid = raw
                            .cluster
                            .pointer("/metadata/uid")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(uid)
                            .to_string();
                        raw.resolved_values = Some(palette.resolved_values(&uid)?);
                    }
                    if a.shape {
                        raw.cloud_config = Some(palette.cloud_config(&raw.cluster)?);
                    }
                    if let Some(dir) = &a.raw_dir {
                        raw.save_dir(dir, &now)?;
                    }
                    let snap = Snapshot::from_cluster(&raw, profile.as_deref(), text, &now)?;
                    if *pending { snap.mark_pending() } else { snap }
                }
                Ref::ClusterGroup { uid } => {
                    ops::cluster_group_snapshot(&palette, uid, text, &now, &progress)?
                }
                Ref::ClusterSet { .. } => unreachable!("a set of clusters returned above"),
                Ref::AppProfile { id, version } => ops::app_profile_snapshot(
                    &palette,
                    id,
                    version.as_deref(),
                    text,
                    &now,
                    &progress,
                )?,
                Ref::EdgeHost { uid, profile } => {
                    let raw = palette.fetch_edge_host(uid, &progress)?;
                    Snapshot::from_cluster(&raw, profile.as_deref(), text, &now)?
                        .with_kind(strata::snapshot::SubjectKind::EdgeHost)
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
                // a file-shaped ref reaches Palette nowhere else, and a path sent as a uid asks
                // for /v1/clusterprofiles/<path>, which 404s after the retries
                Ref::Snapshot(path) => {
                    return Err(Error::RecordNotASource {
                        path: path.display().to_string(),
                    });
                }
                Ref::Draft { id, version } => {
                    ops::draft_snapshot(&palette, id, version.as_deref(), text, &now, &progress)?
                }
                Ref::Uid(_) | Ref::NameVersion { .. } => {
                    let uid = match &reference {
                        Ref::NameVersion { name, version } => palette.resolve(name, version)?,
                        _ => text.to_string(),
                    };
                    let raw = palette.fetch_profile(&uid, &progress)?;
                    if let Some(dir) = &a.raw_dir {
                        raw.save_dir(dir, &now)?;
                    }
                    let mut snap = Snapshot::from_raw(&raw, text, &now)?;
                    if a.validate {
                        snap.source.palette_validation =
                            Some(ops::profile_validation(&palette, &snap)?);
                    }
                    snap
                }
            };
            snap.with_project(palette.project_uid())
        }
    };
    // the answer of Palette is what --validate asks for, so -q does not hide it; it goes to
    // standard error, beside the record on standard output
    if let Some(v) = &snap.source.palette_validation {
        for line in strata::validation::answer_lines(&v.errors) {
            eprintln!("palette    {line}");
        }
    }
    ops::require_acceptance(&snap)?;
    // the root does not cover the flag, so the record alone does not show it to a person
    for p in snap.packs.iter().filter(|p| p.invalid) {
        let reason = Some(p.invalid_reason.as_str()).filter(|r| !r.is_empty());
        progress.warn(&format!(
            "Palette marks pack {} invalid{}. The record keeps the flag, outside the root",
            p.key,
            reason.map_or_else(String::new, |r| format!(" ({r})"))
        ));
    }
    let text = snap.to_json(a.compact)?;
    match &a.output {
        Some(path) => {
            ops::write_file(path, &text)?;
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
    if let Ref::Cluster { .. }
    | Ref::EdgeHost { .. }
    | Ref::ClusterGroup { .. }
    | Ref::AppProfile { .. }
    | Ref::ClusterSet { .. }
    | Ref::Pack { .. } = reference
    {
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
            // the root is the published version's, which the candidate was written from. The
            // candidate file carries no digests, so its own root depends on what the registry
            // holds: `snapshot --from-candidate` is what measures it.
            progress.note(&format!(
                "wrote {} from {} (root {})",
                path.display(),
                snap.source.reference,
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
    let options = ops::VerifyOptions {
        who: a.who,
        allow_no_digest: a.allow_no_digest,
    };
    if bulk {
        let s = ops::verify_many(
            &paths,
            project.as_deref(),
            |scope| connect(progress, scope),
            &now_utc(),
            &options,
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
    let v = ops::verify_with(&recorded, &palette, &now_utc(), &options, &progress)?;
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
    Ok(exit_for(v.status == diff::VerifyStatus::Unchanged))
}

fn cmd_same_release(a: &SameReleaseArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let now = now_utc();
    let mut records = Vec::with_capacity(a.records.len());
    for r in &a.records {
        let snap = ops::snapshot_for(&Ref::parse(r), || connect(progress, None), &now, &progress)?;
        records.push((r.clone(), snap));
    }
    let policy = if a.allow.contains(&Allow::RegistryMove) {
        Policy::RegistryMove
    } else {
        Policy::Exact
    };
    let c = ops::same_release_with(&records, policy, a.allow.contains(&Allow::NoDigest), &now);
    print!("{}", a.out.renderer().render_chain(&c)?);
    Ok(ExitCode::from(c.exit_code()))
}

fn cmd_consumers(a: &ConsumersArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let r = ops::consumers_with(
        &Ref::parse(&a.r#ref),
        &ops::ConsumersOptions {
            list_only: a.list_only,
        },
        |scope| connect(progress, scope),
        &now_utc(),
        &progress,
    )?;
    print!("{}", a.out.renderer().render_consumers(&r)?);
    Ok(ExitCode::from(r.exit_code()))
}

fn cmd_behind(a: &BehindArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let paths = expand_records(&a.records)?;
    let since = match &a.since {
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            let not_one = || Error::Usage {
                message: format!(
                    "--since {} is not a {} document: give the JSON of a previous run",
                    path.display(),
                    ops::BEHIND_SCHEMA
                ),
            };
            let doc: serde_json::Value = serde_json::from_str(&text).map_err(|_| not_one())?;
            match doc.get("schema").and_then(serde_json::Value::as_str) {
                Some(ops::BEHIND_SCHEMA) => {}
                Some(found) => {
                    return Err(Error::UnsupportedSchema {
                        path: path.clone(),
                        found: found.to_string(),
                        supported: ops::BEHIND_SCHEMA,
                    });
                }
                None => return Err(not_one()),
            }
            Some(serde_json::from_value(doc).map_err(|_| not_one())?)
        }
        None => None,
    };
    let r = ops::behind(
        &paths,
        &ops::BehindOptions {
            impact: !a.no_impact,
            since,
        },
        |scope| connect(progress, scope),
        &now_utc(),
        &progress,
    )?;
    print!("{}", a.out.renderer().render_behind(&r)?);
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
        query: ops::ClusterQuery {
            tags: a.tag.clone(),
            filter: a.filter.clone(),
        },
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
        SchemaKind::Behind => schemars::schema_for!(ops::BehindReport),
        SchemaKind::Set => schemars::schema_for!(ops::SetReport),
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
