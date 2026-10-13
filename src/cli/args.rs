//! The arguments of the command line: the commands, and the flags of each one.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use strata::ops::ListKind;
use strata::render::{self, Format};
use strata::term::color::{ColorMode, Paint};
use strata::term::progress::Progress;
use strata::{TOOL_NAME, ops};

use super::progress;

#[derive(Parser)]
#[command(
    name = TOOL_NAME,
    version,
    about = "Record, compare, and check Palette profile versions, the clusters and other objects that run them, and pack versions",
    override_usage = "strata <COMMAND>\n       strata <A> <B>"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) cmd: Cmd,
}

impl Cli {
    /// True when the command will write JSON, so an error must be JSON too.
    pub(crate) fn json_selected(&self) -> bool {
        match &self.cmd {
            Cmd::Preflight(a) => {
                a.args.iter().any(|x| x == "--json" || x == "--format=json")
                    || a.args
                        .windows(2)
                        .any(|w| w[0] == "--format" && w[1] == "json")
            }
            Cmd::Snapshot(a) => a.format() == Format::Json,
            cmd => cmd.out().is_some_and(|out| out.format() == Format::Json),
        }
    }
}

#[derive(Subcommand)]
// In alphabetical order, which is the order `strata --help` lists them in.
pub(crate) enum Cmd {
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

impl Cmd {
    /// The output flags of the command, for each command that renders a report.
    pub(crate) fn out(&self) -> Option<&OutputArgs> {
        match self {
            Cmd::Behind(a) => Some(&a.out),
            Cmd::Compare(a) => Some(&a.out),
            Cmd::Consumers(a) => Some(&a.out),
            Cmd::List(a) => Some(&a.out),
            Cmd::Packs(a) => Some(&a.out),
            Cmd::SameRelease(a) => Some(&a.out),
            Cmd::Verify(a) => Some(&a.out),
            Cmd::Candidate(_) | Cmd::Preflight(_) | Cmd::Schema { .. } | Cmd::Snapshot(_) => None,
        }
    }
}

#[derive(Args)]
pub(crate) struct OutputArgs {
    /// The report format: terminal, markdown, or json
    #[arg(
        long,
        value_enum,
        default_value = "terminal",
        hide_possible_values = true
    )]
    pub(crate) format: Format,
    /// The same as --format json
    #[arg(long, conflicts_with = "format")]
    pub(crate) json: bool,
    /// Color the report: auto, always, or never
    #[arg(long, value_enum, default_value = "auto", num_args = 0..=1,
          default_missing_value = "always", hide_possible_values = true)]
    pub(crate) color: ColorMode,
    /// The same as --color=never
    #[arg(long, conflicts_with = "color")]
    pub(crate) no_color: bool,
    /// Write the JSON on one line
    #[arg(long)]
    pub(crate) compact: bool,
    /// No progress messages
    #[arg(short, long)]
    pub(crate) quiet: bool,
}

/// The format that `--json` and `--format` select: `--json` wins, and the terminal is the
/// default.
fn format_of(json: bool, format: Option<Format>) -> Format {
    if json {
        Format::Json
    } else {
        format.unwrap_or(Format::Terminal)
    }
}

impl OutputArgs {
    pub(crate) fn format(&self) -> Format {
        format_of(self.json, Some(self.format))
    }
    pub(crate) fn paint(&self) -> Paint {
        Paint::resolve(if self.no_color {
            ColorMode::Never
        } else {
            self.color
        })
    }
    pub(crate) fn renderer(&self) -> Box<dyn render::Renderer> {
        render::renderer(self.format(), self.paint(), self.compact)
    }
    pub(crate) fn progress(&self) -> Progress {
        progress(self.quiet)
    }
}

#[derive(Args)]
pub(crate) struct CompareArgs {
    /// The first side: a ref or a snapshot file
    pub(crate) a: String,
    /// The second side, in the same forms as the first
    pub(crate) b: String,
    /// Also print the unified diffs of the values and the manifests
    #[arg(long)]
    pub(crate) full: bool,
    /// Ask Palette if the change repaves the cluster side. One side must be a cluster. The other
    /// side is a profile version, or the update that waits for the cluster (cluster:<c>@pending)
    #[arg(long)]
    pub(crate) repave: bool,
    /// Ask Palette if it accepts the packs of the change on the cluster side; one side must be a
    /// cluster
    #[arg(long)]
    pub(crate) validate: bool,
    #[command(flatten)]
    pub(crate) out: OutputArgs,
}

#[derive(Args)]
pub(crate) struct SnapshotArgs {
    /// What to record: a ref, or the source with --from-raw
    pub(crate) r#ref: Option<String>,
    /// Write to this file, not to standard output
    #[arg(short, long)]
    pub(crate) output: Option<PathBuf>,
    /// Also save the raw API answers in this directory
    #[arg(long)]
    pub(crate) raw_dir: Option<PathBuf>,
    /// Read the raw answers of --raw-dir, with no network
    #[arg(long, conflicts_with = "raw_dir")]
    pub(crate) from_raw: Option<PathBuf>,
    /// Read a candidate file, or a profile export of Palette, before the version is in Palette
    #[arg(long, conflicts_with_all = ["from_raw", "raw_dir", "ref"])]
    pub(crate) from_candidate: Option<PathBuf>,
    /// Ask Palette whether it takes a candidate or a profile version. Palette checks the name, the
    /// version, and the packs of a candidate, and the packs of a version as an update of itself.
    /// A refusal exits 2 and writes no record
    #[arg(long)]
    pub(crate) validate: bool,
    /// Write the JSON on one line
    #[arg(long)]
    pub(crate) compact: bool,
    /// Record the digest of the value Palette gives each macro the content uses. It takes
    /// <name>@<version>, a profile uid, cluster:<cluster>, or clustergroup:<group>
    #[arg(long, conflicts_with_all = ["from_raw", "from_candidate"])]
    pub(crate) macros: bool,
    /// For a cluster: record the shape tier, the machine pools and the cloud part, with one more
    /// request
    #[arg(long, conflicts_with_all = ["from_raw", "from_candidate"])]
    pub(crate) shape: bool,
    /// For tag:KEY=VALUE or filter:NAME: write one record for each cluster into this directory,
    /// as <cluster name>.json
    #[arg(long, value_name = "DIR", conflicts_with_all = ["output", "raw_dir", "from_raw", "from_candidate"])]
    pub(crate) dir: Option<PathBuf>,
    /// With --dir: remove each cluster record of the directory whose cluster the query no
    /// longer selects
    #[arg(long, requires = "dir")]
    pub(crate) prune: bool,
    /// With --dir: the report format: terminal, markdown, or json
    #[arg(long, value_enum, requires = "dir", hide_possible_values = true)]
    pub(crate) format: Option<Format>,
    /// With --dir: the same as --format json
    #[arg(long, conflicts_with = "format", requires = "dir")]
    pub(crate) json: bool,
    /// No progress messages
    #[arg(short, long)]
    pub(crate) quiet: bool,
}

impl SnapshotArgs {
    /// The format of the report of a set. A record is always JSON.
    pub(crate) fn format(&self) -> Format {
        format_of(self.json, self.format)
    }

    /// What the record holds beyond the content, and what Palette is asked.
    pub(crate) fn options(&self) -> ops::SnapshotOptions {
        ops::SnapshotOptions {
            macros: self.macros,
            shape: self.shape,
            raw_dir: self.raw_dir.clone(),
            validate: self.validate,
        }
    }
}

#[derive(Args)]
pub(crate) struct CandidateArgs {
    /// A profile version, by ref or by record
    pub(crate) r#ref: String,
    /// Write to this file, not to standard output
    #[arg(short, long)]
    pub(crate) output: Option<PathBuf>,
    /// Write the values and the manifests to files in this directory
    #[arg(long, requires = "output")]
    pub(crate) files_dir: Option<PathBuf>,
    /// No progress messages
    #[arg(short, long)]
    pub(crate) quiet: bool,
}

#[derive(Args)]
pub(crate) struct VerifyArgs {
    /// Snapshot files; quote a pattern such as 'releases/**/*.json'
    #[arg(required = true)]
    pub(crate) snapshots: Vec<String>,
    /// Check the records in this project, by uid
    #[arg(long)]
    pub(crate) project: Option<String>,
    /// The same, by project name
    #[arg(long, conflicts_with = "project")]
    pub(crate) project_name: Option<String>,
    /// One line for each record and a summary, also for one file
    #[arg(long)]
    pub(crate) summary: bool,
    /// For a record that drifted, read who changed it, and when, from the audit log of Palette
    #[arg(long)]
    pub(crate) who: bool,
    /// Accept a pack with no upstream digest: exit 0 and not 1, and still name it
    #[arg(long)]
    pub(crate) allow_no_digest: bool,
    #[command(flatten)]
    pub(crate) out: OutputArgs,
}

#[derive(Args)]
pub(crate) struct SameReleaseArgs {
    /// Two or more refs, in promotion order
    #[arg(required = true, num_args = 2..)]
    pub(crate) records: Vec<String>,
    /// Pass a pair that differs only in this way; give it more than one time for more
    #[arg(long, value_enum)]
    pub(crate) allow: Vec<Allow>,
    #[command(flatten)]
    pub(crate) out: OutputArgs,
}

#[derive(Args)]
pub(crate) struct BehindArgs {
    /// Record files; quote a pattern such as 'releases/**/*.json'
    #[arg(required = true)]
    pub(crate) records: Vec<String>,
    /// The JSON of a previous run: each pin names the versions that arrived since
    #[arg(long, value_name = "FILE")]
    pub(crate) since: Option<PathBuf>,
    /// Do not compare each pin that is behind with the newest version, for the impact
    #[arg(long)]
    pub(crate) no_impact: bool,
    #[command(flatten)]
    pub(crate) out: OutputArgs,
}

#[derive(Args)]
pub(crate) struct ConsumersArgs {
    /// A profile version or an app profile version, by ref or by record
    pub(crate) r#ref: String,
    /// Read the consumer list that Palette computes, and not each cluster of the fleet. Faster,
    /// but it does not find a consumer that the list omits
    #[arg(long)]
    pub(crate) list_only: bool,
    #[command(flatten)]
    pub(crate) out: OutputArgs,
}

#[derive(Args)]
#[command(disable_help_flag = true)]
pub(crate) struct PreflightArgs {
    /// The arguments of strata-preflight, as `strata-preflight --help` lists them
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) args: Vec<std::ffi::OsString>,
}

#[derive(Args)]
pub(crate) struct PacksArgs {
    /// The pack name, as the registry holds it
    pub(crate) name: String,
    /// Only this registry, by name or uid
    #[arg(long, value_name = "NAME")]
    pub(crate) registry: Option<String>,
    /// Keep the versions of this cloud, such as aws, and the versions of the cloud type all
    #[arg(long, value_name = "TYPE")]
    pub(crate) cloud: Option<String>,
    /// Only the newest version of each registry, and the version Palette calls the latest when it
    /// differs
    #[arg(long)]
    pub(crate) latest: bool,
    #[command(flatten)]
    pub(crate) out: OutputArgs,
}

#[derive(Args)]
pub(crate) struct ListArgs {
    /// What to list; with none, profiles and clusters
    #[arg(value_enum)]
    pub(crate) kinds: Vec<ListKind>,
    /// List this project, by uid; '' is tenant scope
    #[arg(long)]
    pub(crate) project: Option<String>,
    /// The same, by project name
    #[arg(long, conflicts_with = "project")]
    pub(crate) project_name: Option<String>,
    /// Also read each version for its packs, its consumers, and whether it is published
    #[arg(long)]
    pub(crate) packs: bool,
    /// Keep the versions with a pack whose name holds TEXT; sets --packs
    #[arg(long, value_name = "TEXT")]
    pub(crate) pack: Option<String>,
    /// Also read each cluster for the profiles it applies
    #[arg(long)]
    pub(crate) applied: bool,
    /// Keep the clusters with this label, KEY=VALUE or KEY; give it again for each label.
    /// Palette searches every project the key can read
    #[arg(long, value_name = "KEY=VALUE")]
    pub(crate) tag: Vec<String>,
    /// Keep the clusters that a saved tag filter selects, by name or uid
    #[arg(long, value_name = "NAME")]
    pub(crate) filter: Option<String>,
    /// Keep the clusters whose name or uid holds TEXT; sets --applied
    #[arg(long, value_name = "TEXT")]
    pub(crate) cluster: Option<String>,
    #[command(flatten)]
    pub(crate) out: OutputArgs,
}

/// Differences `same-release` can be told to accept.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Allow {
    /// Packs from another registry, with the same tag, version, values, and manifests
    RegistryMove,
    /// A pack with no upstream digest, whose archive the root cannot show
    NoDigest,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum SchemaKind {
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
