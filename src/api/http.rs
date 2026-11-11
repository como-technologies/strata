//! [`PaletteApi`] over HTTP: one attempt per call.

use serde_json::Value;

use super::{Method, PaletteApi};
use crate::config::Config;
use crate::error::{Error, Result};

/// The header that carries the API key.
pub const API_KEY_HEADER: &str = "ApiKey";

/// The header that carries the uid of the project.
pub const PROJECT_UID_HEADER: &str = "ProjectUid";

/// The media type of a JSON document, for `Accept` and for the body of a `POST`.
const JSON_TYPE: &str = "application/json";

/// The media type that a request for a file accepts. Palette sends a file, such as the download of
/// a profile version, only to a request that accepts a file.
const FILE_TYPE: &str = "application/octet-stream";

/// The most bytes that strata reads of one answer.
const BODY_LIMIT: u64 = 256 * 1024 * 1024;

/// The HTTP implementation. One attempt per call; wrap it in [`Retry`](super::Retry) for more.
pub struct HttpApi {
    config: Config,
    agent: ureq::Agent,
}

impl HttpApi {
    /// Build an HTTP client for `config`.
    pub fn new(config: Config) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(config.timeout))
            .build()
            .into();
        HttpApi { config, agent }
    }
}

impl HttpApi {
    /// The headers every request carries: the key, `accept`, and the project when there is one.
    fn headers<B>(&self, req: ureq::RequestBuilder<B>, accept: &str) -> ureq::RequestBuilder<B> {
        let req = req
            .header(API_KEY_HEADER, self.config.api_key.as_str())
            .header("Accept", accept);
        match &self.config.project_uid {
            Some(p) => req.header(PROJECT_UID_HEADER, p.as_str()),
            None => req,
        }
    }

    /// The status and the body of an answer with a 2xx status. Else the error that the answer
    /// stands for: a status out of 2xx, or a body that strata cannot read.
    fn body(
        method: Method,
        path: &str,
        answer: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<(u16, Vec<u8>)> {
        let mut resp = answer.map_err(|e| Error::Transport {
            method,
            path: path.to_string(),
            message: e.to_string(),
        })?;
        let status = resp.status().as_u16();
        let retry_after = retry_after_seconds(
            resp.headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok()),
        );
        let body = resp
            .body_mut()
            .with_config()
            .limit(BODY_LIMIT)
            .read_to_vec()
            .map_err(|e| Error::Transport {
                method,
                path: path.to_string(),
                message: format!("reading the response body: {e}"),
            })?;
        if !(200..300).contains(&status) {
            // the whole body, on one line: a caller that reads the answer of a validate route
            // needs the JSON of the refusal, and the message of the error shows the first part
            return Err(Error::Api {
                method,
                path: path.to_string(),
                status,
                body: String::from_utf8_lossy(&body).replace(['\n', '\r'], " "),
                retry_after,
            });
        }
        Ok((status, body))
    }

    /// The JSON of a response, or the error it stands for: the errors of [`HttpApi::body`], or a
    /// body that is not JSON.
    fn read(
        method: Method,
        path: &str,
        answer: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<Value> {
        let (status, body) = Self::body(method, path, answer)?;
        // a 204 No Content, which the validate routes of the clouds answer, is an answer with no
        // body: null, and not a body that is not JSON. A 200 with no body stays the error it
        // was: a proxy or an outage page, not an answer of Palette
        if status == 204 {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&body).map_err(|source| Error::NotJson {
            method,
            path: path.to_string(),
            source,
        })
    }
}

impl PaletteApi for HttpApi {
    fn get_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.config.base_url, path);
        Self::read(
            Method::Get,
            path,
            self.headers(self.agent.get(url.as_str()), JSON_TYPE).call(),
        )
    }

    fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}{}", self.config.base_url, path);
        let req = self
            .headers(self.agent.post(url.as_str()), JSON_TYPE)
            .header("Content-Type", JSON_TYPE);
        Self::read(Method::Post, path, req.send(body.to_string()))
    }

    fn get_file(&self, path: &str) -> Result<Vec<u8>> {
        let url = format!("{}{}", self.config.base_url, path);
        let answer = self.headers(self.agent.get(url.as_str()), FILE_TYPE).call();
        Self::body(Method::Get, path, answer).map(|(_, body)| body)
    }

    fn project_uid(&self) -> Option<&str> {
        self.config.project_uid.as_deref()
    }
}

/// `Retry-After` as whole seconds. The HTTP-date form is not supported and reads as absent.
fn retry_after_seconds(header: Option<&str>) -> Option<u64> {
    header.and_then(|h| h.trim().parse().ok())
}

// The routes and the answers of Palette that the tests of strata serve, in the file that they use.
// A unit test here uses the refusal of a download, so the other items are dead code here.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../tests/common/palette.rs"]
mod routes;

#[cfg(test)]
mod tests {
    use super::routes::DOWNLOAD_LIMIT_REFUSAL;
    use super::*;
    use crate::api::download_path;

    /// One answer from a server on this machine, so the reading of a body needs no network.
    fn serve_once(status_line: &str, body: &str) -> String {
        serve_bytes(status_line, body.as_bytes()).0
    }

    /// One answer with the bytes `body`, from a server on this machine. The receiver gives the
    /// head of the request that the server read.
    fn serve_bytes(status_line: &str, body: &[u8]) -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut answer = format!(
            "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        answer.extend_from_slice(body);
        let (sent, request) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let _ = sent.send(String::from_utf8_lossy(&buf[..n]).to_string());
            s.write_all(&answer).unwrap();
        });
        (format!("http://{addr}"), request)
    }

    #[test]
    fn a_file_is_the_bytes_of_the_body_of_a_request_that_accepts_a_file() {
        let file = [0x1f, 0x8b, 0x08, 0x00, 0xff, 0x00];
        let (url, request) = serve_bytes("200 OK", &file);
        let api = HttpApi::new(Config::new(url, "k"));
        assert_eq!(api.get_file(&download_path("u1")).unwrap(), file);
        let head = request.recv().unwrap().to_ascii_lowercase();
        assert!(head.contains("accept: application/octet-stream"), "{head}");
        assert!(!head.contains("application/json"), "{head}");
    }

    #[test]
    fn a_file_that_palette_refuses_carries_its_status_and_body() {
        let (url, _) = serve_bytes("429 Too Many Requests", DOWNLOAD_LIMIT_REFUSAL.as_bytes());
        let api = HttpApi::new(Config::new(url, "k"));
        match api.get_file(&download_path("u1")) {
            Err(Error::Api { status, body, .. }) => {
                assert_eq!(status, 429);
                assert_eq!(body, DOWNLOAD_LIMIT_REFUSAL);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_204_with_no_body_is_null_and_not_a_body_that_is_not_json() {
        let url = serve_once("204 No Content", "");
        let api = HttpApi::new(Config::new(url, "k"));
        let v = api
            .post_json("/v1/clouds/aws/account/validate", &serde_json::json!({}))
            .unwrap();
        assert_eq!(v, Value::Null);
    }

    #[test]
    fn a_200_with_no_body_is_not_an_answer() {
        let url = serve_once("200 OK", "");
        let api = HttpApi::new(Config::new(url, "k"));
        let err = api.get_json("/v1/spectroclusters").unwrap_err().to_string();
        assert!(err.contains("/v1/spectroclusters"), "{err}");
    }

    #[test]
    fn a_4xx_carries_its_status_and_body() {
        let url = serve_once(
            "400 Bad Request",
            r#"{"code":"InvalidParam","message":"Node pool size should be multiple of 3"}"#,
        );
        let api = HttpApi::new(Config::new(url, "k"));
        let err = api
            .post_json("/v1/spectroclusters/gke/validate", &serde_json::json!({}))
            .unwrap_err();
        match err {
            Error::Api { status, body, .. } => {
                assert_eq!(status, 400);
                assert!(body.contains("InvalidParam"), "{body}");
            }
            other => panic!("{other}"),
        }
    }

    #[test]
    fn retry_after_header_parses_whole_seconds_only() {
        assert_eq!(retry_after_seconds(Some("2")), Some(2));
        assert_eq!(retry_after_seconds(Some(" 10 ")), Some(10));
        assert_eq!(
            retry_after_seconds(Some("Wed, 21 Oct 2015 07:28:00 GMT")),
            None
        );
        assert_eq!(retry_after_seconds(None), None);
    }
}
