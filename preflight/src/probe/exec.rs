//! The commands of an exec check: the arguments filled from the world, the tools the command
//! needs, the word list a person types, and the run with a time limit.

use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::backend::{Backend, ExecOutput};
use crate::error::{Error, Result};
use crate::jq::Jq;
use crate::rule::Rule;

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

/// The command as a person types it: an argument with a space or a quote is quoted.
pub fn shell_words(argv: &[String]) -> String {
    argv.iter()
        .map(|a| {
            if a.is_empty()
                || a.chars()
                    .any(|c| c.is_whitespace() || "\"'$`\\|&;<>()*?[]{}".contains(c))
            {
                format!("'{}'", a.replace('\'', "'\\''"))
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
