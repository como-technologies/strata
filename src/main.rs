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

/// The commands by task, in the order of `strata --help`. The order follows the work: find the
/// refs, record a release, compare releases, check the records.
const GROUPS: &[(&str, &[&str])] = &[
    ("Find", &["list"]),
    ("Record", &["snapshot", "candidate"]),
    ("Compare", &["compare", "same-release"]),
    ("Check", &["verify", "consumers"]),
    ("Reference", &["schema", "help"]),
];

/// The description of the `help` command, which clap adds.
const HELP_ABOUT: &str = "Print this help, or the help of a command";

/// The refs, the environment, and the exit status, after the options of `strata --help`.
const REFERENCE_HELP: &[(&str, &[(&str, &str)])] = &[
    (
        "Refs",
        &[
            ("<uid>", "a profile version, by its uid"),
            (
                "<name>@<version>",
                "a profile version, by its name and version",
            ),
            (
                "<name>@latest",
                "the highest version of the name in the scope",
            ),
            (
                "cluster:<cluster>",
                "all profiles that a cluster runs, by its name or uid",
            ),
            (
                "cluster:<cluster>/<profile>",
                "one profile that a cluster runs",
            ),
            ("<file>", "a snapshot file from strata snapshot"),
        ],
    ),
    (
        "Environment",
        &[
            (
                "PALETTE_API_KEY",
                "the API key; SPECTROCLOUD_APIKEY also works",
            ),
            (
                "PALETTE_PROJECT",
                "the project name; PALETTE_PROJECT_UID has priority",
            ),
            (
                "PALETTE_API",
                "the Palette address; default https://api.spectrocloud.com",
            ),
            (
                "STRATA_RETRIES",
                "how many times to send a failed request again; default 3",
            ),
            (
                "STRATA_MAX_RETRY_AFTER",
                "the longest Retry-After wait in seconds; default 300",
            ),
            ("STRATA_QUIET", "any value: the same as --quiet"),
            (
                "~/.palette.env",
                "the key, the address, and the project, when the environment does not set them",
            ),
        ],
    ),
    (
        "Exit status",
        &[
            ("0", "the same, unchanged, or not in use"),
            ("1", "different, drifted, or in use"),
            ("2", "an error: strata cannot answer"),
        ],
    ),
];

#[derive(Parser)]
#[command(
    name = TOOL_NAME,
    version,
    about = "Record, compare, and check Palette cluster profile versions as releases",
    long_about = "Record, compare, and check Palette cluster profile versions as releases.\n\n\
A published profile version is a release. Strata writes a release to a snapshot file. It compares \
two releases layer by layer, and it makes sure that Palette still has the release that a file \
records. A cluster runs a release, and strata reads a cluster in the same way.",
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
            Cmd::Snapshot(_) | Cmd::Candidate(_) | Cmd::Schema { .. } => false,
        }
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// List the profile versions, clusters, and projects in the scope
    ///
    /// The first column of each line is a ref that the other commands accept. The scope is the
    /// configured project, or the project that --project or --project-name gives.
    List(ListArgs),
    /// Record a profile version or a cluster in a snapshot file
    ///
    /// The file holds the root, the hashes of each pack, the values and manifests, the project,
    /// and the clusters that use the version. `compare`, `same-release`, and `verify` read it.
    Snapshot(SnapshotArgs),
    /// Write a candidate file from a profile version or a record
    ///
    /// The file gives the pack, tag, registry, values, and manifests of each layer, and the
    /// variables. `strata snapshot --from-candidate` calculates the same root from the file, when
    /// the registry still has the same pack versions.
    Candidate(CandidateArgs),
    /// Compare two profile versions, records, or clusters
    ///
    /// The report gives the root of each side, the variables, and one line for each pack with its
    /// change flags and impact. `strata <A> <B>` is the same as `strata compare <A> <B>`.
    ///
    /// Exit status: 0 when the roots are equal, 1 when they differ, 2 on an error.
    Compare(CompareArgs),
    /// Make sure that two or more records are one release
    ///
    /// Strata compares each record with the next one, in the order given. A pair passes when the
    /// roots are equal. A pair also passes when the roots differ only by a tag name or by the uid
    /// that Palette gives a manifest pack, because the content is the same.
    ///
    /// Exit status: 0 when each pair passes, 1 when a pair fails, 2 on an error.
    SameRelease(SameReleaseArgs),
    /// Make sure that each record agrees with Palette now
    ///
    /// For each record, strata fetches the same uid again in the project of the record and
    /// compares the roots. A version number that changed on the same uid is drift too. One file
    /// prints the full report and a verdict. Two or more files, or a pattern, print one line for
    /// each record and a summary.
    ///
    /// Exit status: 0 when each record is unchanged, 1 when a record drifted, 2 when strata
    /// cannot check a record.
    Verify(VerifyArgs),
    /// List the clusters that use a profile version
    ///
    /// Each line gives the name, uid, project, and state of a cluster. When the copy of a cluster
    /// names a version that differs from the profile, the line says so.
    ///
    /// Exit status: 0 when no cluster uses the version, 1 when a cluster uses it, 2 on an error.
    Consumers(ConsumersArgs),
    /// Print the JSON Schema of a document or of the candidate file
    Schema {
        /// The document: snapshot, diff, verify-summary, chain, consumers, list, or candidate
        #[arg(value_enum)]
        kind: SchemaKind,
    },
}

#[derive(Args)]
struct OutputArgs {
    /// The output format: terminal, markdown for a pull request comment, or json for a script
    #[arg(long, value_enum, default_value = "terminal")]
    format: FormatArg,
    /// The same as --format json
    #[arg(long)]
    json: bool,
    /// When to color the terminal report. auto colors only a terminal, and obeys NO_COLOR and
    /// CLICOLOR_FORCE
    #[arg(long, value_enum, default_value = "auto", num_args = 0..=1, default_missing_value = "always")]
    color: ColorArg,
    /// The same as --color=never
    #[arg(long)]
    no_color: bool,
    /// Write the JSON on one line
    #[arg(long)]
    compact: bool,
    /// Do not write progress messages to standard error
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
    /// The first side: a profile uid, <name>@<version>, <name>@latest,
    /// cluster:<cluster>[/<profile>], or a snapshot file
    a: String,
    /// The second side, in the same forms as the first
    b: String,
    /// Also print unified diffs of the raw values and of each changed manifest
    #[arg(long)]
    full: bool,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct SnapshotArgs {
    /// A profile uid, <name>@<version>, <name>@latest, or cluster:<cluster>[/<profile>]. With
    /// --from-raw, it only names the source and selects one profile of a cluster directory
    r#ref: Option<String>,
    /// Write the snapshot to this file, not to standard output
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Also save the raw API responses in this directory
    #[arg(long)]
    raw_dir: Option<PathBuf>,
    /// Build the snapshot from the raw API responses that --raw-dir saved, with no network
    #[arg(long)]
    from_raw: Option<PathBuf>,
    /// Build the snapshot from a candidate file, before the version is in Palette. Strata finds
    /// each pack in its registry. See `strata schema candidate`
    #[arg(long, conflicts_with_all = ["from_raw", "raw_dir", "ref"])]
    from_candidate: Option<PathBuf>,
    /// Write the JSON on one line
    #[arg(long)]
    compact: bool,
    /// Do not write progress messages to standard error
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Args)]
struct CandidateArgs {
    /// A profile uid, <name>@<version>, <name>@latest, or a profile record
    r#ref: String,
    /// Write the candidate file to this file, not to standard output
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Write the values of each layer and each manifest to a file in this directory, and refer to
    /// those files from the candidate file
    #[arg(long, requires = "output")]
    files_dir: Option<PathBuf>,
    /// Do not write progress messages to standard error
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Args)]
struct VerifyArgs {
    /// Snapshot files. Put a pattern such as 'releases/**/*.json' in quotes: strata expands it
    #[arg(required = true)]
    snapshots: Vec<String>,
    /// Check each record in the project with this uid, not in the project of the record
    #[arg(long)]
    project: Option<String>,
    /// The same as --project, with the project name
    #[arg(long, conflicts_with = "project")]
    project_name: Option<String>,
    /// Print one line and a summary also for one file, so a script always reads the same document
    #[arg(long)]
    summary: bool,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct SameReleaseArgs {
    /// Two or more refs, in order: snapshot files, profile versions, or clusters
    #[arg(required = true, num_args = 2..)]
    records: Vec<String>,
    /// Also pass a pair that differs only in this way
    #[arg(long, value_enum)]
    allow: Option<Allow>,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct ConsumersArgs {
    /// A profile uid, <name>@<version>, <name>@latest, or a profile record. Strata always reads
    /// the clusters from Palette
    r#ref: String,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct ListArgs {
    /// What to list: profiles, clusters, projects. With none: profiles and clusters, or only
    /// profiles with --pack
    #[arg(value_enum)]
    kinds: Vec<KindArg>,
    /// List the profiles and clusters of the project with this uid, not of the configured project.
    /// '' is tenant scope. Strata always lists projects at tenant scope
    #[arg(long)]
    project: Option<String>,
    /// The same as --project, with the project name
    #[arg(long, conflicts_with = "project")]
    project_name: Option<String>,
    /// Also read each profile version for its type, consumers, and packs: one request for each
    #[arg(long)]
    packs: bool,
    /// Keep the profile versions with a pack whose name contains TEXT, in upper or lower case.
    /// Sets --packs
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
    Candidate,
}

/// The command line, with `strata --help` listing the commands by task and ending with the refs,
/// the environment, and the exit status.
fn command() -> clap::Command {
    let cmd = Cli::command();
    let styles = cmd.get_styles().clone();
    let heading = |text: &str| {
        let h = styles.get_header();
        format!("{}{text}:{}\n", h.render(), h.render_reset())
    };
    let row = |width: usize, name: &str, text: &str| {
        let l = styles.get_literal();
        format!(
            "  {}{name:width$}{}  {text}\n",
            l.render(),
            l.render_reset()
        )
    };

    let width = GROUPS
        .iter()
        .flat_map(|(_, names)| names.iter())
        .map(|name| name.len())
        .max()
        .unwrap_or(0);
    let mut commands = String::new();
    for (group, names) in GROUPS {
        commands.push_str(&heading(group));
        for name in *names {
            let about = match cmd.find_subcommand(name).and_then(|c| c.get_about()) {
                Some(about) => about.to_string(),
                None => HELP_ABOUT.to_string(),
            };
            commands.push_str(&row(width, name, &about));
        }
        commands.push('\n');
    }

    let reference: Vec<String> = REFERENCE_HELP
        .iter()
        .map(|(section, rows)| {
            let width = rows.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
            let lines: String = rows
                .iter()
                .map(|(name, text)| row(width, name, text))
                .collect();
            heading(section) + &lines
        })
        .collect();
    let reference = reference.join("\n");

    cmd.help_template(format!(
        "{{about-with-newline}}\n{{usage-heading}} {{usage}}\n\n{commands}{}{{options}}{{after-help}}\n",
        heading("Options")
    ))
    .after_long_help(reference.trim_end().to_string())
}

/// True when `arg` names a command, so `strata <A> <B>` does not become `strata compare`.
fn is_command(arg: &str) -> bool {
    GROUPS.iter().any(|(_, names)| names.contains(&arg))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_lists_each_command_in_exactly_one_group() {
        let mut listed: Vec<&str> = GROUPS
            .iter()
            .flat_map(|(_, names)| names.iter().copied())
            .collect();
        let count = listed.len();
        listed.sort_unstable();
        listed.dedup();
        assert_eq!(listed.len(), count, "a command is in two groups");

        let mut built = command();
        built.build();
        let mut commands: Vec<&str> = built.get_subcommands().map(|c| c.get_name()).collect();
        commands.sort_unstable();
        assert_eq!(listed, commands);
    }

    #[test]
    fn help_follows_the_group_order_and_ends_with_the_exit_status() {
        let help = command().render_long_help().to_string();
        let at = |text: &str| {
            help.find(text)
                .unwrap_or_else(|| panic!("no {text:?} in:\n{help}"))
        };
        let mut last = 0;
        for (group, names) in GROUPS {
            assert!(at(&format!("{group}:")) > last, "{group} is out of order");
            last = at(&format!("{group}:"));
            for name in *names {
                assert!(
                    at(&format!("  {name} ")) > last,
                    "{name} is not under {group}"
                );
            }
        }
        assert!(at("Options:") > last);
        assert!(at("Refs:") > at("Options:"));
        assert!(at("Exit status:") > at("Environment:"));
        assert!(!help.contains("Merkle"));
    }
}
