//! A stand-in Palette that serves documents by route, and a stand-in endpoint of the report that
//! records each request.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use strata::TOOL_VERSION;
use strata::closed_set::unknown::log::{FILE, Line, SCHEMA};
use strata::closed_set::unknown::sighting::Sighting;

use super::{
    PROFILE_LISTING, UID_A, UID_B, isolate_palette, profile_document, profile_route,
    strata_command, template,
};

/// The profile type that a later Palette release adds.
pub const LATER_TYPE: &str = "later-type";

/// The listing entry of a profile version, as `GET /v1/dashboard/clusterprofiles/metadata` gives
/// it.
pub fn listed(name: &str, uid: &str) -> Value {
    json!({"metadata": {"name": name, "uid": uid}, "spec": {"cloudType": "aws", "version": "1.0.0"}})
}

/// The packs of the template: one pack, so that the template is the published one.
pub fn packs() -> Value {
    json!([{"name": "cert-manager", "layer": "addon", "type": "spectro",
        "tag": "1.14.0", "version": "1.14.0"}])
}

/// The documents of two profile versions whose template has the type `LATER_TYPE`, by route.
pub fn later_type_tenant() -> BTreeMap<String, Value> {
    let mut routes = BTreeMap::new();
    routes.insert(
        PROFILE_LISTING.to_string(),
        json!({"items": [listed("payments-prod", UID_A), listed("payments-test", UID_B)]}),
    );
    for (uid, name) in [(UID_A, "payments-prod"), (UID_B, "payments-test")] {
        routes.insert(
            profile_route(uid),
            profile_document(
                uid,
                name,
                "1.0.0",
                template("aws", LATER_TYPE, packs()),
                None,
            ),
        );
    }
    routes
}

/// Serve `routes` on this machine, one answer for each request. A route that nobody lists
/// answers 404. The address is the one for `PALETTE_API`.
pub fn serve_routes(routes: BTreeMap<String, Value>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let path = read_request(&mut stream).map_or_else(|| "/".to_string(), |r| r.path);
            let (status, body) = match routes.get(&path) {
                Some(doc) => ("200 OK", doc.to_string()),
                None => ("404 Not Found", "{}".to_string()),
            };
            let answer = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(answer.as_bytes());
        }
    });
    format!("http://{address}")
}

/// The command `strata list profiles --packs` with the arguments `more` against `api`. The run
/// has a home of its own, `home`, and no state folder from the environment of the test.
pub fn list_command(api: &str, home: &Path, more: &[&str]) -> Command {
    let mut command = strata_command();
    isolate_palette(&mut command, home);
    command
        .env_remove("STRATA_QUIET")
        .env_remove("XDG_STATE_HOME")
        .env("PALETTE_API", api)
        .args(["list", "profiles", "--packs"])
        .args(more);
    command
}

/// The command `strata report` with the arguments `more`. The run has a home of its own, `home`,
/// and no state folder from the environment of the test.
pub fn report_command(home: &Path, more: &[&str]) -> Command {
    let mut command = strata_command();
    isolate_palette(&mut command, home);
    command
        .env_remove("STRATA_QUIET")
        .env_remove("XDG_STATE_HOME")
        .arg("report")
        .args(more);
    command
}

/// The state folder of the run that used `home` as its home.
pub fn state_in(home: &Path) -> PathBuf {
    home.join(".local/state/strata")
}

/// The log of the run that used `home` as its home.
pub fn log_in(home: &Path) -> PathBuf {
    state_in(home).join(FILE)
}

/// A line of the log, as strata writes it. It holds the value `value` of the set `set`, which the
/// route `route` gave to this release of strata.
pub fn log_line(set: &str, value: &str, route: &str) -> String {
    log_line_of(TOOL_VERSION, set, value, route)
}

/// A line of the log like [`log_line`], but of the release `strata`. A log holds lines of older
/// releases, so a test of a version needs a version that is not `TOOL_VERSION`.
pub fn log_line_of(strata: &str, set: &str, value: &str, route: &str) -> String {
    serde_json::to_string(&Line {
        schema: SCHEMA.to_string(),
        sighting: Sighting {
            strata: strata.to_string(),
            set: set.to_string(),
            value: value.to_string(),
            route: route.to_string(),
        },
    })
    .unwrap()
}

/// Write the file `file` with the lines `lines`, one on each row, and make its folder.
pub fn write_lines(file: &Path, lines: &[String]) {
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(file, text).unwrap();
}

/// Write the log of the run that used `home` as its home, and give its path.
pub fn write_log(home: &Path, lines: &[String]) -> PathBuf {
    let file = log_in(home);
    write_lines(&file, lines);
    file
}

/// One request that the stand-in endpoint took.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: String,
}

impl Request {
    /// The body, read as JSON.
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("{e}: {}", self.body))
    }
}

/// The body of an answer that holds no list `waiting`: the endpoint took the whole report.
const TAKEN: &str = r#"{"ok":true}"#;

/// How the stand-in endpoint answers.
#[derive(Debug, Clone, Copy)]
pub enum Answer {
    /// With this status, and a short body.
    Status(u16),
    /// The first request gets the first status, the second request the second, and so on. The
    /// last status stays for each later request.
    Statuses(&'static [u16]),
    /// With this status and this JSON body, for each request.
    Body(u16, &'static str),
    /// It reads the request and never answers, so the sender waits until its timeout.
    Silence,
}

/// A stand-in endpoint of the report on this machine. It records each request, and answers as it
/// was told.
pub struct Endpoint {
    /// The address for `STRATA_REPORT_URL`.
    pub url: String,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl Endpoint {
    /// Serve on a free port of this machine.
    pub fn answering(answer: Answer) -> Endpoint {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let Some(request) = read_request(&mut stream) else {
                    continue;
                };
                let number = {
                    let mut seen = seen.lock().unwrap();
                    seen.push(request);
                    seen.len() - 1
                };
                let (status, body) = match answer {
                    Answer::Status(status) => (Some(status), TAKEN),
                    Answer::Statuses(list) => (list.get(number).or(list.last()).copied(), TAKEN),
                    Answer::Body(status, body) => (Some(status), body),
                    Answer::Silence => (None, TAKEN),
                };
                match status {
                    Some(status) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 {status} Stand-in\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                    }
                    None => std::thread::sleep(Duration::from_secs(20)),
                }
            }
        });
        Endpoint { url, requests }
    }

    /// The requests that the endpoint took so far, in order.
    pub fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
}

/// Send each request of `command` that is not for this machine to `proxy`, a stand-in endpoint.
///
/// A proxy takes the request of the address that a run names, and it records it as `CONNECT` with
/// the host and the port. So a test sees an attempt to send, with any address, and no request
/// leaves the machine. A request to `127.0.0.1`, such as one to the stand-in for Palette, goes
/// direct.
pub fn through_proxy<'a>(command: &'a mut Command, proxy: &Endpoint) -> &'a mut Command {
    command
        .env("ALL_PROXY", &proxy.url)
        .env("NO_PROXY", "127.0.0.1")
}

/// The request on `stream`: the line, the headers, and the body that `Content-Length` names.
fn read_request(stream: &mut std::net::TcpStream) -> Option<Request> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&data[..head_end]).into_owned();
    let length = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
        })
        .unwrap_or(0);
    while data.len() < head_end + length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
        }
    }
    let mut first = head.lines().next()?.split_whitespace();
    Some(Request {
        method: first.next()?.to_string(),
        path: first.next()?.to_string(),
        body: String::from_utf8_lossy(&data[head_end..]).into_owned(),
    })
}
