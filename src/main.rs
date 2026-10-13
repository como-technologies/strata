//! The strata command line: argument parsing, printing, and exit codes. Everything else is in the
//! library.

use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches};
use strata::TOOL_NAME;
use strata::error::Result;

mod cli;

use cli::args::{Cli, Cmd};
use cli::behind::cmd_behind;
use cli::candidate::cmd_candidate;
use cli::compare::cmd_compare;
use cli::consumers::cmd_consumers;
use cli::list::cmd_list;
use cli::packs::cmd_packs;
use cli::preflight::cmd_preflight;
use cli::same_release::cmd_same_release;
use cli::schema::cmd_schema;
use cli::snapshot::cmd_snapshot;
use cli::verify::cmd_verify;

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
    // `strata preflight --help` is the help of strata-preflight, so it goes through
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_string())
        .filter(|name| name != "preflight")
        .collect();
    for name in names {
        cmd = cmd.mut_subcommand(name, |s| s.arg(help_flag()));
    }
    cmd
}

/// True when `arg` names a command of `cmd`, so `strata <A> <B>` does not become
/// `strata compare`. The caller builds `cmd`, so `help` is one of them.
fn is_command(cmd: &clap::Command, arg: &str) -> bool {
    cmd.find_subcommand(arg).is_some()
}

fn main() -> ExitCode {
    let mut cmd = command();
    cmd.build();
    let mut argv: Vec<String> = std::env::args().collect();
    let first = argv.get(1).map(String::as_str).unwrap_or("");
    let is_global_flag = matches!(first, "-h" | "--help" | "-V" | "--version");
    if argv.len() > 1 && !is_command(&cmd, first) && !is_global_flag {
        // `strata <A> <B>` is a compare, so a command name that is not first would become a ref
        // and fail as an unexpected argument of `compare`. Say what is wrong instead.
        if let Some(name) = argv[2..].iter().find(|a| is_command(&cmd, a)) {
            cmd.error(
                clap::error::ErrorKind::InvalidSubcommand,
                format!("'{name}' is a command, so it comes first: strata {name} ..."),
            )
            .exit();
        }
        argv.insert(1, "compare".to_string());
    }
    let matches = cmd.get_matches_from(argv);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_command_and_help_count_as_a_command_and_a_ref_does_not() {
        let mut built = command();
        built.build();
        for name in [
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
        ] {
            assert!(is_command(&built, name), "{name} is a command");
        }
        for text in [
            "aws-stack@1.2.0",
            "cluster:c1",
            "releases/dev.json",
            "64f0a1",
        ] {
            assert!(!is_command(&built, text), "{text} is a ref");
        }
    }

    #[test]
    fn the_help_screen_is_the_one_a_terminal_expects() {
        let mut built = command();
        built.build();
        let help = built.render_help().to_string();
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
        for name in built.get_subcommands().map(clap::Command::get_name) {
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
