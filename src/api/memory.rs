//! [`PaletteApi`] over canned responses, for tests.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::Value;

use super::{Method, PaletteApi};
use crate::error::{Error, Result};

/// Canned responses keyed by path. A path that nobody registered answers 404.
///
/// A GET of a file answers from the files that [`MemoryApi::with_file`] registered, and fails as a
/// GET of the same path does.
///
/// A POST answers from the key `POST <path> <body>` when [`MemoryApi::with_post`] registered that
/// body, else from the key `POST <path>`. The body that it was sent is kept for
/// [`MemoryApi::posted`].
#[derive(Debug, Default, Clone)]
pub struct MemoryApi {
    ok: HashMap<String, Value>,
    files: HashMap<String, Vec<u8>>,
    fail: HashMap<String, (u16, String)>,
    transient: RefCell<HashMap<String, Transient>>,
    calls: RefCell<HashMap<String, u32>>,
    posted: RefCell<Vec<(String, Value)>>,
    project_uid: Option<String>,
}

/// A failure that goes away after `remaining` calls.
#[derive(Debug, Clone)]
struct Transient {
    remaining: u32,
    status: u16,
    body: String,
    retry_after: Option<u64>,
}

impl MemoryApi {
    /// An API that knows no paths.
    pub fn new() -> Self {
        Self::default()
    }

    /// Each POST it was sent, in order: the path and the body.
    pub fn posted(&self) -> Vec<(String, Value)> {
        self.posted.borrow().clone()
    }

    /// Answer `path` with `value`.
    pub fn with(mut self, path: impl Into<String>, value: Value) -> Self {
        self.ok.insert(path.into(), value);
        self
    }

    /// Answer a GET of the file at `path` with `bytes`.
    pub fn with_file(mut self, path: impl Into<String>, bytes: Vec<u8>) -> Self {
        self.files.insert(path.into(), bytes);
        self
    }

    /// Answer a POST of `body` to `path` with `value`: two searches to one path can then get two
    /// answers.
    pub fn with_post(mut self, path: &str, body: &Value, value: Value) -> Self {
        self.ok.insert(format!("POST {path} {body}"), value);
        self
    }

    /// Answer `path` with an HTTP error.
    pub fn failing(
        mut self,
        path: impl Into<String>,
        status: u16,
        body: impl Into<String>,
    ) -> Self {
        self.fail.insert(path.into(), (status, body.into()));
        self
    }

    /// Answer `path` with an HTTP error `n` times, then with whatever else is registered.
    pub fn failing_n_times(
        self,
        path: impl Into<String>,
        n: u32,
        status: u16,
        body: impl Into<String>,
    ) -> Self {
        self.transient.borrow_mut().insert(
            path.into(),
            Transient {
                remaining: n,
                status,
                body: body.into(),
                retry_after: None,
            },
        );
        self
    }

    /// Add a `Retry-After` of `seconds` to the transient failures registered for `path`.
    pub fn with_retry_after(self, path: impl Into<String>, seconds: u64) -> Self {
        if let Some(t) = self.transient.borrow_mut().get_mut(&path.into()) {
            t.retry_after = Some(seconds);
        }
        self
    }

    /// How many times `path` was requested.
    pub fn calls(&self, path: &str) -> u32 {
        self.calls.borrow().get(path).copied().unwrap_or(0)
    }

    /// Report `uid` as the project scope, as the HTTP client does for `PALETTE_PROJECT_UID`.
    pub fn in_project(mut self, uid: impl Into<String>) -> Self {
        self.project_uid = Some(uid.into());
        self
    }

    /// The answer to a request with `method` to `path`, from the canned responses of `key` in
    /// `canned`: the documents, or the files.
    ///
    /// A GET has its path as its key, and a POST has a key of its own: see [`MemoryApi`].
    fn answer<T: Clone>(
        &self,
        method: Method,
        path: &str,
        key: &str,
        canned: &HashMap<String, T>,
    ) -> Result<T> {
        *self.calls.borrow_mut().entry(key.to_string()).or_insert(0) += 1;
        let error = |status: u16, body: String, retry_after: Option<u64>| Error::Api {
            method,
            path: path.to_string(),
            status,
            body,
            retry_after,
        };
        if let Some(t) = self.transient.borrow_mut().get_mut(key)
            && t.remaining > 0
        {
            t.remaining -= 1;
            return Err(error(t.status, t.body.clone(), t.retry_after));
        }
        if let Some((status, body)) = self.fail.get(key) {
            return Err(error(*status, body.clone(), None));
        }
        let answer = canned
            .get(key)
            .cloned()
            .ok_or_else(|| error(404, r#"{"code":"ResourceNotFound"}"#.to_string(), None))?;
        super::prove_key();
        Ok(answer)
    }
}

impl PaletteApi for MemoryApi {
    fn get_json(&self, path: &str) -> Result<Value> {
        self.answer(Method::Get, path, path, &self.ok)
    }

    fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        self.posted
            .borrow_mut()
            .push((path.to_string(), body.clone()));
        let keyed = format!("POST {path} {body}");
        if self.ok.contains_key(&keyed) {
            return self.answer(Method::Post, path, &keyed, &self.ok);
        }
        self.answer(Method::Post, path, &format!("POST {path}"), &self.ok)
    }

    fn get_file(&self, path: &str) -> Result<Vec<u8>> {
        self.answer(Method::Get, path, path, &self.files)
    }

    fn project_uid(&self) -> Option<&str> {
        self.project_uid.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_errors_carry_the_method_path_status_and_body() {
        let api = MemoryApi::new()
            .failing("/v1/x", 401, r#"{"code":"MissingAuthHeader"}"#)
            .failing("POST /v1/y", 403, "forbidden");
        let err = api.get_json("/v1/x").unwrap_err();
        let text = err.to_string();
        // a body with a code and no message gives the code
        assert!(
            text.starts_with("GET /v1/x failed with HTTP 401: MissingAuthHeader; "),
            "{text}"
        );
        // a 401 is almost always the key, and the body names nothing the reader can set
        assert!(text.contains("check PALETTE_API_KEY"), "{text}");
        assert_eq!(err.exit_code(), 2);
        // a POST names its method and its path, as the HTTP client does, and not its key
        let err = api.post_json("/v1/y", &Value::Null).unwrap_err();
        assert!(
            err.to_string()
                .starts_with("POST /v1/y failed with HTTP 403: forbidden"),
            "{err}"
        );
        assert_eq!(err.to_json()["method"], "POST");
        assert_eq!(api.calls("POST /v1/y"), 1);
    }
}
