//! [`PaletteApi`] over canned responses, for tests.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::Value;

use super::PaletteApi;
use crate::error::{Error, Result};

/// Canned responses keyed by path. Paths not registered answer 404. A POST answers from the key
/// `POST <path> <body>` when [`MemoryApi::with_post`] registered that body, else from the key
/// `POST <path>`, and the body it was sent is kept for [`MemoryApi::posted`].
#[derive(Debug, Default, Clone)]
pub struct MemoryApi {
    ok: HashMap<String, Value>,
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
}

impl PaletteApi for MemoryApi {
    fn get_json(&self, path: &str) -> Result<Value> {
        *self.calls.borrow_mut().entry(path.to_string()).or_insert(0) += 1;
        if let Some(t) = self.transient.borrow_mut().get_mut(path)
            && t.remaining > 0
        {
            t.remaining -= 1;
            return Err(Error::Api {
                method: "GET".into(),
                path: path.to_string(),
                status: t.status,
                body: t.body.clone(),
                retry_after: t.retry_after,
            });
        }
        if let Some((status, body)) = self.fail.get(path) {
            return Err(Error::Api {
                method: "GET".into(),
                path: path.to_string(),
                status: *status,
                body: body.clone(),
                retry_after: None,
            });
        }
        self.ok.get(path).cloned().ok_or_else(|| Error::Api {
            method: "GET".into(),
            path: path.to_string(),
            status: 404,
            body: r#"{"code":"ResourceNotFound"}"#.to_string(),
            retry_after: None,
        })
    }

    fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        self.posted
            .borrow_mut()
            .push((path.to_string(), body.clone()));
        let keyed = format!("POST {path} {body}");
        if self.ok.contains_key(&keyed) {
            return self.get_json(&keyed);
        }
        self.get_json(&format!("POST {path}"))
    }

    fn project_uid(&self) -> Option<&str> {
        self.project_uid.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_errors_carry_path_status_and_body() {
        let api = MemoryApi::new().failing("/v1/x", 401, r#"{"code":"MissingAuthHeader"}"#);
        let err = api.get_json("/v1/x").unwrap_err();
        let text = err.to_string();
        assert!(
            text.starts_with(r#"GET /v1/x failed with HTTP 401: {"code":"MissingAuthHeader"}"#),
            "{text}"
        );
        // a 401 is almost always the key, and the body names nothing the reader can set
        assert!(text.contains("check PALETTE_API_KEY"), "{text}");
        assert_eq!(err.exit_code(), 2);
    }
}
