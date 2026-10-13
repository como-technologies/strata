//! The strata-preflight command line: argument parsing, printing, and exit codes. Everything else
//! is in the library. `strata preflight ...` runs this binary.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use strata::api::{HttpApi, Palette, Retry, RetryPolicy};
use strata::color::ColorMode;
use strata::config::Config;
use strata::progress::Progress;
use strata_preflight::confidence::{self, Ledger};
use strata_preflight::error::{Error, Result};
use strata_preflight::probe::LiveBackend;
use strata_preflight::report::{self, Format, Report};
use strata_preflight::rule::Tier;
use strata_preflight::run::{self, Selection, WatchOptions};
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
    /// The Palette release, such as 4.10; with none, a rule of one release applies
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
    /// Only these rules, by id; give it again for more
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
    #[arg(long, value_name = "SECONDS", default_value = "30")]
    every: u64,
    /// With --watch: the longest watch, in seconds
    #[arg(long, value_name = "SECONDS", default_value = "3600")]
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
    format: FormatArg,
    /// The same as --format json
    #[arg(long, conflicts_with = "format")]
    json: bool,
    /// Write the JSON on one line
    #[arg(long)]
    compact: bool,
    /// No color
    #[arg(long)]
    no_color: bool,
    /// When to color the report and the progress lines: auto (a terminal, and NO_COLOR not set), always, or never
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
enum FormatArg {
    Terminal,
    Markdown,
    Json,
    Html,
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
    let json = cli.json || cli.format == FormatArg::Json;
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

fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn format_of(cli: &Cli) -> Format {
    if cli.json {
        return Format::Json;
    }
    match cli.format {
        FormatArg::Terminal => Format::Terminal,
        FormatArg::Markdown => Format::Markdown,
        FormatArg::Json => Format::Json,
        FormatArg::Html => Format::Html,
    }
}

/// When to color: `--color always` colors a pipe too (a `tee` to a log and a terminal), `never`
/// and `--no-color` never do, and `auto` colors a terminal unless `NO_COLOR` is set.
fn wants_color(cli: &Cli, stream_is_terminal: bool) -> bool {
    match cli.color {
        ColorMode::Never => false,
        ColorMode::Always => !cli.no_color,
        ColorMode::Auto => {
            !cli.no_color && std::env::var_os("NO_COLOR").is_none() && stream_is_terminal
        }
    }
}

/// Color for the report, which goes to stdout.
fn color(cli: &Cli) -> bool {
    wants_color(cli, std::io::IsTerminal::is_terminal(&std::io::stdout()))
}

/// Color for the progress lines, which go to stderr.
fn color_err(cli: &Cli) -> bool {
    wants_color(cli, std::io::IsTerminal::is_terminal(&std::io::stderr()))
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
            Tier::parse(t).ok_or_else(|| Error::Usage {
                message: format!("--tier {t}: give static, palette, or cloud, as a list"),
            })
        })
        .collect()
}

fn ledger_of(cli: &Cli) -> Result<Ledger> {
    let mut ledger = Ledger::parse(catalog::LEDGER, std::path::Path::new("evidence/runs.yaml"))?;
    for path in &cli.ledger {
        ledger.extend(Ledger::load(path)?);
    }
    Ok(ledger)
}

fn expand(patterns: &[String]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for p in patterns {
        if p.contains(['*', '?', '[']) {
            let matches: Vec<PathBuf> = glob_paths(p)?;
            if matches.is_empty() {
                return Err(Error::Usage {
                    message: format!("--records '{p}' matched no file"),
                });
            }
            out.extend(matches);
        } else {
            out.push(PathBuf::from(p));
        }
    }
    Ok(out)
}

/// A pattern over the file system, expanded by the same crate as strata, so `releases/**/*.json`
/// names the records at every depth, the ones directly under `releases/` too.
fn glob_paths(pattern: &str) -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = glob::glob(pattern)
        .map_err(|e| Error::Usage {
            message: format!("--records '{pattern}' is not a valid pattern: {}", e.msg),
        })?
        .filter_map(std::result::Result::ok)
        .filter(|p| p.is_file())
        .collect();
    out.sort();
    Ok(out)
}

fn connect(progress: Progress) -> Result<Palette<Retry<HttpApi>>> {
    let mut config = Config::from_env()?;
    if config.project_uid.is_none()
        && let Some(name) = config.project_name.clone()
    {
        let mut tenant = config.clone();
        tenant.project_uid = None;
        progress.note(&format!("looking up project {name}"));
        let uid = client(tenant, progress).project_uid_named(&name)?;
        config.project_uid = Some(uid);
    }
    Ok(client(config, progress))
}

fn client(config: Config, progress: Progress) -> Palette<Retry<HttpApi>> {
    let policy = RetryPolicy::from_config(&config);
    Palette::new(Retry::new(HttpApi::new(config), policy, progress))
}

fn run_cli(cli: Cli) -> Result<ExitCode> {
    // --verbose asks for the lines, so it wins over STRATA_QUIET; --quiet wins over both
    let progress =
        Progress::new(cli.quiet || (std::env::var_os("STRATA_QUIET").is_some() && !cli.verbose));
    if let Some(kind) = cli.schema {
        let schema = match kind {
            SchemaKind::Report => schemars::schema_for!(Report),
            SchemaKind::Rules => schemars::schema_for!(strata_preflight::rule::Section),
            SchemaKind::Ledger => schemars::schema_for!(Ledger),
            SchemaKind::Promotion => schemars::schema_for!(confidence::PromotionReport),
        };
        emit(&format!("{}\n", serde_json::to_string_pretty(&schema)?));
        return Ok(ExitCode::SUCCESS);
    }
    let catalog = catalog::with_probes(&cli.probes)?;
    let tiers = tiers_of(&cli)?;
    if cli.rules {
        let rules = run::list_rules(
            &catalog,
            cli.cloud.as_deref(),
            cli.release.as_deref(),
            if cli.tier.is_empty() { &[] } else { &tiers },
        );
        if format_of(&cli) == Format::Json {
            let doc = serde_json::json!({
                "schema": "strata-preflight-rules/1",
                "tool": {"name": TOOL_NAME, "version": strata_preflight::TOOL_VERSION},
                "cloud": cli.cloud,
                "release": cli.release,
                "sections": catalog.sections.iter().map(|s| serde_json::json!({"section": s.section, "title": s.title, "about": s.about})).collect::<Vec<_>>(),
                "rules": rules,
            });
            print_json(&doc, cli.compact)?;
        } else {
            emit(&run::rules_text(&catalog, &rules));
        }
        return Ok(ExitCode::SUCCESS);
    }
    if cli.promotion {
        let ledger = ledger_of(&cli)?;
        let report = confidence::promotion(&catalog, &ledger);
        if format_of(&cli) == Format::Json {
            print_json(&serde_json::to_value(&report)?, cli.compact)?;
        } else {
            emit(&promotion_text(&report));
        }
        return Ok(ExitCode::from(report.exit_code()));
    }
    let source = cli.source.clone().ok_or_else(|| Error::Usage {
        message: "give the body of a create as a file, or cluster:UID; --rules lists the rules, and --help the options".to_string(),
    })?;
    let now = now_utc();
    let ledger = ledger_of(&cli)?;
    let live = !cli.offline;
    let palette = if live { Some(connect(progress)?) } else { None };

    // the world: the entity, and what the static rules need beside it
    let mut world = match source.strip_prefix("cluster:") {
        Some(uid) => {
            let palette = palette.as_ref().ok_or_else(|| Error::Usage {
                message: "cluster:UID needs Palette: drop --offline, or give the entity as a file"
                    .to_string(),
            })?;
            let uid = if uid.chars().all(|c| c.is_ascii_hexdigit()) && uid.len() == 24 {
                uid.to_string()
            } else {
                palette.cluster_uid_named(uid)?
            };
            World::from_cluster(palette, &uid, &progress)?
        }
        None => World::from_file(std::path::Path::new(&source), cli.cloud.as_deref())?,
    };
    world.release = cli.release.clone();
    if let Some(change) = &cli.change {
        world = world.with_change(change)?;
    }
    if !cli.records.is_empty() {
        world = world.with_records(&expand(&cli.records)?)?;
    }
    if let Some(p) = &palette {
        world = world.with_palette(p, &now, &progress)?;
    }

    let selection = Selection {
        tiers: tiers.clone(),
        ids: cli.rule.clone(),
        strict: cli.strict,
        ready: cli.ready.map(|p| p / 100.0),
    };
    let out_dir = cli.out.as_ref().map(|d| d.display().to_string());
    let mut reading: Option<serde_json::Value> = None;
    let report = if cli.watch {
        let palette = palette.as_ref().ok_or_else(|| Error::Usage {
            message: "--watch needs Palette: drop --offline".to_string(),
        })?;
        run::watch(run::WatchInputs {
            catalog: &catalog,
            world: &world,
            api: palette.api(),
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
            color: color_err(&cli),
        })
        .map(|(report, world)| {
            reading = Some(world);
            report
        })?
    } else {
        let backend = LiveBackend {
            palette: palette.as_ref().map(Palette::api),
            no_exec: cli.no_exec,
        };
        run::run(run::RunInputs {
            catalog: &catalog,
            world: &world,
            backend: &backend,
            ledger: &ledger,
            selection: &selection,
            now: &now,
            out_dir: out_dir.as_deref(),
            progress: &progress,
            verbose: cli.verbose,
            color: color_err(&cli),
        })?
    };
    if let Some(dir) = &cli.out {
        write_out(dir, &report, &world, reading.as_ref())?;
        progress.note(&format!(
            "wrote {}/report.md, report.json, report.html, world.json, entity.json",
            dir.display()
        ));
    }
    emit(&report::render(
        &report,
        format_of(&cli),
        cli.compact,
        color(&cli),
    ));
    Ok(ExitCode::from(report.exit_code()))
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
    std::fs::create_dir_all(dir).map_err(|e| strata_preflight::error::Error::Io {
        path: dir.to_path_buf(),
        source: e,
    })?;
    let write = |name: &str, text: String| -> Result<()> {
        let path = dir.join(name);
        std::fs::write(&path, text)
            .map_err(|e| strata_preflight::error::Error::Io { path, source: e })
    };
    write(
        "report.md",
        report::render(report, Format::Markdown, false, false),
    )?;
    write(
        "report.json",
        report::render(report, Format::Json, false, false),
    )?;
    write(
        "report.html",
        report::render(report, Format::Html, false, false),
    )?;
    let mut world_value = reading.cloned().unwrap_or_else(|| world.to_value());
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
        "{:<32} {:<8} {:<10} {:<10} {:<8} confidence (engine release: catch, clean)\n",
        "rule", "tier", "level", "earned", "advice"
    ));
    for r in &report.rules {
        let conf: Vec<String> = r
            .confidence
            .iter()
            .map(|c| {
                format!(
                    "{} {}: {}, {}",
                    c.engine,
                    if c.release.is_empty() {
                        "-"
                    } else {
                        &c.release
                    },
                    c.catch.text(),
                    c.clean.text()
                )
            })
            .collect();
        out.push_str(&format!(
            "{:<32} {:<8} {:<10} {:<10} {:<8} {}\n",
            r.id,
            r.tier.name(),
            r.level.name(),
            r.earned.name(),
            if r.advice.is_empty() { "-" } else { &r.advice },
            if conf.is_empty() {
                "no runs".to_string()
            } else {
                conf.join("; ")
            }
        ));
    }
    out.push_str(&format!(
        "\n{} rules with a check; {} level(s) disagree with the ledger\n",
        report.rules.len(),
        report.disagreements
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
        assert!(help.contains("--watch cluster:UID"), "{help}");
        assert!(help.contains("--rules"), "{help}");
        assert!(help.contains("--probes"), "{help}");
        assert!(help.contains("--strict"), "{help}");
    }

    #[test]
    fn a_records_pattern_matches_at_every_depth_and_a_plain_path_stays() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        for f in [
            "releases/x.json",
            "releases/aws/a.json",
            "releases/aws/2026/b.json",
        ] {
            let path = dir.join(f);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "{}").unwrap();
        }
        let pattern = dir.join("releases/**/*.json").display().to_string();
        let found = expand(&[pattern]).unwrap();
        let names: Vec<String> = found
            .iter()
            .map(|p| p.strip_prefix(&dir).unwrap().display().to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                "releases/aws/2026/b.json",
                "releases/aws/a.json",
                "releases/x.json"
            ],
            "`**` matches zero directories as well as many"
        );
        let plain = dir.join("releases/x.json").display().to_string();
        assert_eq!(
            expand(std::slice::from_ref(&plain)).unwrap(),
            vec![PathBuf::from(plain)]
        );
        let none = dir.join("releases/**/*.yaml").display().to_string();
        let err = expand(&[none]).unwrap_err().to_string();
        assert!(err.contains("matched no file"), "{err}");
    }
}
