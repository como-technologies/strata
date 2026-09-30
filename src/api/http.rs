//! [`PaletteApi`] over HTTP: one attempt per call.

use serde_json::Value;

use super::PaletteApi;
use crate::config::Config;
use crate::error::{Error, Result};

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
    /// The headers every request carries: the key, JSON, and the project when there is one.
    fn headers<B>(&self, req: ureq::RequestBuilder<B>) -> ureq::RequestBuilder<B> {
        let req = req
            .header("ApiKey", self.config.api_key.as_str())
            .header("Accept", "application/json");
        match &self.config.project_uid {
            Some(p) => req.header("ProjectUid", p.as_str()),
            None => req,
        }
    }

    /// The JSON of a response, or the error it stands for: a status out of 2xx, a body that could
    /// not be read, or a body that is not JSON.
    fn read(
        method: &str,
        path: &str,
        answer: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<Value> {
        let mut resp = answer.map_err(|e| Error::Transport {
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
            .limit(256 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| Error::Transport {
                path: path.to_string(),
                message: format!("reading the response body: {e}"),
            })?;
        if !(200..300).contains(&status) {
            // the whole body, on one line: a caller that reads the answer of a validate route
            // needs the JSON of the refusal, and the message of the error shows the first part
            return Err(Error::Api {
                method: method.to_string(),
                path: path.to_string(),
                status,
                body: body.replace(['\n', '\r'], " "),
                retry_after,
            });
        }
        // a 204 No Content, which the validate routes of the clouds answer, is an answer with no
        // body: null, and not a body that is not JSON. A 200 with no body stays the error it
        // was: a proxy or an outage page, not an answer of Palette
        if status == 204 {
            return Ok(Value::Null);
        }
        serde_json::from_str(&body).map_err(|source| Error::NotJson {
            path: path.to_string(),
            source,
        })
    }
}

impl PaletteApi for HttpApi {
    fn get_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.config.base_url, path);
        Self::read(
            "GET",
            path,
            self.headers(self.agent.get(url.as_str())).call(),
        )
    }

    fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}{}", self.config.base_url, path);
        let req = self
            .headers(self.agent.post(url.as_str()))
            .header("Content-Type", "application/json");
        Self::read("POST", path, req.send(body.to_string()))
    }

    fn project_uid(&self) -> Option<&str> {
        self.config.project_uid.as_deref()
    }
}

/// `Retry-After` as whole seconds. The HTTP-date form is not supported and reads as absent.
fn retry_after_seconds(header: Option<&str>) -> Option<u64> {
    header.and_then(|h| h.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One answer from a server on this machine, so the reading of a body needs no network.
    fn serve_once(status_line: &str, body: &str) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let answer = format!(
            "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            s.write_all(answer.as_bytes()).unwrap();
        });
        format!("http://{addr}")
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
