//! How each kind of check runs, and what it can answer. A check reads the world, adds its own
//! answer to a copy of it, runs the program of the rule, and gives an [`Outcome`].
//!
//! The answers of Palette and of a command come through a [`Backend`], so a test runs the same
//! code on a fixture: [`FixtureBackend`] reads the answers from the world document itself, and
//! [`LiveBackend`] asks Palette and runs the command.

use std::collections::BTreeMap;
use std::process::Command;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use strata::api::codes::ValidateRoute;
use strata::api::{Method, PaletteApi};
use strata::closed_set::impl_closed_set;
use strata::closed_set::legend::Described;
use strata::config::KEY_VARS;
use strata::json::{flag, opt_flag, str_at, text_or, value_or_null};

use crate::error::{Error, Result};
use crate::jq::Jq;
use crate::rule::{Check, Rule};

/// The answer of Palette to one request: a 2xx is `ok`, a 4xx is an answer with its `V1Error`
/// body, and a 204 is `ok` with a `null` body. A refusal of the cluster validate route with HTTP
/// 500 is an answer too.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// True for a 2xx.
    pub ok: bool,
    /// The HTTP status, when known. A 2xx that came as JSON is `200`.
    pub status: u16,
    /// The body, as JSON when it is JSON, else as text.
    pub body: Value,
}

/// The output of one command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecOutput {
    /// The exit code, or `-1` for a signal or a timeout.
    pub exit: i32,
    /// Standard output, as JSON when it parses, else as text.
    pub stdout: Value,
    /// Standard error, as text.
    pub stderr: String,
}

/// Where a check gets an answer from outside the world.
pub trait Backend {
    /// One request to Palette. `Err` is a failure no check can read: no key, a transport error,
    /// a 5xx after the retries.
    fn palette(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Response>;
    /// True when the tool `name` is on this machine.
    fn has_tool(&self, name: &str) -> bool;
    /// Run one command. `Err` is a failure to start it.
    fn exec(&self, argv: &[String], timeout: Duration) -> Result<ExecOutput>;
    /// True when a Palette check can run: a key is there.
    fn has_palette(&self) -> bool;
    /// True when no command may run (`--no-exec`): each exec check is then `NOT RUN`.
    fn exec_off(&self) -> bool {
        false
    }
}

/// The backend of a run: Palette over HTTP, and the commands of this machine.
pub struct LiveBackend<'a, A: PaletteApi> {
    /// The Palette client, or none when no key is there.
    pub palette: Option<&'a A>,
    /// Run no command. Every exec check is then `NOT RUN`.
    pub no_exec: bool,
}

/// A 4xx that is the answer of a check: the route read the request and refused what it says. A
/// 401 or a 403 is the key, and a 429 is the rate: the check did not run, and the error names it.
pub fn is_answer(status: u16) -> bool {
    (400..500).contains(&status) && !matches!(status, 401 | 403 | 429)
}

/// A refusal of the cluster validate route that Palette gives with HTTP 500, such as a profile of
/// another cloud type. The route read the body and refused it, so the refusal is an answer, like a
/// 4xx. Another 5xx is a fault of Palette, and the check did not run.
fn is_refusal(status: u16, body: &str) -> bool {
    ValidateRoute::ClusterCreate.refuses_answer(status, body)
}

impl<A: PaletteApi> Backend for LiveBackend<'_, A> {
    fn palette(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Response> {
        let api = self.palette.ok_or_else(|| Error::Usage {
            message: format!(
                "a Palette check needs a key: set {}, or run with --tier static",
                KEY_VARS[0]
            ),
        })?;
        let answer = match method {
            Method::Get => api.get_json(path),
            Method::Post => api.post_json(path, body.unwrap_or(&Value::Null)),
        };
        match answer {
            Ok(body) => Ok(Response {
                ok: true,
                status: if body.is_null() { 204 } else { 200 },
                body,
            }),
            // a 4xx is what a validate route answers when the entity is wrong: it is the result.
            // The route also refuses with a 500 and a code that it names.
            Err(strata::Error::Api { status, body, .. })
                if is_answer(status) || is_refusal(status, &body) =>
            {
                // a body that is not JSON, such as a plain-text 404 of the gateway, is an object
                // with the text as its message, so a program reads `.body.message` either way
                let body = serde_json::from_str(&body)
                    .ok()
                    .filter(Value::is_object)
                    .unwrap_or_else(|| serde_json::json!({ "message": body }));
                Ok(Response {
                    ok: false,
                    status,
                    body,
                })
            }
            Err(e) => Err(e.into()),
        }
    }

    fn has_tool(&self, name: &str) -> bool {
        !self.no_exec && tool_on_path(name)
    }

    fn exec(&self, argv: &[String], timeout: Duration) -> Result<ExecOutput> {
        run_command(argv, timeout)
    }

    fn has_palette(&self) -> bool {
        self.palette.is_some()
    }

    fn exec_off(&self) -> bool {
        self.no_exec
    }
}

/// The backend of a fixture: the world document holds the answers. `_responses` maps
/// `"METHOD /path"` to a [`Response`], and `_exec` maps the command, as one string with a space
/// between the arguments, to an [`ExecOutput`]. `_tools` lists the tools that are "there"; with
/// no list, every tool is.
pub struct FixtureBackend {
    responses: BTreeMap<String, Response>,
    exec: BTreeMap<String, ExecOutput>,
    tools: Option<Vec<String>>,
    has_palette: bool,
}

impl FixtureBackend {
    /// Read the answers out of a fixture, and give back the world without them.
    pub fn split(mut fixture: Value) -> (FixtureBackend, Value) {
        let take = |doc: &mut Value, key: &str| doc.as_object_mut().and_then(|o| o.remove(key));
        let responses = take(&mut fixture, "_responses")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        let exec = take(&mut fixture, "_exec")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        let tools = take(&mut fixture, "_tools").and_then(|v| serde_json::from_value(v).ok());
        let has_palette = take(&mut fixture, "_palette")
            .and_then(|v| opt_flag(&v, ""))
            .unwrap_or(true);
        (
            FixtureBackend {
                responses,
                exec,
                tools,
                has_palette,
            },
            fixture,
        )
    }
}

impl Backend for FixtureBackend {
    fn palette(&self, method: Method, path: &str, _body: Option<&Value>) -> Result<Response> {
        let key = format!("{method} {path}");
        self.responses
            .get(&key)
            .cloned()
            .ok_or_else(|| Error::Input {
                message: format!("the fixture has no answer for {key} under _responses"),
            })
    }

    fn has_tool(&self, name: &str) -> bool {
        self.tools
            .as_ref()
            .is_none_or(|t| t.iter().any(|x| x == name))
    }

    fn exec(&self, argv: &[String], _timeout: Duration) -> Result<ExecOutput> {
        let key = argv.join(" ");
        self.exec.get(&key).cloned().ok_or_else(|| Error::Input {
            message: format!("the fixture has no output for `{key}` under _exec"),
        })
    }

    fn has_palette(&self) -> bool {
        self.has_palette
    }
}

/// What one check answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// The check ran, and found no fault.
    Passed,
    /// The check ran, and found the fault.
    Failed,
    /// The rule has no check strata-preflight can run here: a cloud rule with no tool, a race, or
    /// a row with no check. The reproduce lines say what to run by hand.
    NotChecked,
    /// The rule has a check that applies here, and the check did not run: the tier was not asked
    /// for (`--tier`, `--offline`), there was no key, or `--no-exec`. It counts against the
    /// readiness, because nothing shows that it passes.
    NotRun,
    /// The rule does not apply to this run: another release, a `when` that is not true, a rule of
    /// the tool that makes the entity, or a watch rule outside a watch.
    Skipped,
    /// The check could not answer: Palette did not answer, the command did not start, or the
    /// program failed. The exit code is `2`.
    Error,
}

impl_closed_set!(Verdict {
    Passed => "passed",
    Failed => "failed",
    NotChecked => "not-checked",
    NotRun => "not-run",
    Skipped => "skipped",
    Error => "error",
});

impl Described for Verdict {
    fn describe(self) -> &'static str {
        match self {
            Verdict::Passed => "the check ran and found no fault",
            Verdict::Failed => "the check ran and found the fault; the messages say what",
            Verdict::NotChecked => {
                "strata-preflight has no check it can run here; the reproduce lines say what to run by hand"
            }
            Verdict::NotRun => {
                "the check applies here and did not run: a tier not asked for, no key, or --no-exec; it counts against the readiness"
            }
            Verdict::Skipped => "the rule does not apply to this run",
            Verdict::Error => "the check could not answer",
        }
    }
}

impl Verdict {
    /// The word of the report.
    pub fn name(&self) -> &'static str {
        match self {
            Verdict::Passed => "PASS",
            Verdict::Failed => "FAIL",
            Verdict::NotChecked => "NOT CHECKED",
            Verdict::NotRun => "NOT RUN",
            Verdict::Skipped => "SKIP",
            Verdict::Error => "ERROR",
        }
    }
}

/// One line for a request or a command, with its answer when it has one: `GET /v1/x -> HTTP 200`.
pub(crate) fn ran_line(ran: &str, answer: Option<&str>) -> String {
    match answer {
        Some(answer) => format!("{ran} -> {answer}"),
        None => ran.to_string(),
    }
}

/// One request that a check made, with the answer to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Call {
    /// The method of the request.
    pub method: Method,
    /// The path of the request, with its query.
    pub path: String,
    /// The answer: `HTTP 200`. None when Palette gave no answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

impl Call {
    /// The request as a person writes it: the method and the path, `GET /v1/x`.
    pub fn ran(&self) -> String {
        format!("{} {}", self.method, self.path)
    }

    /// The request and its answer on one line: `GET /v1/x -> HTTP 200`.
    pub fn line(&self) -> String {
        ran_line(&self.ran(), self.answer.as_deref())
    }
}

#[cfg(test)]
impl Call {
    /// A `GET` request with its answer, for the tests of this crate.
    pub(crate) fn get(path: &str, answer: Option<&str>) -> Call {
        Call {
            method: Method::Get,
            path: path.into(),
            answer: answer.map(Into::into),
        }
    }
}

/// What a check ran, with the answer to it.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Ran {
    /// A static check, a check that did not run, or a check with `each` over an empty list.
    #[default]
    Nothing,
    /// The command of a check that runs a program, as a person runs it by hand. The answer is
    /// `exit 0`, and none when the command did not start.
    Exec {
        /// The command line.
        line: String,
        /// The exit code, as `exit 0`.
        answer: Option<String>,
    },
    /// The one request of a check of Palette.
    Request(Call),
    /// The requests of a check with `each`, one for each item, in the order of the list.
    Requests(Vec<Call>),
}

impl Ran {
    /// The request or the command of a check that makes one: what a person runs by hand. None for
    /// a check that made none, or one for each item.
    pub fn text(&self) -> Option<String> {
        match self {
            Ran::Exec { line, .. } => Some(line.clone()),
            Ran::Request(call) => Some(call.ran()),
            Ran::Nothing | Ran::Requests(_) => None,
        }
    }

    /// The answer to that one request or command: `HTTP 200`, or `exit 0`.
    pub fn answer(&self) -> Option<&str> {
        match self {
            Ran::Exec { answer, .. } => answer.as_deref(),
            Ran::Request(call) => call.answer.as_deref(),
            Ran::Nothing | Ran::Requests(_) => None,
        }
    }

    /// The requests of a check with `each`, one for each item, in the order of the list.
    pub fn requests(&self) -> &[Call] {
        match self {
            Ran::Requests(calls) => calls,
            Ran::Nothing | Ran::Exec { .. } | Ran::Request(_) => &[],
        }
    }
}

/// The result of one check.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// What it answered.
    pub verdict: Verdict,
    /// One line for each failure, or the reason for `NOT CHECKED`, `SKIP`, or `ERROR`.
    pub messages: Vec<String>,
    /// How long the check took.
    pub elapsed: Duration,
    /// The request or the command the check ran, for the report: what a person runs by hand.
    pub ran: Ran,
}

impl Outcome {
    fn new(verdict: Verdict, messages: Vec<String>, started: Instant) -> Outcome {
        Outcome {
            verdict,
            messages,
            elapsed: started.elapsed(),
            ran: Ran::Nothing,
        }
    }

    /// A check that did not run, with the reason.
    pub fn not_run(verdict: Verdict, reason: impl Into<String>) -> Outcome {
        Outcome {
            verdict,
            messages: vec![reason.into()],
            elapsed: Duration::ZERO,
            ran: Ran::Nothing,
        }
    }
}

/// Run the check of `rule` on `world`, through `backend`. A rule with no check is `NOT CHECKED`.
pub fn run_check(rule: &Rule, world: &Value, backend: &dyn Backend, jq: &Jq) -> Outcome {
    let started = Instant::now();
    let Some(check) = &rule.check else {
        return Outcome::not_run(
            Verdict::NotChecked,
            "no check in strata-preflight; the reproduce lines say what to run by hand",
        );
    };
    match check {
        Check::Static { expr } => judge(rule, expr, world, jq, started),
        Check::Palette {
            method,
            path,
            body,
            each,
            expr,
        } => run_palette(
            rule,
            &PaletteCheck {
                method: *method,
                path,
                body: body.as_deref(),
                each: each.as_deref(),
                expr,
            },
            world,
            backend,
            jq,
            started,
        ),
        Check::Exec {
            argv,
            needs,
            timeout,
            expr,
        } => {
            if backend.exec_off() {
                return Outcome::not_run(Verdict::NotRun, "--no-exec: the command did not run");
            }
            let missing: Vec<&str> = needs
                .iter()
                .map(String::as_str)
                .filter(|t| !backend.has_tool(t))
                .collect();
            let argv: Vec<String> = match argv
                .iter()
                .map(|a| jq.render(&rule.id, a, world, None))
                .collect::<Result<_>>()
            {
                Ok(a) => a,
                Err(e) => return Outcome::new(Verdict::Error, vec![e.to_string()], started),
            };
            let line = shell_words(&argv);
            if !missing.is_empty() {
                let mut out = Outcome::not_run(
                    Verdict::NotChecked,
                    format!(
                        "{} not on this machine; run by hand: {line}",
                        missing.join(", ")
                    ),
                );
                out.ran = Ran::Exec { line, answer: None };
                return out;
            }
            let limit = Duration::from_secs(timeout.unwrap_or(60));
            match backend.exec(&argv, limit) {
                Ok(output) => {
                    let mut with = world.clone();
                    with["exec"] = serde_json::to_value(&output).unwrap_or(Value::Null);
                    let mut out = judge(rule, expr, &with, jq, started);
                    out.ran = Ran::Exec {
                        line,
                        answer: Some(format!("exit {}", output.exit)),
                    };
                    out
                }
                Err(e) => {
                    let mut out = Outcome::new(Verdict::Error, vec![e.to_string()], started);
                    out.ran = Ran::Exec { line, answer: None };
                    out
                }
            }
        }
        // the watch adds the cluster, the summary, and the events; the caller of a watch does,
        // and then judges with `judge_watch`
        Check::Watch { .. } => Outcome::not_run(
            Verdict::Skipped,
            "a watch rule runs with --watch cluster:UID, during the build",
        ),
    }
}

/// The fields of a check of Palette, as the rule file gives them.
struct PaletteCheck<'a> {
    method: Method,
    path: &'a str,
    body: Option<&'a str>,
    each: Option<&'a str>,
    expr: &'a str,
}

/// The answer of Palette to the request for one item of a check with `each`. It holds the item,
/// and the response in the shape of `.response` of a check with no `each`.
#[derive(Serialize)]
struct ItemResponse<'a> {
    item: &'a Value,
    response: Response,
}

/// A request that went out, and the answer to it.
struct Asked {
    call: Call,
    response: Response,
}

/// A request that no check can read: the templates did not fill, or Palette did not answer.
struct Failed {
    /// The request as it went out, when the templates filled.
    call: Option<Call>,
    error: Error,
}

/// Fill the templates of the request for `item`, and ask Palette. A check with no `each` has no
/// item.
fn ask(
    rule: &Rule,
    check: &PaletteCheck<'_>,
    item: Option<&Value>,
    world: &Value,
    backend: &dyn Backend,
    jq: &Jq,
) -> std::result::Result<Asked, Box<Failed>> {
    let unfilled = |error| Box::new(Failed { call: None, error });
    let path = jq
        .render(&rule.id, check.path, world, item)
        .map_err(unfilled)?;
    let body = match (check.method, check.body) {
        (Method::Post, Some(program)) => {
            Some(jq.first(&rule.id, program, world, item).map_err(unfilled)?)
        }
        (Method::Post, None) => Some(value_or_null(world, "/entity")),
        (Method::Get, _) => None,
    };
    let method = check.method;
    match backend.palette(method, &path, body.as_ref()) {
        Ok(response) => {
            let answer = Some(format!("HTTP {}", response.status));
            Ok(Asked {
                call: Call {
                    method,
                    path,
                    answer,
                },
                response,
            })
        }
        Err(error) => Err(Box::new(Failed {
            call: Some(Call {
                method,
                path,
                answer: None,
            }),
            error,
        })),
    }
}

/// Run a check of Palette: one request, or one request for each item of `each`, then the program
/// of the rule.
fn run_palette(
    rule: &Rule,
    check: &PaletteCheck<'_>,
    world: &Value,
    backend: &dyn Backend,
    jq: &Jq,
    started: Instant,
) -> Outcome {
    if !backend.has_palette() {
        return Outcome::not_run(
            Verdict::NotRun,
            format!("no key: set {} to run the Palette checks", KEY_VARS[0]),
        );
    }
    let Some(each) = check.each else {
        return match ask(rule, check, None, world, backend, jq) {
            Ok(Asked { call, response }) => {
                let mut with = world.clone();
                with["response"] = serde_json::to_value(&response).unwrap_or(Value::Null);
                let mut out = judge(rule, check.expr, &with, jq, started);
                out.ran = Ran::Request(call);
                out
            }
            Err(failed) => {
                let mut out = Outcome::new(Verdict::Error, vec![failed.error.to_string()], started);
                if let Some(call) = failed.call {
                    out.ran = Ran::Request(call);
                }
                out
            }
        };
    };
    let items = match jq.first(&rule.id, each, world, None) {
        Ok(Value::Array(items)) => items,
        Ok(other) => {
            return Outcome::new(
                Verdict::Error,
                vec![format!("`each` gave {other}; give a list")],
                started,
            );
        }
        Err(e) => return Outcome::new(Verdict::Error, vec![e.to_string()], started),
    };
    let mut calls = Vec::with_capacity(items.len());
    let mut responses = Vec::with_capacity(items.len());
    for item in &items {
        match ask(rule, check, Some(item), world, backend, jq) {
            Ok(asked) => {
                calls.push(asked.call);
                responses.push(ItemResponse {
                    item,
                    response: asked.response,
                });
            }
            Err(failed) => {
                calls.extend(failed.call);
                let mut out = Outcome::new(Verdict::Error, vec![failed.error.to_string()], started);
                out.ran = Ran::Requests(calls);
                return out;
            }
        }
    }
    let mut with = world.clone();
    // a derived `Serialize` of plain data does not fail
    with["responses"] = serde_json::to_value(&responses).unwrap_or(Value::Null);
    let mut out = judge(rule, check.expr, &with, jq, started);
    out.ran = Ran::Requests(calls);
    out
}

/// Judge one reading of a watch: `world` holds `cluster`, `summary`, `events`, and `elapsed`.
pub fn judge_watch(rule: &Rule, world: &Value, jq: &Jq) -> Outcome {
    let started = Instant::now();
    match &rule.check {
        Some(Check::Watch { expr }) => judge(rule, expr, world, jq, started),
        _ => Outcome::not_run(Verdict::Skipped, "not a watch rule"),
    }
}

/// Run the program and read its output: `true`/`false`, a list of messages, or `{pass,
/// message}`. Several outputs must all pass.
fn judge(rule: &Rule, expr: &str, world: &Value, jq: &Jq, started: Instant) -> Outcome {
    let outputs = match jq.run(&rule.id, expr, world, None) {
        Ok(o) => o,
        Err(e) => return Outcome::new(Verdict::Error, vec![e.to_string()], started),
    };
    if outputs.is_empty() {
        return Outcome::new(
            Verdict::Error,
            vec!["the program gave no output; give true, false, a list of messages, or {pass, message}".into()],
            started,
        );
    }
    let mut failures = Vec::new();
    for out in outputs {
        match &out {
            Value::Bool(true) => {}
            Value::Bool(false) => failures.push("the check is false".to_string()),
            Value::Array(items) => {
                for item in items {
                    failures.push(one_line(match item {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    }));
                }
            }
            Value::Object(_) => {
                if !flag(&out, "/pass") {
                    failures.push(one_line(
                        text_or(&out, "/message", "the check is false").to_string(),
                    ));
                }
            }
            // null is a path that is not in the world, not a pass: a wrong program says so
            other => {
                return Outcome::new(
                    Verdict::Error,
                    vec![format!(
                        "the program gave {other}; give true, false, a list of messages, or {{pass, message}}"
                    )],
                    started,
                );
            }
        }
    }
    if failures.is_empty() {
        Outcome::new(Verdict::Passed, vec![], started)
    } else {
        Outcome::new(Verdict::Failed, failures, started)
    }
}

/// A message on one line: the report prints each message under its rule, and a line break inside
/// one, such as the output of a tool, would leave the column.
pub(crate) fn one_line(s: String) -> String {
    if s.contains(['\n', '\r']) {
        s.split(['\n', '\r'])
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        s
    }
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
fn run_command(argv: &[String], timeout: Duration) -> Result<ExecOutput> {
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

/// The `V1Error` of Palette as one line: `code: message`.
pub fn palette_error_line(body: &Value) -> String {
    match (body.get("code"), body.get("message")) {
        (Some(_), Some(_)) => format!("{}: {}", str_at(body, "/code"), str_at(body, "/message")),
        _ => body.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{Level, Severity, Status};
    use serde_json::json;

    fn rule(check: Check) -> Rule {
        Rule {
            id: "t".into(),
            title: "t".into(),
            section: "s".into(),
            engines: vec![strata::cloud::CLOUD_ALL.into()],
            tier: check.tier(),
            level: Level::Probe,
            severity: Severity::Fail,
            status: Status::Seen,
            releases: None,
            what: "w".into(),
            when: None,
            check: Some(check),
            fix: None,
            reproduce: Default::default(),
            evidence: vec![],
            promotion: None,
            tests: vec![],
        }
    }

    fn fixture(v: Value) -> (FixtureBackend, Value) {
        FixtureBackend::split(v)
    }

    #[test]
    fn a_static_check_reads_the_three_output_shapes() {
        let jq = Jq::new();
        let (b, w) = fixture(json!({"n": 4}));
        let pass = run_check(
            &rule(Check::Static {
                expr: ".n == 4".into(),
            }),
            &w,
            &b,
            &jq,
        );
        assert_eq!(pass.verdict, Verdict::Passed);
        let fail = run_check(
            &rule(Check::Static {
                expr: ".n == 3".into(),
            }),
            &w,
            &b,
            &jq,
        );
        assert_eq!(fail.verdict, Verdict::Failed);
        let list = run_check(
            &rule(Check::Static {
                expr: r#"[if .n % 3 != 0 then "size \(.n) is not a multiple of 3" else empty end]"#
                    .into(),
            }),
            &w,
            &b,
            &jq,
        );
        assert_eq!(list.messages, vec!["size 4 is not a multiple of 3"]);
        let obj = run_check(
            &rule(Check::Static {
                expr: r#"{pass: (.n > 3), message: "small"}"#.into(),
            }),
            &w,
            &b,
            &jq,
        );
        assert_eq!(obj.verdict, Verdict::Passed);
        let bad = run_check(&rule(Check::Static { expr: ".n".into() }), &w, &b, &jq);
        assert_eq!(bad.verdict, Verdict::Error);
        assert!(bad.messages[0].contains("gave 4"), "{:?}", bad.messages);
    }

    #[test]
    fn a_program_that_gives_null_is_an_error_and_not_a_pass() {
        let jq = Jq::new();
        let (b, w) = fixture(json!({"cloud": "gke", "entity": {"spec": {"items": []}}}));
        let typo = rule(Check::Static {
            expr: ".entity.spec.itmes | length == 0".into(),
        });
        let out = run_check(&typo, &w, &b, &jq);
        assert_eq!(out.verdict, Verdict::Passed, "length of null is 0 in jq");
        let path = rule(Check::Static {
            expr: ".entity.spec.itmes".into(),
        });
        let out = run_check(&path, &w, &b, &jq);
        assert_eq!(out.verdict, Verdict::Error);
        assert!(out.messages[0].contains("gave null"), "{:?}", out.messages);
    }

    #[test]
    fn a_palette_check_reads_a_4xx_as_an_answer() {
        let jq = Jq::new();
        let (b, w) = fixture(json!({"cloud": "gke", "entity": {"spec": {}},
            "_responses": {"POST /v1/spectroclusters/gke/validate": {"ok": false, "status": 400,
                "body": {"code": "InvalidParam", "message": "Node pool size should be multiple of 3"}}}}));
        let r = rule(Check::Palette {
            method: Method::Post,
            path: "/v1/spectroclusters/{{ .cloud }}/validate".into(),
            body: None,
            each: None,
            expr: r#"[select(.response.ok | not) | "Palette: \(.response.body.code): \(.response.body.message)"]"#.into(),
        });
        let out = run_check(&r, &w, &b, &jq);
        assert_eq!(out.verdict, Verdict::Failed);
        assert_eq!(
            out.messages,
            vec!["Palette: InvalidParam: Node pool size should be multiple of 3"]
        );
        assert_eq!(
            out.ran.text().as_deref(),
            Some("POST /v1/spectroclusters/gke/validate")
        );
    }

    fn each_check(each: &str, path: &str, expr: &str) -> Rule {
        rule(Check::Palette {
            method: Method::Get,
            path: path.into(),
            body: None,
            each: Some(each.into()),
            expr: expr.into(),
        })
    }

    #[test]
    fn a_palette_check_with_each_asks_once_for_each_item_and_reads_the_responses_in_order() {
        let jq = Jq::new();
        let ok =
            |name: &str| json!({"ok": true, "status": 200, "body": {"metadata": {"name": name}}});
        let (b, w) = fixture(
            json!({"entity": {"spec": {"profiles": [{"uid": "p1"}, {"uid": "p2"}]}},
            "_responses": {"GET /v1/clusterprofiles/p1": ok("base"),
                "GET /v1/clusterprofiles/p2": {"ok": false, "status": 404, "body": {"code": "ResourceNotFound"}}}}),
        );
        let r = each_check(
            ".entity.spec.profiles",
            "/v1/clusterprofiles/{{ $item.uid }}",
            r#"[.responses[] | "\(.item.uid) \(.response.status) \(.response.body.metadata.name // .response.body.code)"]"#,
        );
        let out = run_check(&r, &w, &b, &jq);
        assert_eq!(out.verdict, Verdict::Failed);
        assert_eq!(out.messages, vec!["p1 200 base", "p2 404 ResourceNotFound"]);
        assert_eq!(
            out.ran,
            Ran::Requests(vec![
                Call::get("/v1/clusterprofiles/p1", Some("HTTP 200")),
                Call::get("/v1/clusterprofiles/p2", Some("HTTP 404")),
            ])
        );
        assert_eq!(out.ran.text(), None);
    }

    #[test]
    fn a_post_with_each_fills_its_body_from_the_item() {
        struct Echo(std::cell::RefCell<Vec<(String, Value)>>);
        impl Backend for Echo {
            fn palette(&self, _m: Method, path: &str, body: Option<&Value>) -> Result<Response> {
                self.0
                    .borrow_mut()
                    .push((path.to_string(), body.cloned().unwrap_or(Value::Null)));
                Ok(Response {
                    ok: true,
                    status: 200,
                    body: Value::Null,
                })
            }
            fn has_tool(&self, _: &str) -> bool {
                true
            }
            fn exec(&self, _: &[String], _: Duration) -> Result<ExecOutput> {
                unreachable!("no command in this test")
            }
            fn has_palette(&self) -> bool {
                true
            }
        }
        let echo = Echo(Default::default());
        let r = rule(Check::Palette {
            method: Method::Post,
            path: "/v1/capacity?arn={{ $item.arn }}".into(),
            body: Some("{arn: $item.arn, size: ($item.size + 1)}".into()),
            each: Some("[.pools[] | select(.arn != null)]".into()),
            expr: "[.responses[] | select(.response.ok | not) | .item.arn]".into(),
        });
        let world =
            json!({"pools": [{"arn": "a1", "size": 1}, {"size": 9}, {"arn": "a2", "size": 2}]});
        let out = run_check(&r, &world, &echo, &Jq::new());
        assert_eq!(out.verdict, Verdict::Passed);
        assert_eq!(
            *echo.0.borrow(),
            vec![
                (
                    "/v1/capacity?arn=a1".to_string(),
                    json!({"arn": "a1", "size": 2})
                ),
                (
                    "/v1/capacity?arn=a2".to_string(),
                    json!({"arn": "a2", "size": 3})
                ),
            ]
        );
    }

    #[test]
    fn a_check_with_each_over_no_item_asks_nothing_and_passes_the_empty_list() {
        let jq = Jq::new();
        let (b, w) = fixture(json!({"hosts": []}));
        let r = each_check(
            ".hosts",
            "/v1/edgehosts/{{ $item }}",
            "[.responses[] | .item]",
        );
        let out = run_check(&r, &w, &b, &jq);
        assert_eq!(out.verdict, Verdict::Passed);
        assert_eq!(out.ran, Ran::Requests(vec![]));
    }

    #[test]
    fn a_check_with_each_is_an_error_when_each_gives_no_list_or_a_request_fails() {
        let jq = Jq::new();
        let (b, w) = fixture(json!({"hosts": "h1", "list": ["h1", "h2"],
            "_responses": {"GET /v1/edgehosts/h1": {"ok": true, "status": 200, "body": {}}}}));
        let out = run_check(
            &each_check(".hosts", "/v1/edgehosts/{{ $item }}", "true"),
            &w,
            &b,
            &jq,
        );
        assert_eq!(out.verdict, Verdict::Error);
        assert_eq!(out.messages, vec![r#"`each` gave "h1"; give a list"#]);
        let out = run_check(
            &each_check(".missing", "/v1/edgehosts/{{ $item }}", "true"),
            &w,
            &b,
            &jq,
        );
        assert_eq!(out.verdict, Verdict::Error);
        assert_eq!(out.messages, vec!["`each` gave null; give a list"]);
        // the second request has no answer: the check is an error, and the report keeps both requests
        let out = run_check(
            &each_check(".list", "/v1/edgehosts/{{ $item }}", "true"),
            &w,
            &b,
            &jq,
        );
        assert_eq!(out.verdict, Verdict::Error);
        assert!(
            out.messages[0].contains("GET /v1/edgehosts/h2"),
            "{:?}",
            out.messages
        );
        assert_eq!(
            out.ran,
            Ran::Requests(vec![
                Call::get("/v1/edgehosts/h1", Some("HTTP 200")),
                Call::get("/v1/edgehosts/h2", None),
            ])
        );
    }

    #[test]
    fn a_401_a_403_and_a_429_are_errors_of_the_check_and_a_404_is_its_answer() {
        for status in [401u16, 403, 429] {
            let api = strata::api::MemoryApi::new().failing("/v1/x", status, "{}");
            let b = LiveBackend {
                palette: Some(&api),
                no_exec: false,
            };
            let err = b
                .palette(Method::Get, "/v1/x", None)
                .unwrap_err()
                .to_string();
            assert!(err.contains(&format!("HTTP {status}")), "{status}: {err}");
        }
        let api = strata::api::MemoryApi::new().failing("/v1/x", 404, r#"{"code": "NotFound"}"#);
        let b = LiveBackend {
            palette: Some(&api),
            no_exec: false,
        };
        let r = b.palette(Method::Get, "/v1/x", None).unwrap();
        assert!(!r.ok);
        assert_eq!(r.status, 404);
        assert_eq!(r.body["code"], "NotFound");
    }

    #[test]
    fn a_palette_check_with_no_key_is_not_run() {
        let jq = Jq::new();
        let (b, w) = fixture(json!({"cloud": "gke", "_palette": false}));
        let r = rule(Check::Palette {
            method: Method::Get,
            path: "/v1/x".into(),
            body: None,
            each: None,
            expr: "true".into(),
        });
        assert_eq!(run_check(&r, &w, &b, &jq).verdict, Verdict::NotRun);
    }

    #[test]
    fn an_exec_check_needs_its_tool_and_reads_the_output() {
        let jq = Jq::new();
        let (b, w) = fixture(json!({"region": "us-east-2", "_tools": ["aws"],
            "_exec": {"aws ec2 describe-subnets --region us-east-2": {"exit": 0, "stdout": {"Subnets": [{"AvailabilityZone": "us-east-2a"}]}, "stderr": ""}}}));
        let r = rule(Check::Exec {
            argv: vec!["aws".into(), "ec2".into(), "describe-subnets".into(), "--region".into(), "{{ .region }}".into()],
            needs: vec!["aws".into()],
            timeout: None,
            expr: r#"[.exec.stdout.Subnets | map(.AvailabilityZone) | unique | select(length < 2) | "one zone: \(.)"]"#.into(),
        });
        let out = run_check(&r, &w, &b, &jq);
        assert_eq!(out.verdict, Verdict::Failed);
        assert_eq!(out.messages, vec![r#"one zone: ["us-east-2a"]"#]);
        assert_eq!(
            out.ran,
            Ran::Exec {
                line: "aws ec2 describe-subnets --region us-east-2".into(),
                answer: Some("exit 0".into())
            }
        );
        let missing = rule(Check::Exec {
            argv: vec!["gcloud".into(), "services".into(), "list".into()],
            needs: vec!["gcloud".into()],
            timeout: None,
            expr: "true".into(),
        });
        let out = run_check(&missing, &w, &b, &jq);
        assert_eq!(out.verdict, Verdict::NotChecked);
        assert!(
            out.messages[0].contains("gcloud not on this machine"),
            "{:?}",
            out.messages
        );
    }

    #[test]
    fn a_real_command_runs_with_a_time_limit() {
        let ok = run_command(
            &["sh".into(), "-c".into(), "echo '{\"a\":1}'".into()],
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(ok.exit, 0);
        assert_eq!(ok.stdout, json!({"a": 1}));
        let text = run_command(
            &[
                "sh".into(),
                "-c".into(),
                "echo hi; echo err >&2; exit 3".into(),
            ],
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(
            (text.exit, text.stdout.as_str(), text.stderr.as_str()),
            (3, Some("hi"), "err")
        );
        let slow = run_command(&["sleep".into(), "5".into()], Duration::from_millis(200)).unwrap();
        assert_eq!(slow.exit, -1);
        assert!(slow.stderr.starts_with("stopped after"), "{}", slow.stderr);
        assert!(run_command(&["no-such-tool-xyz".into()], Duration::from_secs(1)).is_err());
    }

    #[test]
    fn a_message_with_a_line_break_prints_on_one_line() {
        let jq = Jq::new();
        let (b, w) = fixture(json!({"e": "aws failed: \nNoCredentials\n"}));
        let out = run_check(
            &rule(Check::Static {
                expr: r#"["tool said: \(.e)"]"#.into(),
            }),
            &w,
            &b,
            &jq,
        );
        assert_eq!(out.messages, vec!["tool said: aws failed: NoCredentials"]);
    }

    #[test]
    fn shell_words_quotes_what_needs_it() {
        assert_eq!(
            shell_words(&["a".into(), "b c".into(), "d'e".into()]),
            "a 'b c' 'd'\\''e'"
        );
    }
}
