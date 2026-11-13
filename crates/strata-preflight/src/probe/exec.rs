//! The commands of an exec check: the arguments filled from the world and the tools the command
//! needs. It also holds the word list a person types and the run with a time limit.

use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::Value;
use strata::config::KEY_VARS;

use super::backend::{Backend, ExecOutput};
use super::outcome::{Invocation, Outcome, Ran, Verdict, by_hand};
use super::palette::{Asked, read as read_palette};
use super::{judge, put_data};
use crate::error::{Error, Result};
use crate::jq::Jq;
use crate::rule::{ExecCheck, PaletteRead, Rule};

/// Run an exec check. It reads Palette first when the check has a read, and then runs the command,
/// and the second command when the check has one. The program of the rule judges the world with
/// `.exec`, with `.also` after a second command, and with `.response` after a read.
pub(super) fn run_exec(
    rule: &Rule,
    check: &ExecCheck,
    world: &Value,
    backend: &dyn Backend,
    jq: &Jq,
    started: Instant,
) -> Outcome {
    if backend.exec_off() {
        return Outcome::not_run(Verdict::NotRun, "--no-exec: the command did not run");
    }
    let (argv, also) = match (
        render_argv(rule, &check.argv, world, jq),
        render_argv(rule, &check.also, world, jq),
    ) {
        (Ok(argv), Ok(also)) => (argv, also),
        (Err(e), _) | (_, Err(e)) => {
            return Outcome::new(Verdict::Error, vec![e.to_string()], started);
        }
    };
    let line = shell_words(&argv);
    let also_line = (!also.is_empty()).then(|| shell_words(&also));
    let missing = missing_tools(&check.needs, backend);
    if !missing.is_empty() {
        let to_run = by_hand(&line, also_line.as_deref());
        let mut out = Outcome::not_run(
            Verdict::NotChecked,
            format!(
                "{} not on this machine; run by hand: {to_run}",
                missing.join(", ")
            ),
        );
        out.ran = Ran::Exec {
            first: Invocation::not_run(line),
            also: also_line.map(Invocation::not_run),
            request: None,
        };
        return out;
    }
    let mut with = world.clone();
    let mut request = None;
    if let Some(read) = &check.palette {
        match read_first(rule, read, world, backend, jq, started) {
            Ok(asked) => {
                put_data(&mut with, "response", &asked.response);
                request = Some(asked.call);
            }
            Err(out) => return *out,
        }
    }
    let limit = Duration::from_secs(check.timeout.unwrap_or(60));
    match run_commands(backend, &argv, &also, limit) {
        Ok((output, second)) => {
            put_data(&mut with, "exec", &output);
            if let Some(second) = &second {
                put_data(&mut with, "also", second);
            }
            let mut out = judge(rule, &check.expr, &with, jq, started);
            out.ran = Ran::Exec {
                first: Invocation::ended(line, output.exit),
                // the second command has a line and an output together, or neither
                also: also_line
                    .zip(second)
                    .map(|(line, second)| Invocation::ended(line, second.exit)),
                request,
            };
            out
        }
        Err(e) => {
            let mut out = Outcome::new(Verdict::Error, vec![e.to_string()], started);
            out.ran = Ran::Exec {
                first: Invocation::not_run(line),
                also: also_line.map(Invocation::not_run),
                request,
            };
            out
        }
    }
}

/// The output of the command `argv`, and the output of the second command `also` when it has
/// words. The second command runs after the first, whatever the exit code of the first is.
fn run_commands(
    backend: &dyn Backend,
    argv: &[String],
    also: &[String],
    limit: Duration,
) -> Result<(ExecOutput, Option<ExecOutput>)> {
    let output = backend.exec(argv, limit)?;
    let second = if also.is_empty() {
        None
    } else {
        Some(backend.exec(also, limit)?)
    };
    Ok((output, second))
}

/// The read of Palette that an exec check makes before its command. The `Err` is the outcome of
/// the check when the read gives no answer: no key, or a failure that no program can read.
fn read_first(
    rule: &Rule,
    read: &PaletteRead,
    world: &Value,
    backend: &dyn Backend,
    jq: &Jq,
    started: Instant,
) -> std::result::Result<Asked, Box<Outcome>> {
    if !backend.has_palette() {
        return Err(Box::new(Outcome::not_run(
            Verdict::NotRun,
            format!(
                "no key: set {} to run this check, which reads Palette before the command",
                KEY_VARS[0]
            ),
        )));
    }
    read_palette(rule, read, world, backend, jq).map_err(|failed| {
        let mut out = Outcome::new(Verdict::Error, vec![failed.error.to_string()], started);
        if let Some(call) = failed.call {
            out.ran = Ran::Request(call);
        }
        Box::new(out)
    })
}

/// Each argument of the command of `rule`, with its `{{ jq }}` templates filled from `world`.
pub(super) fn render_argv(
    rule: &Rule,
    argv: &[String],
    world: &Value,
    jq: &Jq,
) -> Result<Vec<String>> {
    argv.iter()
        .map(|a| jq.render(&rule.id, a, world, None))
        .collect()
}

/// The tools of `needs` that are not on this machine.
pub(super) fn missing_tools<'a>(needs: &'a [String], backend: &dyn Backend) -> Vec<&'a str> {
    needs
        .iter()
        .map(String::as_str)
        .filter(|t| !backend.has_tool(t))
        .collect()
}

/// The command as a person types it: an argument with a space or a quote is quoted, by the rule
/// of [`strata::text::shell_words`].
pub fn shell_words(argv: &[String]) -> String {
    strata::text::shell_words(argv)
}

/// True when `name` is an executable on the PATH.
pub fn tool_on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let p = dir.join(name);
        p.is_file()
    })
}

/// Run `argv` with a time limit. Standard input is closed, and the environment is this
/// process's, so the tools of the cloud find their own credentials.
pub(super) fn run_command(argv: &[String], timeout: Duration) -> Result<ExecOutput> {
    let (program, args) = argv.split_first().ok_or_else(|| Error::Usage {
        message: "an exec check needs a command".to_string(),
    })?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| Error::Input {
            message: format!("cannot start {program}: {e}"),
        })?;
    let started = Instant::now();
    // read both pipes on threads, so a command that writes much to one does not block on the other
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = std::io::Read::read_to_end(s, &mut buf);
        }
        buf
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = stderr.as_mut() {
            let _ = std::io::Read::read_to_end(s, &mut buf);
        }
        buf
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break None,
        }
    };
    let out = out_thread.join().unwrap_or_default();
    let err = err_thread.join().unwrap_or_default();
    let stdout_text = String::from_utf8_lossy(&out).into_owned();
    let stdout = serde_json::from_str(&stdout_text)
        .unwrap_or(Value::String(stdout_text.trim_end().to_string()));
    let mut stderr = String::from_utf8_lossy(&err).trim_end().to_string();
    let exit = match status {
        Some(s) => s.code().unwrap_or(-1),
        None => {
            stderr = format!(
                "stopped after {}s{}",
                timeout.as_secs(),
                if stderr.is_empty() {
                    String::new()
                } else {
                    format!(": {stderr}")
                }
            );
            -1
        }
    };
    Ok(ExecOutput {
        exit,
        stdout,
        stderr,
    })
}
