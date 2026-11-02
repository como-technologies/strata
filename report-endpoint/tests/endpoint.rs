//! The endpoint end to end: the send of strata posts to the real service, and the service writes
//! to a stand-in of GitHub on this machine. The stand-in keeps the issues and records each call.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use strata::closed_set::unknown::Report;
use strata::closed_set::unknown::send::post;
use strata_report_endpoint::server::{Service, serve};
use strata_report_endpoint::tracker::GitHub;

const TOKEN: &str = "ghp_test_token_0123456789";

/// An issue of the stand-in of GitHub.
#[derive(Debug, Clone)]
struct Stored {
    number: u64,
    title: String,
    body: String,
    labels: Vec<String>,
    milestone: Option<u64>,
    state: &'static str,
    pull_request: bool,
}

/// One call that the stand-in of GitHub took.
#[derive(Debug, Clone)]
struct Call {
    method: String,
    path: String,
    authorization: Option<String>,
    body: Value,
}

/// The stand-in of GitHub: the issues, the calls, and a switch that makes it refuse.
#[derive(Default)]
struct GitHubState {
    issues: Vec<Stored>,
    calls: Vec<Call>,
    refuse: bool,
}

type Shared = Arc<Mutex<GitHubState>>;

/// Serve the API of issues of GitHub on a free port. Gives the address and the state.
fn stand_in_github() -> (String, Shared) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let address = format!("http://{}", server.server_addr().to_ip().unwrap());
    let state: Shared = Arc::default();
    let shared = Arc::clone(&state);
    std::thread::spawn(move || {
        for mut request in server.incoming_requests() {
            let mut text = String::new();
            request.as_reader().read_to_string(&mut text).unwrap();
            let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            let method = request.method().as_str().to_string();
            let url = request.url().to_string();
            let authorization = request
                .headers()
                .iter()
                .find(|h| h.field.equiv("Authorization"))
                .map(|h| h.value.as_str().to_string());
            let mut state = shared.lock().unwrap();
            state.calls.push(Call {
                method: method.clone(),
                path: url.clone(),
                authorization,
                body: body.clone(),
            });
            let (status, answer) = if state.refuse {
                (401, json!({"message": "Bad credentials"}))
            } else {
                github_answer(&mut state, &method, &url, &body)
            };
            let response = tiny_http::Response::from_string(answer.to_string())
                .with_status_code(status)
                .with_header(
                    tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                );
            let _ = request.respond(response);
        }
    });
    (address, state)
}

/// The answer of the stand-in to one call.
fn github_answer(state: &mut GitHubState, method: &str, url: &str, body: &Value) -> (u16, Value) {
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let path = path.strip_prefix("/repos/o/r").unwrap_or("?");
    match (method, path) {
        ("GET", "/milestones") => (
            200,
            json!([{"title": "Wave", "number": 8}, {"title": "Backlog", "number": 7}]),
        ),
        ("GET", "/issues") => {
            let page: usize = query
                .split('&')
                .find_map(|p| p.strip_prefix("page="))
                .and_then(|n| n.parse().ok())
                .unwrap_or(1);
            let items: Vec<Value> = state
                .issues
                .iter()
                .skip((page - 1) * 100)
                .take(100)
                .map(|i| {
                    let mut item = json!({"number": i.number, "title": i.title, "body": i.body,
                        "state": i.state});
                    if i.pull_request {
                        item["pull_request"] = json!({});
                    }
                    item
                })
                .collect();
            (200, json!(items))
        }
        ("POST", "/issues") => {
            let number = state.issues.len() as u64 + 1;
            state.issues.push(Stored {
                number,
                title: body["title"].as_str().unwrap().into(),
                body: body["body"].as_str().unwrap().into(),
                labels: serde_json::from_value(body["labels"].clone()).unwrap(),
                milestone: body["milestone"].as_u64(),
                state: "open",
                pull_request: false,
            });
            (201, json!({"number": number}))
        }
        ("PATCH", p) if p.starts_with("/issues/") => {
            let number: u64 = p["/issues/".len()..].parse().unwrap();
            let issue = state
                .issues
                .iter_mut()
                .find(|i| i.number == number)
                .unwrap();
            issue.body = body["body"].as_str().unwrap().into();
            (200, json!({"number": number}))
        }
        _ => (404, json!({"message": "Not Found"})),
    }
}

/// Start the real service against the stand-in. Gives the address of the service.
fn start_service(github: &str) -> String {
    start_service_behind(github, false)
}

/// Start the real service. With `proxy`, it reads the sender from `X-Forwarded-For`, as it does
/// behind the front end of Cloud Run.
fn start_service_behind(github: &str, proxy: bool) -> String {
    let tracker = GitHub::new(github, "o/r", TOKEN, "palette-drift", Some("Backlog"));
    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let address = format!("http://{}/", server.server_addr().to_ip().unwrap());
    let service = Arc::new(Mutex::new(Service::new(tracker)));
    std::thread::spawn(move || serve(&server, &service, proxy));
    address
}

fn report(entries: &[Value]) -> Report {
    serde_json::from_value(json!({"schema": "strata-report/1", "unknown": entries})).unwrap()
}

fn entry(value: &str, count: u64) -> Value {
    json!({"strata": "0.26.0", "palette": "4.7.1", "set": "ProfileType", "value": value,
        "route": "/v1/clusterprofiles/{uid}", "count": count})
}

/// What a raw POST of `body` to the service answers: the status and the JSON.
fn raw_post(address: &str, content_type: &str, body: &str) -> (u16, Value) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let mut answer = agent
        .post(address)
        .content_type(content_type)
        .send(body)
        .unwrap();
    let status = answer.status().as_u16();
    let text = answer.body_mut().read_to_string().unwrap();
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

fn writes(state: &Shared) -> usize {
    state
        .lock()
        .unwrap()
        .calls
        .iter()
        .filter(|c| c.method != "GET")
        .count()
}

#[test]
fn two_sends_of_strata_for_one_value_are_one_issue_with_a_count_of_both() {
    let (github, state) = stand_in_github();
    let service = start_service(&github);
    post(&service, &report(&[entry("later", 2)])).unwrap();
    post(&service, &report(&[entry("later", 3)])).unwrap();
    let state = state.lock().unwrap();
    assert_eq!(state.issues.len(), 1, "{:?}", state.issues);
    let issue = &state.issues[0];
    assert_eq!(issue.labels, ["palette-drift"]);
    assert_eq!(issue.milestone, Some(7), "the milestone Backlog");
    assert_eq!(issue.state, "open");
    assert!(
        issue
            .body
            .starts_with("<!-- strata-report: values ProfileType later -->\n")
    );
    assert!(issue.body.contains("| Lines | 5 |"), "{}", issue.body);
    assert!(issue.body.contains("| Reports | 2 |"), "{}", issue.body);
    // one list, one milestone lookup, one open for the first send; one list and one change for the second
    let calls: Vec<(&str, &str)> = state
        .calls
        .iter()
        .map(|c| (c.method.as_str(), c.path.split('?').next().unwrap()))
        .collect();
    assert_eq!(
        calls,
        [
            ("GET", "/repos/o/r/issues"),
            ("GET", "/repos/o/r/milestones"),
            ("POST", "/repos/o/r/issues"),
            ("GET", "/repos/o/r/issues"),
            ("PATCH", "/repos/o/r/issues/1"),
        ]
    );
}

#[test]
fn the_service_writes_with_the_token_and_the_token_is_in_no_issue_and_no_answer() {
    let (github, state) = stand_in_github();
    let service = start_service(&github);
    post(&service, &report(&[entry("later", 1)])).unwrap();
    let (status, answer) = raw_post(
        &service,
        "application/json",
        &report_text(&[entry("later", 1)]),
    );
    assert_eq!(status, 200);
    assert!(!answer.to_string().contains(TOKEN));
    let state = state.lock().unwrap();
    for call in &state.calls {
        assert_eq!(
            call.authorization.as_deref(),
            Some(format!("Bearer {TOKEN}").as_str())
        );
    }
    for issue in &state.issues {
        assert!(!issue.body.contains(TOKEN) && !issue.title.contains(TOKEN));
    }
}

fn report_text(entries: &[Value]) -> String {
    json!({"schema": "strata-report/1", "unknown": entries}).to_string()
}

#[test]
fn a_report_with_another_field_is_refused_and_nothing_is_written() {
    let (github, state) = stand_in_github();
    let service = start_service(&github);
    let mut with = entry("later", 1);
    with["name"] = json!("payments-prod");
    let (status, answer) = raw_post(&service, "application/json", &report_text(&[with]));
    assert_eq!(status, 422);
    assert!(
        answer["error"]
            .as_str()
            .unwrap()
            .contains("unknown field `name`"),
        "{answer}"
    );
    assert!(!answer.to_string().contains("payments-prod"));
    assert!(
        state.lock().unwrap().calls.is_empty(),
        "the endpoint did not even read the issues"
    );
}

#[test]
fn a_route_that_is_not_a_template_is_refused_and_nothing_is_written() {
    let (github, state) = stand_in_github();
    let service = start_service(&github);
    for route in [
        "/v1/packs\n\n@someone read [this](http://example.invalid)",
        "/v1/packs\r\n![x](http://example.invalid/x.png)",
        "/v1/packs`x`",
        "/v1/packs|x",
        "/v1/@someone",
        "/v1/pa cks",
        "/v1/<b>packs</b>",
        "/v1/packs\u{202e}",
        "/other/packs",
        "v1/packs",
    ] {
        let mut with = entry("later", 1);
        with["route"] = json!(route);
        let (status, answer) = raw_post(&service, "application/json", &report_text(&[with]));
        assert_eq!(status, 422, "{route:?}");
        assert!(
            answer["error"]
                .as_str()
                .unwrap()
                .contains("the route is not a template of a route"),
            "{answer}"
        );
        assert!(!answer.to_string().contains("someone"), "{answer}");
    }
    assert!(
        state.lock().unwrap().calls.is_empty(),
        "the endpoint did not even read the issues"
    );
    // a second service has its own limit of the rate: the refusals above spent that of the first
    let (status, _) = raw_post(
        &start_service(&github),
        "application/json",
        &report_text(&[entry("later", 1)]),
    );
    assert_eq!(status, 200, "a template of a route passes");
}

#[test]
fn a_request_that_is_not_json_or_not_a_report_is_refused_with_its_status() {
    let (github, state) = stand_in_github();
    let service = start_service(&github);
    assert_eq!(raw_post(&service, "text/plain", "x").0, 415);
    assert_eq!(raw_post(&service, "application/json", "x").0, 400);
    assert_eq!(
        raw_post(&service, "application/json", &"x".repeat(70_000)).0,
        413
    );
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    assert_eq!(agent.get(&service).call().unwrap().status().as_u16(), 405);
    assert_eq!(
        agent
            .post(&format!("{service}other"))
            .send("{}")
            .unwrap()
            .status()
            .as_u16(),
        404
    );
    assert_eq!(writes(&state), 0);
}

#[test]
fn the_eleventh_request_of_a_sender_in_an_hour_gets_429() {
    let (github, _) = stand_in_github();
    let service = start_service(&github);
    let text = report_text(&[entry("later", 1)]);
    for _ in 0..10 {
        assert_eq!(raw_post(&service, "application/json", &text).0, 200);
    }
    assert_eq!(raw_post(&service, "application/json", &text).0, 429);
}

#[test]
fn an_issue_on_the_second_page_is_found_and_a_pull_request_with_a_marker_is_not() {
    let (github, state) = stand_in_github();
    {
        let mut state = state.lock().unwrap();
        for number in 1..=150 {
            state.issues.push(Stored {
                number,
                title: format!("issue {number}"),
                body: format!("<!-- strata-contract: {number} -->"),
                labels: vec!["palette-drift".into()],
                milestone: None,
                state: "open",
                pull_request: false,
            });
        }
        // the first page holds a pull request with our marker, and the second page the issue
        state.issues[0].body = "<!-- strata-report: values ProfileType later -->\nx".into();
        state.issues[0].pull_request = true;
        state.issues[120].body = "<!-- strata-report: values ProfileType later -->\nx".into();
        state.issues[120].state = "closed";
    }
    let service = start_service(&github);
    post(&service, &report(&[entry("later", 1)])).unwrap();
    let state = state.lock().unwrap();
    assert_eq!(state.issues.len(), 150, "no issue was opened");
    assert!(state.issues[120].body.contains("| Lines | 1 |"));
    assert_eq!(
        state.issues[120].state, "closed",
        "the merge does not open a closed issue"
    );
    assert_eq!(
        state.issues[0].body,
        "<!-- strata-report: values ProfileType later -->\nx"
    );
    let patch = state.calls.iter().find(|c| c.method == "PATCH").unwrap();
    assert_eq!(patch.path, "/repos/o/r/issues/121");
    assert_eq!(
        patch.body.as_object().unwrap().len(),
        1,
        "only the body changes: {}",
        patch.body
    );
}

#[test]
fn a_github_that_refuses_the_token_gives_the_sender_a_502_and_no_detail() {
    let (github, state) = stand_in_github();
    state.lock().unwrap().refuse = true;
    let service = start_service(&github);
    let (status, answer) = raw_post(
        &service,
        "application/json",
        &report_text(&[entry("later", 1)]),
    );
    assert_eq!(status, 502);
    assert!(!answer.to_string().contains("Bad credentials"), "{answer}");
    assert!(!answer.to_string().contains(TOKEN));
    let error = post(&service, &report(&[entry("later", 1)]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("HTTP 502"), "{error}");
}

#[test]
fn the_report_of_the_send_of_strata_passes_the_check_of_the_endpoint_for_each_set_of_the_log() {
    // the entries that strata makes from its own log pass: the client and the endpoint share one check
    let (github, _) = stand_in_github();
    let service = start_service(&github);
    let lines: Vec<strata::closed_set::unknown::Line> = serde_json::from_str(
        r#"[{"schema":"strata-unknown/1","strata":"0.26.0","set":"PackState","value":"retired","route":"/v1/packs"},
            {"schema":"strata-unknown/1","strata":"0.26.0","palette":"4.7.1","set":"ProfileType","value":"later","route":"/v1/clusterprofiles/{uid}","status":409,"errorCode":"Gone"}]"#,
    )
    .unwrap();
    post(&service, &Report::of(&lines)).unwrap();
}

/// A POST of `body` that says in `X-Forwarded-For` that it comes from `address`. Gives the status.
fn post_from(service: &str, address: &str, body: &str) -> u16 {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    agent
        .post(service)
        .header("X-Forwarded-For", address)
        .content_type("application/json")
        .send(body)
        .unwrap()
        .status()
        .as_u16()
}

#[test]
fn a_service_that_no_proxy_fronts_ignores_the_forwarded_header_and_limits_the_connection() {
    let (github, _) = stand_in_github();
    let service = start_service(&github);
    let text = report_text(&[entry("later", 1)]);
    // a client that sends a new address with each request is still one sender
    let statuses: Vec<u16> = (0..11)
        .map(|n| post_from(&service, &format!("203.0.113.{n}"), &text))
        .collect();
    assert_eq!(&statuses[..10], [200; 10]);
    assert_eq!(statuses[10], 429, "{statuses:?}");
}

#[test]
fn a_service_behind_the_proxy_limits_each_address_that_the_proxy_names_by_its_last_entry() {
    let (github, _) = stand_in_github();
    let service = start_service_behind(&github, true);
    let text = report_text(&[entry("later", 1)]);
    // the sender writes the first entry, and the proxy adds the true address at the end
    for n in 0..11 {
        let header = format!("198.51.100.{n}, 203.0.113.9");
        let status = post_from(&service, &header, &text);
        if n < 10 {
            assert_eq!(status, 200, "{header}");
        } else {
            assert_eq!(status, 429, "the 11th request of 203.0.113.9: {header}");
        }
    }
    // another true address has its own limit
    assert_eq!(
        post_from(&service, "198.51.100.1, 203.0.113.10", &text),
        200
    );
}

#[test]
fn more_issues_than_the_service_reads_give_502_and_no_second_issue_for_the_value() {
    let (github, state) = stand_in_github();
    {
        let mut state = state.lock().unwrap();
        for number in 1..=1000 {
            state.issues.push(Stored {
                number,
                title: format!("issue {number}"),
                body: format!("<!-- strata-contract: {number} -->"),
                labels: vec!["palette-drift".into()],
                milestone: None,
                state: "open",
                pull_request: false,
            });
        }
    }
    let service = start_service(&github);
    let (status, _) = raw_post(
        &service,
        "application/json",
        &report_text(&[entry("later", 1)]),
    );
    assert_eq!(status, 502);
    assert_eq!(
        writes(&state),
        0,
        "the service opened no issue it could not find again"
    );
    assert_eq!(state.lock().unwrap().issues.len(), 1000);
}
