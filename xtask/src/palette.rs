//! A local stand-in for the Palette API. It serves JSON files from a directory.
//!
//! The book shows real strata output. The examples runner starts this server, points strata at it
//! with `PALETTE_API`, and runs each command of the book.
//!
//! A request reads one file:
//!
//! ```text
//! GET <path>            ProjectUid: <uid>   ->  <root>/<uid>/<path>.json
//! GET <path>            no ProjectUid       ->  <root>/tenant/<path>.json
//! GET <path>?<query>    ...                 ->  <root>/.../<path>@<query>.json
//! ```
//!
//! The query leaves out `startTime` and `endTime`: see `without_time`.
//!
//! When the file `<name>.json` is empty, the answer is HTTP 204 with no body, as Palette answers
//! some validate routes.
//!
//! When the file `<name>.<status>.json` exists in place of `<name>.json`, for a status of
//! [`REFUSALS`], the answer is that status with that file as the body: Palette refuses the
//! request.
//!
//! The validate route of a create, `POST /v1/spectroclusters/{cloudType}/validate`, reads the body
//! first. When a profile of the body is of another cloud type, the answer is HTTP 500 with the
//! code `ClusterProfilesNotSupported`, as Palette gives it: see `profile_cloud_refusal`.
//!
//! When the file does not exist, the answer is HTTP 404 with the body that Palette gives. The
//! server reads each file at each request, so the runner can change the tenant between two
//! commands, and a page can show what Palette does after an edit in place.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::{fs, thread};

use crate::tenant::{AUDITS, CLUSTER_SEARCH_PATH, PROFILE_SEARCH_PATH, TENANT};

/// Serve `root` on a free port of 127.0.0.1, write the port to `port_file`, and never return.
pub fn serve(root: &Path, port_file: &Path) -> std::io::Result<()> {
    // The runner holds the write end of this server's standard input. Whatever ends the runner --
    // a clean exit, a failed step, a panic, a kill -- closes it, and this server stops with it.
    // A server that outlives its runner keeps the tenant directory and the port, and holds open
    // whatever the runner's output was piped into.
    thread::spawn(|| {
        let mut sink = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut sink);
        std::process::exit(0);
    });
    let root = root.canonicalize()?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    fs::write(port_file, listener.local_addr()?.port().to_string())?;
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let root = root.clone();
        // One thread for each request, as the runner can ask for several at once. A panic in one
        // request must not stop the server, and a joinless thread ends with its request.
        thread::spawn(move || {
            let _ = answer(&root, stream);
        });
    }
    Ok(())
}

fn answer(root: &Path, mut stream: TcpStream) -> std::io::Result<()> {
    let Some(request) = read_request(&mut stream)? else {
        return Ok(());
    };
    let (status, body) = match profile_cloud_refusal(root, &request) {
        Some(body) => (500, body),
        None => served(root, &request)?,
    };
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        403 => "Forbidden",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        500 => "Internal Server Error",
        _ => "Not Found",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    stream.flush()
}

/// The status and the body that the files of the tenant give for `request`.
fn served(root: &Path, request: &Request) -> std::io::Result<(u16, Vec<u8>)> {
    let path = file_for(root, request.project.as_deref(), &request.target);
    let refusal = path.as_deref().and_then(refusal_of);
    Ok(match (path, refusal) {
        (Some(path), _) if path.is_file() && with_query(&request.target, CLUSTER_SEARCH_PATH) => {
            (200, searched(&fs::read(path)?, &request.body))
        }
        (Some(path), _) if path.is_file() && with_query(&request.target, PROFILE_SEARCH_PATH) => {
            (200, profiles_searched(&fs::read(path)?, &request.body))
        }
        (Some(path), _) if path.is_file() => match fs::read(path)? {
            body if body.is_empty() => (204, body),
            body => (200, body),
        },
        (Some(path), Some(status)) => (status, fs::read(refused(&path, status))?),
        // Palette answers an empty log for an object with no entry, and not 404
        (Some(_), None) if with_query(&request.target, AUDITS) => (
            200,
            br#"{"items":[],"listmeta":{"continue":"","limit":100}}"#.to_vec(),
        ),
        _ => (
            404,
            serde_json::json!({
                "code": "ResourceNotFound",
                "message": format!("no file for {}", request.target),
            })
            .to_string()
            .into_bytes(),
        ),
    })
}

/// The cloud type in `target`, when it is the validate route of a create:
/// `/v1/spectroclusters/{cloudType}/validate`.
fn cluster_validate_cloud(target: &str) -> Option<&str> {
    let cloud = target
        .strip_prefix("/v1/spectroclusters/")?
        .strip_suffix("/validate")?;
    (!cloud.contains('/')).then_some(cloud)
}

/// The refusal of the validate route of a create for a body with a profile of another cloud type,
/// as Palette gives it: HTTP 500, the code `ClusterProfilesNotSupported`, and the names of those
/// profiles. A profile of the cloud type `all` fits each cloud. `None` when `request` is not that
/// route, or when each profile of its body fits the cloud of the route.
///
/// A live run shows the message for one profile. The stand-in joins the names of several with a
/// comma, which no run shows.
fn profile_cloud_refusal(root: &Path, request: &Request) -> Option<Vec<u8>> {
    let cloud = cluster_validate_cloud(&request.target)?;
    let entity: serde_json::Value = serde_json::from_slice(&request.body).ok()?;
    let names: Vec<String> = entity["spec"]["profiles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let route = format!("/v1/clusterprofiles/{}", p["uid"].as_str()?);
            let file = file_for(root, request.project.as_deref(), &route)?;
            let profile: serde_json::Value = serde_json::from_slice(&fs::read(file).ok()?).ok()?;
            let kind = profile["spec"]["published"]["cloudType"].as_str()?;
            if kind == "all" || kind.eq_ignore_ascii_case(cloud) {
                return None;
            }
            profile["metadata"]["name"].as_str().map(str::to_string)
        })
        .collect();
    if names.is_empty() {
        return None;
    }
    let message = format!(
        "Cluster profiles '{}' are not supported with '{cloud}' environment",
        names.join(", ")
    );
    Some(
        serde_json::json!({"code": "ClusterProfilesNotSupported", "message": message})
            .to_string()
            .into_bytes(),
    )
}

/// True when `target` asks for the route `path` with a query, as a search or a log does.
///
/// The path alone, or a longer path under it, is another route.
fn with_query(target: &str, path: &str) -> bool {
    target
        .strip_prefix('/')
        .and_then(|t| t.strip_prefix(path))
        .is_some_and(|rest| rest.starts_with('?'))
}

/// The statuses of a refusal that a file beside the file of the answer gives:
///
/// - 403: the key cannot read the resource.
/// - 409: the resource exists.
/// - 422: the API schema refuses the body.
/// - 500: Palette gives the refusal the type of a server fault.
pub(crate) const REFUSALS: [u16; 4] = [403, 409, 422, 500];

/// The file whose presence makes the server refuse the request for `path` with HTTP 403.
pub fn forbidden(path: &Path) -> PathBuf {
    refused(path, 403)
}

/// The file whose presence makes the server refuse the request for `path` with `status`.
pub fn refused(path: &Path, status: u16) -> PathBuf {
    path.with_extension(format!("{status}.json"))
}

/// The status of the refusal of the request for `path`, when a file of [`REFUSALS`] is there.
fn refusal_of(path: &Path) -> Option<u16> {
    REFUSALS
        .into_iter()
        .find(|status| refused(path, *status).is_file())
}

struct Request {
    target: String,
    project: Option<String>,
    body: Vec<u8>,
}

/// The clusters of the search file that the body of a search selects, as Palette reads it: each
/// group of conditions holds. A condition on `tags` holds when a value `key:value` names a label
/// of the cluster, or a value `key` names a label key; on `clusterProfiles` when a value is the
/// name of a profile of the cluster, since Palette changes each name into the uids of the profiles
/// with that name and a uid matches no name (#181); on `environment` when a value is its cloud
/// type. The search
/// leaves out each virtual cluster, of the cloud type `nested`, unless a condition asks for it.
fn searched(file: &[u8], body: &[u8]) -> Vec<u8> {
    let Ok(mut doc) = serde_json::from_slice::<serde_json::Value>(file) else {
        return file.to_vec();
    };
    let query: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let groups = query["filter"]["filterGroups"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let holds = |cluster: &serde_json::Value, item: &serde_json::Value| {
        let c = &item["condition"]["string"];
        let labels = &cluster["metadata"]["labels"];
        let summary = &cluster["specSummary"];
        let hit = c["match"]["values"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| {
                let v = v.as_str().unwrap_or_default();
                match item["property"].as_str().unwrap_or_default() {
                    "clusterProfiles" => summary["clusterProfileTemplates"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|t| t["name"] == v),
                    "environment" => summary["cloudConfig"]["cloudType"] == v,
                    _ => match v.split_once(':') {
                        Some((k, value)) => labels[k] == value,
                        None => labels.get(v).is_some(),
                    },
                }
            });
        hit != c["negation"].as_bool().unwrap_or(false)
    };
    let asks_nested = groups.iter().any(|g| {
        g["filters"].as_array().into_iter().flatten().any(|f| {
            f["property"] == "environment"
                && f["condition"]["string"]["match"]["values"]
                    .as_array()
                    .is_some_and(|vs| vs.iter().any(|v| v == "nested"))
        })
    });
    if let Some(items) = doc["items"].as_array_mut() {
        items.retain(|cluster| {
            let nested = cluster["specSummary"]["cloudConfig"]["cloudType"] == "nested";
            (asks_nested || !nested)
                && groups.iter().all(|g| {
                    let filters = g["filters"].as_array().cloned().unwrap_or_default();
                    if g["conjunction"] == "or" {
                        filters.iter().any(|f| holds(cluster, f))
                    } else {
                        filters.iter().all(|f| holds(cluster, f))
                    }
                })
        });
    }
    doc.to_string().into_bytes()
}

/// The profiles of the search file that the body of a profile search selects, as Palette reads
/// it. With `filter.scope`, the items of that scope alone stay, and `listmeta.count` counts them.
/// The search gives one item for each profile name of each scope, with the uid of each version
/// in `specSummary.versions[]`.
fn profiles_searched(file: &[u8], body: &[u8]) -> Vec<u8> {
    let Ok(mut doc) = serde_json::from_slice::<serde_json::Value>(file) else {
        return file.to_vec();
    };
    let query: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    if let Some(scope) = query["filter"]["scope"].as_str()
        && let Some(items) = doc["items"].as_array_mut()
    {
        items.retain(|p| p["metadata"]["annotations"]["scope"] == scope);
        let count = items.len();
        doc["listmeta"]["count"] = serde_json::json!(count);
    }
    doc.to_string().into_bytes()
}

/// The request line, the headers this server reads, and the body: a search is a POST, and its
/// body selects the clusters. Every other header is skipped.
fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<Request>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let Some(target) = line.split_whitespace().nth(1) else {
        return Ok(None);
    };
    let mut request = Request {
        target: target.to_string(),
        project: None,
        body: Vec::new(),
    };
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("ProjectUid") {
                request.project = Some(value.trim().to_string());
            } else if name.eq_ignore_ascii_case("Content-Length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    if length > 0 {
        let mut body = vec![0; length];
        reader.read_exact(&mut body)?;
        request.body = body;
    }
    Ok(Some(request))
}

/// The file that answers `target` in `project`, or at tenant scope when there is no project.
/// A path that leaves the root answers nothing.
fn file_for(root: &Path, project: Option<&str>, target: &str) -> Option<PathBuf> {
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (target, None),
    };
    let path = percent_decode(path.trim_matches('/'));
    if path.is_empty() || path.split('/').any(|p| p == ".." || p.is_empty()) {
        return None;
    }
    let name = match query.map(without_time) {
        Some(q) if !q.is_empty() => format!("{path}@{}", percent_decode(&q)),
        _ => path,
    };
    let scope = project.filter(|p| !p.is_empty()).unwrap_or(TENANT);
    if scope.contains('/') || scope == ".." {
        return None;
    }
    Some(root.join(scope).join(name + ".json"))
}

/// The query without `startTime` and `endTime`. A command sends the real time, as `verify --who`
/// sends the time of its record, and a file name cannot hold it. The server gives the full audit
/// log of a resource, and the tenant logs a write only after a record.
fn without_time(query: &str) -> String {
    query
        .split('&')
        .filter(|p| !p.starts_with("startTime=") && !p.starts_with("endTime="))
        .collect::<Vec<_>>()
        .join("&")
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tenant::PROJECT_LISTING;

    /// The body of a search with one condition on `clusterProfiles`.
    fn profile_query(value: &str) -> Vec<u8> {
        serde_json::json!({"filter": {"conjunction": "and", "filterGroups": [
            {"conjunction": "and", "filters": [{"property": "clusterProfiles", "type": "string",
                "condition": {"string": {"operator": "eq", "negation": false, "ignoreCase": false,
                    "match": {"conjunction": "or", "values": [value]}}}}]}]}})
        .to_string()
        .into_bytes()
    }

    #[test]
    fn the_search_takes_the_name_of_a_profile_and_a_uid_selects_no_cluster() {
        // Palette changes the name into the uids of the profiles with that name, and a value that
        // is no name selects nothing (#181)
        let file = serde_json::json!({"items": [{
            "metadata": {"uid": "c1", "name": "fred"},
            "specSummary": {"cloudConfig": {"cloudType": "aws"},
                "clusterProfileTemplates": [{"uid": "p-uid", "name": "lab-profile", "type": "cluster"}]},
        }]})
        .to_string()
        .into_bytes();
        let names = |body: &[u8]| -> Vec<String> {
            let doc: serde_json::Value = serde_json::from_slice(&searched(&file, body)).unwrap();
            doc["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["metadata"]["name"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(names(&profile_query("lab-profile")), ["fred"]);
        assert!(names(&profile_query("p-uid")).is_empty());
    }

    #[test]
    fn a_project_uid_is_the_first_part_of_the_path() {
        let got = file_for(Path::new("/r"), Some("p1"), "/v1/clusterprofiles/abc");
        assert_eq!(got.unwrap(), Path::new("/r/p1/v1/clusterprofiles/abc.json"));
    }

    #[test]
    fn no_project_uid_is_tenant_scope() {
        let got = file_for(Path::new("/r"), None, &format!("/{PROJECT_LISTING}"));
        assert_eq!(
            got.unwrap(),
            Path::new("/r")
                .join(TENANT)
                .join(format!("{PROJECT_LISTING}.json"))
        );
    }

    /// An empty `ProjectUid` header is tenant scope, as an empty `--project` is.
    #[test]
    fn an_empty_project_uid_is_tenant_scope() {
        let got = file_for(Path::new("/r"), Some(""), "/v1/packs");
        assert_eq!(
            got.unwrap(),
            Path::new("/r").join(TENANT).join("v1/packs.json")
        );
    }

    /// The profile listing is under the path of the profile search, and is not the search.
    #[test]
    fn a_search_is_its_path_with_a_query() {
        assert!(with_query(
            "/v1/dashboard/clusterprofiles?limit=50",
            PROFILE_SEARCH_PATH
        ));
        assert!(!with_query(
            "/v1/dashboard/clusterprofiles",
            PROFILE_SEARCH_PATH
        ));
        assert!(!with_query(
            "/v1/dashboard/clusterprofiles/metadata",
            PROFILE_SEARCH_PATH
        ));
    }

    /// A query string is part of the file name, so one path with two queries is two files.
    #[test]
    fn a_query_string_joins_the_name_after_an_at_sign() {
        let got = file_for(
            Path::new("/r"),
            Some("p1"),
            "/v1/packs?filters=a%3Db&limit=1",
        );
        assert_eq!(
            got.unwrap(),
            Path::new("/r/p1/v1/packs@filters=a=b&limit=1.json")
        );
    }

    /// The time in a query is the real time, so the file name leaves it out.
    #[test]
    fn a_query_leaves_out_the_time() {
        let got = file_for(
            Path::new("/r"),
            Some("p1"),
            "/v1/audits?resourceKind=clusterprofile&resourceUid=u1&startTime=2026-09-24T10:00:00Z&limit=100",
        );
        assert_eq!(
            got.unwrap(),
            Path::new("/r/p1/v1/audits@resourceKind=clusterprofile&resourceUid=u1&limit=100.json")
        );
    }

    /// The validate route of a create refuses a body with a profile of another cloud type with
    /// HTTP 500 and `ClusterProfilesNotSupported`, as a live run shows (#415). A profile of the
    /// cloud type `all` fits each cloud.
    #[test]
    fn the_validate_route_of_a_create_refuses_a_profile_of_another_cloud_type() {
        let dir = std::env::temp_dir().join(format!("strata-cloud-type-{}", std::process::id()));
        let profiles = dir.join("p1/v1/clusterprofiles");
        fs::create_dir_all(&profiles).unwrap();
        for (uid, name, cloud) in [
            ("u-aws", "aws-stack", "aws"),
            ("u-gcp", "gcp-stack", "gcp"),
            ("u-all", "addons", "all"),
        ] {
            let profile = serde_json::json!({"metadata": {"name": name},
                "spec": {"published": {"cloudType": cloud}}});
            fs::write(profiles.join(format!("{uid}.json")), profile.to_string()).unwrap();
        }
        let refusal = |target: &str, uids: &[&str]| {
            let profiles: Vec<_> = uids.iter().map(|u| serde_json::json!({"uid": u})).collect();
            let request = Request {
                target: target.to_string(),
                project: Some("p1".to_string()),
                body: serde_json::json!({"spec": {"profiles": profiles}})
                    .to_string()
                    .into_bytes(),
            };
            profile_cloud_refusal(&dir, &request)
                .map(|body| serde_json::from_slice::<serde_json::Value>(&body).unwrap())
        };
        let route = "/v1/spectroclusters/aws/validate";
        assert_eq!(refusal(route, &["u-aws", "u-all"]), None);
        let got = refusal(route, &["u-aws", "u-gcp"]).unwrap();
        assert_eq!(got["code"], "ClusterProfilesNotSupported");
        assert_eq!(
            got["message"],
            "Cluster profiles 'gcp-stack' are not supported with 'aws' environment"
        );
        // the route of another cloud type, and another route under it, are not this check
        assert_eq!(
            refusal("/v1/spectroclusters/gcp/validate", &["u-gcp"]),
            None
        );
        assert_eq!(
            refusal("/v1/spectroclusters/aws/validate/packs", &["u-gcp"]),
            None
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A file of a refusal gives its status, and the first status of [`REFUSALS`] wins.
    #[test]
    fn the_file_of_a_refusal_gives_its_status() {
        let dir = std::env::temp_dir().join(format!("strata-refusals-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let answer = dir.join("name@name=barney&version=1.1.1.1.json");
        assert_eq!(refusal_of(&answer), None);
        let file = refused(&answer, 500);
        assert_eq!(file, dir.join("name@name=barney&version=1.1.1.1.500.json"));
        fs::write(&file, "{}").unwrap();
        assert_eq!(refusal_of(&answer), Some(500));
        fs::write(refused(&answer, 409), "{}").unwrap();
        assert_eq!(refusal_of(&answer), Some(409));
        fs::remove_dir_all(&dir).unwrap();
    }

    /// The file that makes the server answer HTTP 403 is beside the file of the answer.
    #[test]
    fn the_file_of_a_refusal_is_beside_the_file_of_the_answer() {
        let got = file_for(
            Path::new("/r"),
            Some("p1"),
            "/v1/spectroclusters/c1/config/rbacs",
        );
        assert_eq!(
            forbidden(&got.unwrap()),
            Path::new("/r/p1/v1/spectroclusters/c1/config/rbacs.403.json")
        );
    }

    #[test]
    fn a_path_that_climbs_out_of_the_root_answers_nothing() {
        assert!(file_for(Path::new("/r"), None, "/../etc/passwd").is_none());
        assert!(file_for(Path::new("/r"), Some(".."), "/v1/packs").is_none());
        assert!(file_for(Path::new("/r"), Some("a/b"), "/v1/packs").is_none());
        assert!(file_for(Path::new("/r"), None, "/").is_none());
    }
}
