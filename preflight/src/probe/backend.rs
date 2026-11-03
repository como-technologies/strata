//! The backends: where a check gets an answer from outside the world. [`LiveBackend`] asks Palette
//! and runs the command, and [`FixtureBackend`] reads the answers from the world document.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use strata::api::codes::ValidateRoute;
use strata::api::{Method, PaletteApi};
use strata::config::KEY_VARS;
use strata::json::{opt_flag, opt_str};

use super::exec::{run_command, tool_on_path};
use crate::error::{Error, Result};

/// The answer of Palette to one request.
///
/// A 2xx is `ok`, a 4xx is an answer with its `V1Error` body, and a 204 is `ok` with a `null`
/// body. A refusal of the cluster validate route with HTTP 500 is an answer too. So is a 5xx with
/// a code that the check names in `refusal_codes`.
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
    /// One request to Palette. `refusal_codes` are the codes that the check reads as an answer
    /// also at a 5xx.
    ///
    /// # Errors
    ///
    /// [`Error::Palette`] for a failure that no check can read: no key, a transport error, or a
    /// 5xx after the retries. A 4xx is a [`Response`]. So is a 5xx whose `code` is in
    /// `refusal_codes`.
    fn palette(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        refusal_codes: &[String],
    ) -> Result<Response>;
    /// True when the tool `name` is on this machine.
    fn has_tool(&self, name: &str) -> bool;
    /// Run one command.
    ///
    /// # Errors
    ///
    /// [`Error::Usage`] for an empty command, and [`Error::Input`] when the command cannot start.
    /// A command that starts and fails gives an [`ExecOutput`], and no error.
    fn exec(&self, argv: &[String], timeout: Duration) -> Result<ExecOutput>;
    /// True when a Palette check can run: a key is there.
    fn has_palette(&self) -> bool;
    /// True when no command may run (`--no-exec`): each exec check is then `NOT RUN`.
    fn exec_off(&self) -> bool {
        false
    }
}

/// Who reads a 4xx of a route, and so which 4xx is an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader {
    /// A check of a rule. A 403 is the key: the check did not run, and the error names it.
    Check,
    /// The world of a run, the watch, or the contract run. A 404 is one case. The 403 of a project
    /// key on an object of the tenant is another, so the object is not there for this key. The
    /// world holds `null`, and a rule says so.
    World,
}

/// True when the 4xx `status` is the answer of a route for `reader`: the route read the request
/// and refused what it says.
///
/// A 401 is the key and a 429 is the rate for each reader. The run stops with the error that names
/// it, because the status says nothing about the object. A 403 depends on the reader. Another 4xx
/// is the answer of Palette to the request.
pub fn is_answer(status: u16, reader: Reader) -> bool {
    let key_or_rate = match reader {
        Reader::Check => matches!(status, 401 | 403 | 429),
        Reader::World => matches!(status, 401 | 429),
    };
    (400..500).contains(&status) && !key_or_rate
}

/// True when a 5xx is the answer of a check: the `code` of its body is one that the check names in
/// `refusal_codes`. Palette answers some refusals by design with HTTP 500, and a retry gets the
/// same refusal. A 5xx with another code, or with no code, is an error.
fn is_named_refusal(status: u16, error: &Value, refusal_codes: &[String]) -> bool {
    (500..600).contains(&status)
        && opt_str(error, "/code").is_some_and(|code| refusal_codes.iter().any(|c| c == code))
}

/// True when the check reads the HTTP error `status` as the answer of the route. That is a 4xx
/// that is no key or rate error, a refusal of a validate route, or a 5xx that the check names.
/// `error` is the body of the Palette error, parsed, or `None` when it is not JSON.
fn reads_error(
    method: Method,
    path: &str,
    status: u16,
    error: Option<&Value>,
    refusal_codes: &[String],
) -> bool {
    is_answer(status, Reader::Check)
        || error.is_some_and(|error| {
            ValidateRoute::refuses_request(method, path, status, error)
                || is_named_refusal(status, error, refusal_codes)
        })
}

/// The error that Palette gave to the request `method` `path`: an HTTP error with its body as text.
fn http_error(method: Method, path: &str, status: u16, body: &str) -> strata::Error {
    strata::Error::Api {
        method,
        path: path.to_string(),
        status,
        body: body.to_string(),
        retry_after: None,
    }
}

/// What the backend keeps of the answer to one request. An answer is the same for the rest of
/// the run, because a run changes nothing in Palette.
#[derive(Debug, Clone)]
enum Kept {
    /// A 2xx: the body, as JSON. A 204 has a `null` body.
    Value(Value),
    /// An HTTP error that is an answer. It is a 4xx that a check reads. It is also the refusal of
    /// a validate route, such as a profile of another cloud type with HTTP 500. A check that names
    /// the code of a 5xx in `refusal_codes` reads that 5xx too, and no other check does.
    Answer {
        status: u16,
        /// The body as Palette sent it.
        text: String,
        /// The body as an object, so a program reads `.body.message` either way. A body that is
        /// not JSON, such as a plain-text 404 of the gateway, is an object with the text as its
        /// message.
        body: Value,
    },
}

impl Kept {
    /// Make sure that a check that names `refusal_codes` reads this answer: the error that Palette
    /// gave, when it does not. An answer that was kept for a check that named its code is an error
    /// for a check that does not.
    fn read_by(&self, method: Method, path: &str, refusal_codes: &[String]) -> Result<()> {
        match self {
            Kept::Answer { status, text, body }
                if !reads_error(method, path, *status, Some(body), refusal_codes) =>
            {
                Err(http_error(method, path, *status, text).into())
            }
            _ => Ok(()),
        }
    }

    fn response(&self) -> Response {
        match self {
            Kept::Value(body) => Response {
                ok: true,
                status: if body.is_null() { 204 } else { 200 },
                body: body.clone(),
            },
            Kept::Answer { status, body, .. } => Response {
                ok: false,
                status: *status,
                body: body.clone(),
            },
        }
    }
}

/// The backend of a run: Palette over HTTP, and the commands of this machine.
///
/// The backend keeps the answer of each request, by its method, its path, and its body, for the
/// run. So a rule that asks what another rule asked, or what the world read, costs no request.
/// An error that no check can read is not kept: the next request tries again.
///
/// The backend is a [`PaletteApi`] too. The world of a run reads through it, and shares its
/// answers with the checks.
pub struct LiveBackend<'a, A: PaletteApi> {
    palette: Option<&'a A>,
    no_exec: bool,
    kept: RefCell<BTreeMap<String, Kept>>,
}

impl<'a, A: PaletteApi> LiveBackend<'a, A> {
    /// A backend over the Palette client `palette`, or over none when no key is there. With
    /// `no_exec`, every exec check is `NOT RUN`.
    pub fn new(palette: Option<&'a A>, no_exec: bool) -> Self {
        LiveBackend {
            palette,
            no_exec,
            kept: RefCell::new(BTreeMap::new()),
        }
    }

    /// What `read` makes of the answer to a request. The answer is the one that the run kept for
    /// this request, else the one that Palette gives now. `refusal_codes` are the codes that the
    /// check reads at a 5xx. The request is the same for each check, so the answer is the same,
    /// and only the reading of a 5xx differs. `read` borrows the answer, so a kept answer is not
    /// cloned.
    fn ask<R>(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        refusal_codes: &[String],
        read: impl FnOnce(&Kept) -> R,
    ) -> Result<R> {
        let api = self.client()?;
        let key = format!(
            "{method} {path} {}",
            body.map(Value::to_string).unwrap_or_default()
        );
        if let Some(kept) = self.kept.borrow().get(&key) {
            kept.read_by(method, path, refusal_codes)?;
            return Ok(read(kept));
        }
        let answer = match method {
            Method::Get => api.get_json(path),
            Method::Post => api.post_json(path, body.unwrap_or(&Value::Null)),
        };
        let kept = match answer {
            Ok(value) => Kept::Value(value),
            Err(strata::Error::Api {
                method,
                path,
                status,
                body: text,
                retry_after,
            }) => {
                // the body is parsed one time, for the refusal and for the answer
                let parsed = serde_json::from_str::<Value>(&text).ok();
                // a 4xx is what a validate route answers when the entity is wrong: it is the
                // result. A route also refuses with a 5xx and a code. The validate routes are
                // known, and a check names the code of any other route in `refusal_codes`.
                if !reads_error(method, &path, status, parsed.as_ref(), refusal_codes) {
                    return Err(strata::Error::Api {
                        method,
                        path,
                        status,
                        body: text,
                        retry_after,
                    }
                    .into());
                }
                let body = parsed
                    .filter(Value::is_object)
                    .unwrap_or_else(|| serde_json::json!({ "message": text }));
                Kept::Answer { status, text, body }
            }
            Err(e) => return Err(e.into()),
        };
        let out = read(&kept);
        self.kept.borrow_mut().insert(key, kept);
        Ok(out)
    }

    /// The Palette client, or the error that says a key is missing.
    fn client(&self) -> Result<&'a A> {
        self.palette.ok_or_else(|| Error::Usage {
            message: format!(
                "a Palette check needs a key: set {}, or run with --tier static",
                KEY_VARS[0]
            ),
        })
    }
}

impl<A: PaletteApi> Backend for LiveBackend<'_, A> {
    fn palette(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        refusal_codes: &[String],
    ) -> Result<Response> {
        self.ask(method, path, body, refusal_codes, Kept::response)
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

impl<A: PaletteApi> PaletteApi for LiveBackend<'_, A> {
    fn get_json(&self, path: &str) -> strata::Result<Value> {
        self.send(Method::Get, path, None)
    }

    fn post_json(&self, path: &str, body: &Value) -> strata::Result<Value> {
        self.send(Method::Post, path, Some(body))
    }

    fn project_uid(&self) -> Option<&str> {
        self.palette.and_then(PaletteApi::project_uid)
    }
}

impl<A: PaletteApi> LiveBackend<'_, A> {
    /// A request of the world, in the terms of the library: an answer that is an HTTP error is
    /// the error that Palette gave.
    fn send(&self, method: Method, path: &str, body: Option<&Value>) -> strata::Result<Value> {
        let read = |kept: &Kept| match kept {
            Kept::Value(value) => Ok(value.clone()),
            Kept::Answer { status, text, .. } => Err(http_error(method, path, *status, text)),
        };
        match self.ask(method, path, body, &[], read) {
            Ok(answer) => answer,
            Err(Error::Palette(e)) => Err(e),
            Err(other) => Err(strata::Error::Usage {
                message: other.to_string(),
            }),
        }
    }
}

/// The backend of a fixture: the world document holds the answers.
///
/// `_responses` maps `"METHOD /path"` to a [`Response`], and `_exec` maps the command, as one
/// string with a space between the arguments, to an [`ExecOutput`]. `_tools` lists the tools that
/// are "there"; with no list, every tool is.
///
/// An entry of `_responses` with a status that is not 2xx is a [`Response`] only when
/// [`LiveBackend`] reads that status as an answer. Else it is the error that the live
/// backend gives, so a fixture cannot show a catch that a live run does not give.
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

    /// The output of each command that the fixture holds under `_exec`, by the command as one text
    /// with a space between the arguments. [`Backend::exec`] answers from the same map, so a
    /// caller that reads the answers of a fixture reads what a rule test runs.
    pub fn outputs(&self) -> &BTreeMap<String, ExecOutput> {
        &self.exec
    }
}

impl Backend for FixtureBackend {
    fn palette(
        &self,
        method: Method,
        path: &str,
        _body: Option<&Value>,
        refusal_codes: &[String],
    ) -> Result<Response> {
        let key = format!("{method} {path}");
        let response = self
            .responses
            .get(&key)
            .cloned()
            .ok_or_else(|| Error::Input {
                message: format!("the fixture has no answer for {key} under _responses"),
            })?;
        // An HTTP error is an answer on the live backend only when `reads_error` says so. That is
        // a 4xx that is no key or rate error, a refusal of a validate route, or a 5xx that the
        // check names. The fixture keeps the same rule for each status that is not 2xx. A fixture
        // cannot show a catch that a live run does not give. A 401, a 403, and a 429 are errors.
        let status = response.status;
        if !(200..300).contains(&status)
            && !reads_error(method, path, status, Some(&response.body), refusal_codes)
        {
            return Err(http_error(method, path, status, &response.body.to_string()).into());
        }
        Ok(response)
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
