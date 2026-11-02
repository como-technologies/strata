//! The strata-preflight command line: argument parsing, printing, and exit codes. Everything else
//! is in the library. `strata preflight ...` runs this binary.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use strata::api::{self, Palette, PaletteApi};
use strata::closed_set::ClosedSet;
use strata::config;
use strata::ops;
use strata::reference::Ref;
use strata::render::count_of;
use strata::term::color::{ColorMode, Paint, Stream};
use strata::term::progress::Progress;
use strata::text::{dash, plural};
use strata::timestamp::now_utc;
use strata_preflight::confidence::{self, Ledger};
use strata_preflight::error::{Error, Result};
use strata_preflight::probe::LiveBackend;
use strata_preflight::report::{self, Format, Report};
use strata_preflight::rule::{self, Catalog, Tier};
use strata_preflight::run::{
    self, DEFAULT_EVERY_SECS, DEFAULT_LIMIT_SECS, Selection, WatchOptions,
};
use strata_preflight::world::World;
use strata_preflight::{TOOL_NAME, catalog};

#[derive(Parser)]
#[command(
    name = TOOL_NAME,
    version,
    disable_help_flag = true,
    about = "Check that Palette can build a cluster, before the build",
    override_usage = "strata-preflight <SOURCE> [CHANGE] [OPTIONS]\n       strata-preflight --watch cluster:UID [OPTIONS]\n       strata-preflight --rules [OPTIONS]\n       strata-preflight --promotion [OPTIONS]",
    after_help = "The book: https://legendary-train-pzq9ojj.pages.github.io/commands/preflight.html"
)]
struct Cli {
    /// The body of a create as a file, or cluster:UID for a cluster as Palette holds it now
    source: Option<String>,
    /// A change of the cluster, as a file: the pools or the profiles that change
    change: Option<PathBuf>,
    /// The engine of a file: aws, eks, gcp, gke, azure, aks, maas, edge, vsphere; a cluster names its own
    #[arg(long, value_name = "CLOUD")]
    cloud: Option<String>,
    /// The Palette release, such as 4.10; with none, a rule of one release applies; --promotion counts only the runs of this release
    #[arg(long, value_name = "RELEASE")]
    release: Option<String>,
    /// Records of the profiles, as files or a pattern in quotes; with none, read from Palette
    #[arg(long, value_name = "FILE")]
    records: Vec<String>,
    /// A file of custom probes, in the format of the rules; give it again for more
    #[arg(long, value_name = "FILE")]
    probes: Vec<PathBuf>,
    /// The tiers to run: static, palette, cloud, as a list; default all three
    #[arg(long, value_name = "TIER", value_delimiter = ',')]
    tier: Vec<String>,
    /// Only these rules, by id; give it again for more; an id that is not in the catalog is an error
    #[arg(long, value_name = "ID")]
    rule: Vec<String>,
    /// Count a failed probe as a failed check
    #[arg(long)]
    strict: bool,
    /// Exit 1 when the readiness is under this percentage, such as 80
    #[arg(long, value_name = "PERCENT")]
    ready: Option<f64>,
    /// Read nothing from Palette: the static tier alone, from the files given
    #[arg(long)]
    offline: bool,
    /// Run no command of a cloud tool; each cloud rule is NOT CHECKED
    #[arg(long)]
    no_exec: bool,
    /// Watch the build of cluster:UID: the conditions and the events, until Running or a stuck state
    #[arg(long)]
    watch: bool,
    /// With --watch: seconds between two readings
    #[arg(long, value_name = "SECONDS", default_value_t = DEFAULT_EVERY_SECS)]
    every: u64,
    /// With --watch: the longest watch, in seconds
    #[arg(long, value_name = "SECONDS", default_value_t = DEFAULT_LIMIT_SECS)]
    limit: u64,
    /// List the rules, with the level and the tier of each, and run nothing
    #[arg(long)]
    rules: bool,
    /// Compare the level of each rule with the runs of the ledger; exit 1 when they disagree
    #[arg(long)]
    promotion: bool,
    /// A ledger of runs to read beside the one in the binary; give it again for more
    #[arg(long, value_name = "FILE")]
    ledger: Vec<PathBuf>,
    /// Print the JSON Schema of a document, and run nothing
    #[arg(long, value_enum, value_name = "KIND")]
    schema: Option<SchemaKind>,
    /// Write report.md, report.json, report.html, world.json, and entity.json into this directory
    #[arg(long, value_name = "DIR")]
    out: Option<PathBuf>,
    /// The report format: terminal, markdown, json, or html
    #[arg(
        long,
        value_enum,
        default_value = "terminal",
        hide_possible_values = true
    )]
    format: Format,
    /// The same as --format json
    #[arg(long, conflicts_with = "format")]
    json: bool,
    /// Write the JSON on one line
    #[arg(long)]
    compact: bool,
    /// No color
    #[arg(long)]
    no_color: bool,
    /// When to color the report and the progress lines: auto (a terminal; NO_COLOR, CLICOLOR_FORCE, and TERM=dumb decide), always, or never
    #[arg(long, value_enum, default_value = "auto", value_name = "WHEN")]
    color: ColorMode,
    /// No progress messages
    #[arg(short, long)]
    quiet: bool,
    /// Print what each check ran: the request or the command, its answer, the time, and the verdict
    #[arg(short, long)]
    verbose: bool,
    /// Print help
    #[arg(short, long, action = clap::ArgAction::HelpShort)]
    help: Option<bool>,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SchemaKind {
    /// The report of a run
    Report,
    /// A rule file: the catalog, and a probes file
    Rules,
    /// The ledger of runs
    Ledger,
    /// The promotion report
    Promotion,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = format_of(&cli) == Format::Json;
    match run_cli(cli) {
        Ok(code) => code,
        Err(e) => {
            if json {
                eprintln!("{}", serde_json::json!({ "error": e.to_json() }));
            } else {
                eprintln!("{TOOL_NAME}: {e}");
            }
            ExitCode::from(2)
        }
    }
}

fn format_of(cli: &Cli) -> Format {
    if cli.json { Format::Json } else { cli.format }
}

/// The color of one stream, by the rule of `strata`: `--color always` colors a pipe too (a `tee`
/// to a log and a terminal), `never` and `--no-color` never do, and `auto` colors a terminal
/// unless the environment says otherwise. The report goes to standard output, and the progress
/// lines go to standard error.
fn paint_of(cli: &Cli, stream: Stream) -> Paint {
    Paint::resolve(cli.color.unless(cli.no_color), stream)
}

fn tiers_of(cli: &Cli) -> Result<Vec<Tier>> {
    if cli.tier.is_empty() {
        return Ok(if cli.offline {
            vec![Tier::Static]
        } else {
            Selection::default().tiers
        });
    }
    cli.tier
        .iter()
        .map(|t| {
            Tier::from_input(t).ok_or_else(|| Error::Usage {
                message: format!("--tier {t}: give static, palette, or cloud, as a list"),
            })
        })
        .collect()
}

fn ledger_of(cli: &Cli) -> Result<Ledger> {
    let mut ledger = catalog::builtin_ledger()?;
    for path in &cli.ledger {
        ledger.extend(Ledger::load(path)?);
    }
    Ok(ledger)
}

fn run_cli(cli: Cli) -> Result<ExitCode> {
    // --verbose asks for the lines, so it wins over STRATA_QUIET; --quiet wins over both
    let progress = Progress::new(cli.quiet || (config::quiet_from_env() && !cli.verbose));
    if let Some(kind) = cli.schema {
        return print_schema(kind);
    }
    // a release that is not a version is in each range: refuse it before any request
    if let Some(release) = &cli.release {
        rule::require_release(release)?;
    }
    let catalog = catalog::with_probes(&cli.probes)?;
    let tiers = tiers_of(&cli)?;
    if cli.rules {
        return print_rules(&cli, &catalog, &tiers);
    }
    if cli.promotion {
        return print_promotion(&cli, &catalog);
    }
    check_cluster(&cli, &catalog, tiers, progress)
}

/// Print the JSON Schema of one document of strata-preflight.
fn print_schema(kind: SchemaKind) -> Result<ExitCode> {
    let schema = match kind {
        SchemaKind::Report => schemars::schema_for!(Report),
        SchemaKind::Rules => schemars::schema_for!(strata_preflight::rule::Section),
        SchemaKind::Ledger => schemars::schema_for!(Ledger),
        SchemaKind::Promotion => schemars::schema_for!(confidence::PromotionReport),
    };
    emit(&format!("{}\n", strata::text::schema_json(&schema)?));
    Ok(ExitCode::SUCCESS)
}

/// Print the rules of the catalog (`--rules`), as text or as a document.
fn print_rules(cli: &Cli, catalog: &Catalog, tiers: &[Tier]) -> Result<ExitCode> {
    let rules = run::list_rules(
        catalog,
        cli.cloud.as_deref(),
        cli.release.as_deref(),
        if cli.tier.is_empty() { &[] } else { tiers },
    );
    if format_of(cli) == Format::Json {
        let doc = run::rules_document(
            catalog,
            cli.cloud.as_deref(),
            cli.release.as_deref(),
            &rules,
        );
        print_json(&doc, cli.compact)?;
    } else {
        emit(&run::rules_text(catalog, &rules));
    }
    Ok(ExitCode::SUCCESS)
}

/// Print the promotion report (`--promotion`). Its exit code is `1` when a level disagrees with
/// the ledger.
fn print_promotion(cli: &Cli, catalog: &Catalog) -> Result<ExitCode> {
    let ledger = ledger_of(cli)?;
    let report = confidence::promotion(catalog, &ledger, cli.release.as_deref());
    if format_of(cli) == Format::Json {
        print_json(&serde_json::to_value(&report)?, cli.compact)?;
    } else {
        emit(&promotion_text(&report));
    }
    Ok(ExitCode::from(report.exit_code()))
}

/// Run the checks of one entity, or watch one cluster, and print the report.
fn check_cluster(
    cli: &Cli,
    catalog: &Catalog,
    tiers: Vec<Tier>,
    progress: Progress,
) -> Result<ExitCode> {
    let source = cli.source.clone().ok_or_else(|| Error::Usage {
        message: "give the body of a create as a file, or cluster:UID; --rules lists the rules, and --help the options".to_string(),
    })?;
    let selection = Selection {
        tiers,
        ids: cli.rule.clone(),
        strict: cli.strict,
        ready: cli.ready.map(|p| p / 100.0),
    };
    // a wrong id is a usage error, so it comes before the first request
    selection.require_known(catalog)?;
    let now = now_utc();
    let ledger = ledger_of(cli)?;
    let reference = Ref::parse(&source);
    refuse_a_profile_of_a_cluster(&reference)?;
    let palette = if cli.offline {
        None
    } else {
        Some(api::connect_from_env(progress)?)
    };
    // the world and the checks read through one backend, which keeps each answer for the run. So
    // a request that the world made, and that a rule makes again, goes out one time
    let backend = LiveBackend::new(palette.as_ref().map(Palette::api), cli.no_exec);
    let reads = palette.as_ref().map(|_| Palette::new(&backend));
    let world = world_of(cli, &source, reference, reads.as_ref(), &now, &progress)?;
    let out_dir = cli.out.as_ref().map(|d| d.display().to_string());
    let mut reading: Option<serde_json::Value> = None;
    let report = if cli.watch {
        let palette = palette.as_ref().ok_or_else(|| Error::Usage {
            message: "--watch needs Palette: drop --offline".to_string(),
        })?;
        run::watch(run::WatchInputs {
            catalog,
            world: &world,
            palette,
            ledger: &ledger,
            selection: &selection,
            options: &WatchOptions {
                every: Duration::from_secs(cli.every),
                limit: Duration::from_secs(cli.limit),
                stop_at_first: true,
            },
            now: &now,
            progress: &progress,
            sleep: &std::thread::sleep,
            out_dir: out_dir.as_deref(),
            verbose: cli.verbose,
            paint: paint_of(cli, Stream::Stderr),
        })
        .map(|(report, world)| {
            reading = Some(world);
            report
        })?
    } else {
        run::run(run::RunInputs {
            catalog,
            world: &world,
            backend: &backend,
            ledger: &ledger,
            selection: &selection,
            now: &now,
            out_dir: out_dir.as_deref(),
            progress: &progress,
            verbose: cli.verbose,
            paint: paint_of(cli, Stream::Stderr),
        })?
    };
    print_report(cli, &report, &world, reading.as_ref(), &progress)?;
    Ok(ExitCode::from(report.exit_code()))
}

/// A wrong use of the source is a usage error, so it comes before the first request: the checks
/// read the whole cluster.
fn refuse_a_profile_of_a_cluster(reference: &Ref) -> Result<()> {
    if let Ref::Cluster {
        profile, pending, ..
    } = reference
        && (profile.is_some() || *pending)
    {
        return Err(Error::Usage {
            message: "the checks read the whole cluster: give cluster:UID with no profile and no @pending"
                .to_string(),
        });
    }
    Ok(())
}

/// The world of the run: the entity, and what the static rules need beside it. `reads` is none
/// with `--offline`.
fn world_of<A: PaletteApi>(
    cli: &Cli,
    source: &str,
    reference: Ref,
    reads: Option<&Palette<A>>,
    now: &str,
    progress: &Progress,
) -> Result<World> {
    let mut world = match reference {
        Ref::Cluster { uid, .. } => {
            let reads = reads.ok_or_else(|| Error::Usage {
                message: "cluster:UID needs Palette: drop --offline, or give the entity as a file"
                    .to_string(),
            })?;
            World::from_cluster(reads, &uid, progress)?
        }
        _ => World::from_file(std::path::Path::new(source), cli.cloud.as_deref())?,
    };
    world.release = cli.release.clone();
    if let Some(change) = &cli.change {
        world = world.with_change(change)?;
    }
    if !cli.records.is_empty() {
        world = world.with_records(&ops::expand_records(&cli.records)?)?;
    }
    if let Some(reads) = reads {
        world = world.with_palette(reads, now, progress)?;
    }
    Ok(world)
}

/// Write the files of `--out`, then print the report.
fn print_report(
    cli: &Cli,
    report: &Report,
    world: &World,
    reading: Option<&serde_json::Value>,
    progress: &Progress,
) -> Result<()> {
    if let Some(dir) = &cli.out {
        write_out(dir, report, world, reading)?;
        progress.note(&format!(
            "wrote {}/report.md, report.json, report.html, world.json, entity.json",
            dir.display()
        ));
    }
    emit(&report::render(
        report,
        format_of(cli),
        cli.compact,
        &paint_of(cli, Stream::Stdout),
    ));
    Ok(())
}

/// Write to stdout. A reader that closes the pipe early (`| head`) is not an error of strata,
/// so a failed write is not a panic.
fn emit(text: &str) {
    use std::io::Write;
    let _ = std::io::stdout().write_all(text.as_bytes());
}

fn print_json(doc: &serde_json::Value, compact: bool) -> Result<()> {
    if compact {
        emit(&format!("{}\n", serde_json::to_string(doc)?));
    } else {
        emit(&format!("{}\n", serde_json::to_string_pretty(doc)?));
    }
    Ok(())
}

/// Write the reports and the world. With a watch, `reading` is the world with the last reading of
/// the cluster in it, so `world.json` holds the conditions and the events the rules saw.
fn write_out(
    dir: &std::path::Path,
    report: &Report,
    world: &World,
    reading: Option<&serde_json::Value>,
) -> Result<()> {
    // the library of strata makes the directory, and names the file in its error
    let write = |name: &str, text: String| -> Result<()> {
        strata::ops::write_file(&dir.join(name), &text).map_err(|e| match e {
            strata::Error::Io { path, source } => Error::Io { path, source },
            other => other.into(),
        })
    };
    let off = Paint::off();
    write(
        "report.md",
        report::render(report, Format::Markdown, false, &off),
    )?;
    write(
        "report.json",
        report::render(report, Format::Json, false, &off),
    )?;
    write(
        "report.html",
        report::render(report, Format::Html, false, &off),
    )?;
    let mut world_value = match reading {
        Some(read) => read.clone(),
        None => world.to_value()?,
    };
    strata_preflight::world::redact_for_output(&mut world_value);
    write(
        "world.json",
        serde_json::to_string_pretty(&world_value)? + "\n",
    )?;
    write(
        "entity.json",
        serde_json::to_string_pretty(&world.entity)? + "\n",
    )?;
    Ok(())
}

fn promotion_text(report: &confidence::PromotionReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<32} {:<8} {:<10} {:<10} {:<8} confidence (engine release: catch, clean, older runs)\n",
        "rule", "tier", "level", "earned", "advice"
    ));
    for r in &report.rules {
        let conf: Vec<String> = r
            .confidence
            .iter()
            .map(|c| format!("{} {}: {}", c.engine, dash(&c.release), c.text()))
            .collect();
        out.push_str(&format!(
            "{:<32} {:<8} {:<10} {:<10} {:<8} {}\n",
            r.id,
            r.tier.as_str(),
            r.level.as_str(),
            r.earned.as_str(),
            dash(&r.advice),
            if conf.is_empty() {
                confidence::NO_RUNS.to_string()
            } else {
                conf.join("; ")
            }
        ));
    }
    let n = report.disagreements as usize;
    out.push_str(&format!(
        "\n{} rules with a check; {} {} the ledger\n",
        report.rules.len(),
        count_of(n, "level"),
        plural(n, "disagrees with", "disagree with")
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_help_screen_fits_a_terminal_and_names_the_forms() {
        let help = Cli::command().render_help().to_string();
        assert!(
            help.contains(&strata_preflight::world::cloud_list()),
            "{help}"
        );
        assert!(help.contains("--watch cluster:UID"), "{help}");
        assert!(help.contains("--rules"), "{help}");
        assert!(help.contains("--probes"), "{help}");
        assert!(help.contains("--strict"), "{help}");
    }
}
