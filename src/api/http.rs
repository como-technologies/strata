//! [`PaletteApi`] over HTTP: one attempt per call.

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

use serde_json::Value;

use super::mask::mask_secret;
use super::{Method, PaletteApi};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::text::LINE_ENDS;

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

/// The keys that an answer with a 2xx status proved in this process: see [`KeyProof`].
///
/// The proof belongs to the key, and not to a client or a thread. A run of strata makes more than
/// one client of a key. Two examples are the client of the lookup of a project and the client of
/// the project. The clients of one key can also run on many threads, such as the reads of an adapter,
/// so the set is behind a lock.
static PROVED_KEYS: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// The proof of one key at one address of Palette: true after an answer with a 2xx status to a
/// client of the key. Then the key is right, and a 403 that is a
/// [`Denial`](super::codes::Denial) is a refusal of the role of the key.
///
/// It holds [`Config::key_digest`], so [`PROVED_KEYS`] holds no key either. A client with another
/// key starts with no proof, also at the same address.
struct KeyProof(String);

impl KeyProof {
    /// The proof of the key of `config`.
    fn of(config: &Config) -> Self {
        KeyProof(config.key_digest())
    }

    /// Note an answer with a 2xx status: Palette took the key.
    fn prove(&self) {
        // a panic of another thread with the lock leaves the set as it was, so the set stays true
        let mut proved = PROVED_KEYS.lock().unwrap_or_else(PoisonError::into_inner);
        if !proved.contains(&self.0) {
            proved.insert(self.0.clone());
        }
    }

    /// True when an answer to a client of the key had a 2xx status.
    fn proved(&self) -> bool {
        PROVED_KEYS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&self.0)
    }
}

/// The HTTP implementation. One attempt per call; wrap it in [`Retry`](super::Retry) for more.
pub struct HttpApi {
    config: Config,
    agent: ureq::Agent,
    /// The proof of the key of `config`: see [`Error::Api`].
    proof: KeyProof,
}

impl HttpApi {
    /// Build an HTTP client for `config`.
    pub fn new(config: Config) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(config.timeout))
            .build()
            .into();
        HttpApi {
            proof: KeyProof::of(&config),
            config,
            agent,
        }
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
    /// stands for: a status out of 2xx, or a body that strata cannot read. The body of the error
    /// holds no copy of the key of the request. The error says whether an earlier answer to a client
    /// of the key proved the key.
    fn body(
        &self,
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
            // needs the JSON of the refusal. The message of the error gives only the code and
            // the message of Palette from it. The field `body` of `--json` gives all of it
            return Err(Error::Api {
                method,
                path: path.to_string(),
                status,
                body: mask_secret(&String::from_utf8_lossy(&body), &self.config.api_key)
                    .replace(LINE_ENDS, " "),
                retry_after,
                key_proved: self.proof.proved(),
            });
        }
        self.proof.prove();
        Ok((status, body))
    }

    /// The JSON of a response, or the error it stands for: the errors of [`HttpApi::body`], or a
    /// body that is not JSON.
    fn read(
        &self,
        method: Method,
        path: &str,
        answer: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<Value> {
        let (status, body) = self.body(method, path, answer)?;
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
        self.read(
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
        self.read(Method::Post, path, req.send(body.to_string()))
    }

    fn get_file(&self, path: &str) -> Result<Vec<u8>> {
        let url = format!("{}{}", self.config.base_url, path);
        let answer = self.headers(self.agent.get(url.as_str()), FILE_TYPE).call();
        self.body(Method::Get, path, answer).map(|(_, body)| body)
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
// The crate loads the file here alone, and the other unit tests read it as `api::routes`.
// Each unit test uses a few items, so the other items are dead code.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../tests/common/palette.rs"]
pub(crate) mod routes;

// The small HTTP server of the tests of strata, in the file that they use. The crate loads the file
// here alone, and the other unit tests read it as `api::server`. The unit tests use some of its
// items, and the other items are dead code.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../tests/common/server.rs"]
pub(crate) mod server;

#[cfg(test)]
mod tests {
    use super::routes::DOWNLOAD_LIMIT_REFUSAL;
    use super::server::{HttpAnswer, serve_each};
    use super::*;
    use crate::api::download_path;
    use crate::closed_set::ClosedSet;
    use crate::config::KEY_VARS;

    /// The address of a server on this machine that gives one answer with `status` and the JSON
    /// text `body`. So the read of a body needs no network.
    fn serve_once(status: u16, body: &str) -> String {
        serve_each(vec![HttpAnswer::json(status, body)]).0
    }

    /// The server of the tests gets the head of a request in one write and its body in a second
    /// write. The name of the header `ProjectUid` is in small letters.
    #[test]
    fn the_server_of_the_tests_reads_a_body_after_the_head_and_the_name_of_a_header_with_no_case() {
        use std::io::{Read, Write};
        let (url, requests) = serve_each(vec![HttpAnswer::json(201, "{}")]);
        let mut stream = std::net::TcpStream::connect(url.trim_start_matches("http://")).unwrap();
        stream
            .write_all(b"POST /v1/x?y=1 HTTP/1.1\r\nprojectuid: p1\r\nContent-Length: 7\r\n\r\n")
            .unwrap();
        stream.flush().unwrap();
        // a gap between the two writes, so that the body is not in the first read of the server
        std::thread::sleep(std::time::Duration::from_millis(50));
        stream.write_all(br#"{"a":1}"#).unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        assert!(answer.starts_with("HTTP/1.1 201 "), "{answer}");
        assert!(answer.ends_with("\r\n\r\n{}"), "{answer}");
        let request = requests.lock().unwrap()[0].clone();
        assert_eq!(
            (request.method.as_str(), request.target.as_str()),
            (Method::Post.as_str(), "/v1/x?y=1")
        );
        assert_eq!(request.project(), Some("p1"), "{request:?}");
        assert_eq!(request.json(), serde_json::json!({"a": 1}));
    }

    #[test]
    fn the_proof_of_a_key_belongs_to_the_key_so_a_second_client_with_another_key_names_the_key() {
        let forbidden = HttpAnswer::json(403, routes::filters_denied());
        let (url, _) = serve_each(vec![
            HttpAnswer::json(200, "{}"),
            forbidden.clone(),
            forbidden.clone(),
            forbidden,
        ]);
        // the first client: an answer with a 2xx status proves its key, so its denial is the role
        let first_key = "first-key";
        let first = HttpApi::new(Config::new(url.as_str(), first_key));
        first.get_json(routes::PROJECT_LISTING).unwrap();
        // then a new client of the same key, such as the client of a project after its lookup.
        // Last, a client with another key, on the same thread and at the same address.
        for (api, names_key) in [
            (first, false),
            (HttpApi::new(Config::new(url.as_str(), first_key)), false),
            (HttpApi::new(Config::new(url.as_str(), "second-key")), true),
        ] {
            let text = api
                .get_json(routes::TAG_FILTER_LISTING)
                .unwrap_err()
                .to_string();
            let named = KEY_VARS.iter().all(|var| text.contains(var));
            assert_eq!(named, names_key, "{text}");
        }
    }

    #[test]
    fn a_file_is_the_bytes_of_the_body_of_a_request_that_accepts_a_file() {
        let file = [0x1f, 0x8b, 0x08, 0x00, 0xff, 0x00];
        let (url, requests) = serve_each(vec![HttpAnswer {
            status: 200,
            kind: FILE_TYPE,
            body: file.to_vec(),
        }]);
        let api = HttpApi::new(Config::new(url, "k"));
        assert_eq!(api.get_file(&download_path("u1")).unwrap(), file);
        let request = requests.lock().unwrap()[0].clone();
        assert_eq!(request.header("Accept"), Some(FILE_TYPE), "{request:?}");
        assert!(
            request.headers.iter().all(|(_, v)| !v.contains(JSON_TYPE)),
            "{request:?}"
        );
    }

    #[test]
    fn a_file_that_palette_refuses_carries_its_status_and_body() {
        let (url, _) = serve_each(vec![HttpAnswer::json(429, DOWNLOAD_LIMIT_REFUSAL)]);
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
        let url = serve_once(204, "");
        let api = HttpApi::new(Config::new(url, "k"));
        let v = api
            .post_json("/v1/clouds/aws/account/validate", &serde_json::json!({}))
            .unwrap();
        assert_eq!(v, Value::Null);
    }

    #[test]
    fn a_200_with_no_body_is_not_an_answer() {
        let url = serve_once(200, "");
        let api = HttpApi::new(Config::new(url, "k"));
        let err = api.get_json("/v1/spectroclusters").unwrap_err().to_string();
        assert!(err.contains("/v1/spectroclusters"), "{err}");
    }

    #[test]
    fn a_4xx_carries_its_status_and_body() {
        let url = serve_once(
            400,
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
    fn the_error_of_an_answer_that_quotes_the_key_holds_no_copy_of_the_key() {
        // a body with three whole copies of a long key
        let key = format!("strata-{}", "x".repeat(400));
        let body = format!(
            r#"{{"code":"ResourceNotFound","message":"API Key '{key}' is not found","details":"{key} {key}"}}"#
        );
        let url = serve_once(401, &body);
        let api = HttpApi::new(Config::new(url, key.as_str()));
        let err = api.get_json(routes::PROJECT_LISTING).unwrap_err();
        match &err {
            Error::Api { status, body, .. } => {
                assert_eq!(*status, 401);
                assert_eq!(
                    body.matches(super::super::mask::MASKED).count(),
                    3,
                    "{body}"
                );
            }
            other => panic!("{other:?}"),
        }
        for printed in [err.to_string(), err.to_json().to_string(), err.reason()] {
            assert!(!printed.contains(&key), "{printed}");
            assert!(
                !printed.contains("xxxxxxxx"),
                "no part of the key: {printed}"
            );
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
