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
//! When the file does not exist, the answer is HTTP 404 with the body that Palette gives. The
//! server reads each file at each request, so the runner can change the tenant between two
//! commands, and a page can show what Palette does after an edit in place.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::{fs, thread};

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
    let (status, body) = match file_for(root, request.project.as_deref(), &request.target) {
        Some(path)
            if path.is_file()
                && request
                    .target
                    .starts_with("/v1/dashboard/spectroclusters/search?") =>
        {
            (200, searched(&fs::read(path)?, &request.body))
        }
        Some(path) if path.is_file() => (200, fs::read(path)?),
        _ => (
            404,
            serde_json::json!({
                "code": "ResourceNotFound",
                "message": format!("no file for {}", request.target),
            })
            .to_string()
            .into_bytes(),
        ),
    };
    let reason = if status == 200 { "OK" } else { "Not Found" };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    stream.flush()
}

struct Request {
    target: String,
    project: Option<String>,
    body: Vec<u8>,
}

/// The clusters of the search file that the body of a search selects, as Palette reads it: each
/// group of conditions holds. A condition on `tags` holds when a value `key:value` names a label
/// of the cluster, or a value `key` names a label key; on `clusterProfiles` when a value is the
/// uid of a profile of the cluster; on `environment` when a value is its cloud type. The search
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
                        .any(|t| t["uid"] == v),
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
    let scope = project.filter(|p| !p.is_empty()).unwrap_or("tenant");
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

    #[test]
    fn a_project_uid_is_the_first_part_of_the_path() {
        let got = file_for(Path::new("/r"), Some("p1"), "/v1/clusterprofiles/abc");
        assert_eq!(got.unwrap(), Path::new("/r/p1/v1/clusterprofiles/abc.json"));
    }

    #[test]
    fn no_project_uid_is_tenant_scope() {
        let got = file_for(Path::new("/r"), None, "/v1/dashboard/projects/metadata");
        assert_eq!(
            got.unwrap(),
            Path::new("/r/tenant/v1/dashboard/projects/metadata.json")
        );
    }

    /// An empty `ProjectUid` header is tenant scope, as an empty `--project` is.
    #[test]
    fn an_empty_project_uid_is_tenant_scope() {
        let got = file_for(Path::new("/r"), Some(""), "/v1/packs");
        assert_eq!(got.unwrap(), Path::new("/r/tenant/v1/packs.json"));
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

    #[test]
    fn a_path_that_climbs_out_of_the_root_answers_nothing() {
        assert!(file_for(Path::new("/r"), None, "/../etc/passwd").is_none());
        assert!(file_for(Path::new("/r"), Some(".."), "/v1/packs").is_none());
        assert!(file_for(Path::new("/r"), Some("a/b"), "/v1/packs").is_none());
        assert!(file_for(Path::new("/r"), None, "/").is_none());
    }
}
