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
use strata::json::opt_flag;

use super::exec::{run_command, tool_on_path};
use crate::error::{Error, Result};

/// The answer of Palette to one request.
///
/// A 2xx is `ok`, a 4xx is an answer with its `V1Error` body, and a 204 is `ok` with a `null`
/// body. A refusal of the cluster validate route with HTTP 500 is an answer too.
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
    /// One request to Palette.
    ///
    /// # Errors
    ///
    /// [`Error::Palette`] for a failure that no check can read: no key, a transport error, or a
    /// 5xx after the retries. A 4xx is a [`Response`].
    fn palette(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Response>;
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

/// A 4xx that is the answer of a check: the route read the request and refused what it says. A
/// 401 or a 403 is the key, and a 429 is the rate: the check did not run, and the error names it.
pub fn is_answer(status: u16) -> bool {
    (400..500).contains(&status) && !matches!(status, 401 | 403 | 429)
}

/// What the backend keeps of the answer to one request. An answer is the same for the rest of
/// the run, because a run changes nothing in Palette.
#[derive(Debug, Clone)]
enum Kept {
    /// A 2xx: the body, as JSON. A 204 has a `null` body.
    Value(Value),
    /// An HTTP error that is an answer. It is a 4xx that a check reads. It is also the refusal of
    /// a validate route, such as a profile of another cloud type with HTTP 500.
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

    /// The answer to a request: the one that the run kept when it sent the same request, else
    /// the one that Palette gives now.
    fn ask(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Kept> {
        let api = self.client()?;
        let key = format!(
            "{method} {path} {}",
            body.map(Value::to_string).unwrap_or_default()
        );
        if let Some(kept) = self.kept.borrow().get(&key) {
            return Ok(kept.clone());
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
                // result. The route also refuses with a 500 and a code that it names.
                let refusal = parsed
                    .as_ref()
                    .is_some_and(|b| ValidateRoute::refuses_request(method, &path, status, b));
                if !is_answer(status) && !refusal {
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
        self.kept.borrow_mut().insert(key, kept.clone());
        Ok(kept)
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
    fn palette(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Response> {
        Ok(self.ask(method, path, body)?.response())
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
        let kept = match self.ask(method, path, body) {
            Ok(kept) => kept,
            Err(Error::Palette(e)) => return Err(e),
            Err(other) => {
                return Err(strata::Error::Usage {
                    message: other.to_string(),
                });
            }
        };
        match kept {
            Kept::Value(value) => Ok(value),
            Kept::Answer { status, text, .. } => Err(strata::Error::Api {
                method,
                path: path.to_string(),
                status,
                body: text,
                retry_after: None,
            }),
        }
    }
}

/// The backend of a fixture: the world document holds the answers.
///
/// `_responses` maps `"METHOD /path"` to a [`Response`], and `_exec` maps the command, as one
/// string with a space between the arguments, to an [`ExecOutput`]. `_tools` lists the tools that
/// are "there"; with no list, every tool is.
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
