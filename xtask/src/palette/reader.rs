//! The answer of a route of a file to a request that accepts JSON.
//!
//! The export route of a profile, `GET /v1/clusterprofiles/{uid}/export`, and its download route,
//! `GET /v1/clusterprofiles/{uid}/spc/download`, send the file only to a request that accepts a
//! file, such as `Accept: application/octet-stream`. To `Accept: application/json` the answer is
//! HTTP 200 with `{"Reader":{}}`, as a live run of the export shows (#156). Palette builds the two
//! routes in the same way. The server of [`super`] answers such a request before it reads a file.

use std::path::Path;

use super::{Request, file_for};
use crate::tenant::{downloaded_profile, exported_profile};

/// The answer of a route of a file to a request that accepts JSON: `{"Reader":{}}`, and not the
/// file.
const FILE_FOR_JSON: &[u8] = br#"{"Reader":{}}"#;

/// True when `request` has the header `Accept: application/json`.
pub(super) fn accepts_json(request: &Request) -> bool {
    request
        .accept
        .as_deref()
        .is_some_and(|accept| accept.trim().eq_ignore_ascii_case("application/json"))
}

/// The body of the answer to a request for the export or the download of a profile that exists,
/// when the request accepts JSON: [`FILE_FOR_JSON`], for each query. `None` when `request` is
/// another request, or when it accepts a file: the file answers it.
pub(super) fn file_reader(root: &Path, request: &Request) -> Option<Vec<u8>> {
    if !accepts_json(request) {
        return None;
    }
    let path = request
        .target
        .split_once('?')
        .map_or(request.target.as_str(), |(path, _)| path)
        .trim_start_matches('/');
    let profile = exported_profile(path).or_else(|| downloaded_profile(path))?;
    file_for(root, request.project.as_deref(), profile)?
        .is_file()
        .then(|| FILE_FOR_JSON.to_vec())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::palette::served;

    /// The export route gives the file to a request that accepts a file. It gives `{"Reader":{}}`
    /// to a request that accepts JSON, as a live run shows (#156). The answer for JSON needs the
    /// profile, and not the file, and it does not depend on the query.
    #[test]
    fn the_export_route_answers_a_request_that_accepts_json_with_a_reader_and_not_the_file() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        let profiles = dir.join("p1/v1/clusterprofiles");
        fs::create_dir_all(profiles.join("u1")).unwrap();
        fs::write(profiles.join("u1.json"), "{}").unwrap();
        fs::write(
            profiles.join("u1/export@format=json.json"),
            r#"{"spec":{}}"#,
        )
        .unwrap();
        let answer = |target: &str, accept: Option<&str>| {
            let request = Request {
                target: target.to_string(),
                project: Some("p1".to_string()),
                accept: accept.map(str::to_string),
                body: Vec::new(),
            };
            let (status, body) = served(dir, &request).unwrap();
            (status, String::from_utf8(body).unwrap())
        };
        let file = (200, r#"{"spec":{}}"#.to_string());
        let reader = (200, r#"{"Reader":{}}"#.to_string());
        let route = "/v1/clusterprofiles/u1/export?format=json";
        assert_eq!(answer(route, Some("application/json")), reader);
        assert_eq!(answer(route, Some(" Application/JSON ")), reader);
        assert_eq!(
            answer(
                "/v1/clusterprofiles/u1/export?format=yaml",
                Some("application/json")
            ),
            reader
        );
        assert_eq!(answer(route, Some("application/octet-stream")), file);
        assert_eq!(answer(route, None), file);
        // another route under the same profile gives its own file, and a profile with no
        // document has no export
        assert_eq!(
            answer("/v1/clusterprofiles/u1", Some("application/json")).1,
            "{}"
        );
        let missing = answer(
            "/v1/clusterprofiles/u2/export?format=json",
            Some("application/json"),
        );
        assert_eq!(missing.0, 404);
    }
}
