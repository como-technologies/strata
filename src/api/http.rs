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
            return Err(Error::Api {
                path: path.to_string(),
                status,
                body: body
                    .chars()
                    .take(300)
                    .collect::<String>()
                    .replace('\n', " "),
                retry_after,
            });
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
        Self::read(path, self.headers(self.agent.get(url.as_str())).call())
    }

    fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}{}", self.config.base_url, path);
        let req = self
            .headers(self.agent.post(url.as_str()))
            .header("Content-Type", "application/json");
        Self::read(path, req.send(body.to_string()))
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
