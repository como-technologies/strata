use super::exec::run_command;
use super::*;
use crate::error::Result;
use crate::rule::{ExecCheck, Level, PaletteCheck, PaletteRead, Severity, Status};
use serde_json::json;
use std::time::Duration;
use strata::api::{Method, PaletteApi};

fn rule(check: Check) -> Rule {
    Rule {
        id: "t".into(),
        title: "t".into(),
        section: "s".into(),
        engines: vec![strata::model::cloud::CLOUD_ALL.into()],
        tier: check.tier(),
        level: Level::Probe,
        severity: Severity::Fail,
        status: Status::Seen,
        releases: None,
        what: "w".into(),
        when: None,
        check: Some(check),
        fix: None,
        reproduce: Default::default(),
        evidence: vec![],
        promotion: None,
        tests: vec![],
    }
}

fn fixture(v: Value) -> (FixtureBackend, Value) {
    FixtureBackend::split(v)
}

#[test]
fn a_static_check_reads_the_three_output_shapes() {
    let jq = Jq::new();
    let (b, w) = fixture(json!({"n": 4}));
    let pass = run_check(
        &rule(Check::Static {
            expr: ".n == 4".into(),
        }),
        &w,
        &b,
        &jq,
    );
    assert_eq!(pass.verdict, Verdict::Passed);
    let fail = run_check(
        &rule(Check::Static {
            expr: ".n == 3".into(),
        }),
        &w,
        &b,
        &jq,
    );
    assert_eq!(fail.verdict, Verdict::Failed);
    let list = run_check(
        &rule(Check::Static {
            expr: r#"[if .n % 3 != 0 then "size \(.n) is not a multiple of 3" else empty end]"#
                .into(),
        }),
        &w,
        &b,
        &jq,
    );
    assert_eq!(list.messages, vec!["size 4 is not a multiple of 3"]);
    let obj = run_check(
        &rule(Check::Static {
            expr: r#"{pass: (.n > 3), message: "small"}"#.into(),
        }),
        &w,
        &b,
        &jq,
    );
    assert_eq!(obj.verdict, Verdict::Passed);
    let bad = run_check(&rule(Check::Static { expr: ".n".into() }), &w, &b, &jq);
    assert_eq!(bad.verdict, Verdict::Error);
    assert!(bad.messages[0].contains("gave 4"), "{:?}", bad.messages);
}

#[test]
fn a_program_that_gives_null_is_an_error_and_not_a_pass() {
    let jq = Jq::new();
    let (b, w) = fixture(json!({"cloud": "gke", "entity": {"spec": {"items": []}}}));
    let typo = rule(Check::Static {
        expr: ".entity.spec.itmes | length == 0".into(),
    });
    let out = run_check(&typo, &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::Passed, "length of null is 0 in jq");
    let path = rule(Check::Static {
        expr: ".entity.spec.itmes".into(),
    });
    let out = run_check(&path, &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::Error);
    assert!(out.messages[0].contains("gave null"), "{:?}", out.messages);
}

#[test]
fn a_palette_check_reads_a_4xx_as_an_answer() {
    let jq = Jq::new();
    let (b, w) = fixture(json!({"cloud": "gke", "entity": {"spec": {}},
            "_responses": {"POST /v1/spectroclusters/gke/validate": {"ok": false, "status": 400,
                "body": {"code": "InvalidParam", "message": "Node pool size should be multiple of 3"}}}}));
    let r = rule(Check::Palette(PaletteCheck {
            method: Method::Post,
            path: "/v1/spectroclusters/{{ .cloud }}/validate".into(),
            body: None,
            each: None,
            expr: r#"[select(.response.ok | not) | "Palette: \(.response.body.code): \(.response.body.message)"]"#.into(),
        }));
    let out = run_check(&r, &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::Failed);
    assert_eq!(
        out.messages,
        vec!["Palette: InvalidParam: Node pool size should be multiple of 3"]
    );
    assert_eq!(
        out.ran.text().as_deref(),
        Some("POST /v1/spectroclusters/gke/validate")
    );
}

fn each_check(each: &str, path: &str, expr: &str) -> Rule {
    rule(Check::Palette(PaletteCheck {
        method: Method::Get,
        path: path.into(),
        body: None,
        each: Some(each.into()),
        expr: expr.into(),
    }))
}

#[test]
fn a_palette_check_with_each_asks_once_for_each_item_and_reads_the_responses_in_order() {
    let jq = Jq::new();
    let ok = |name: &str| json!({"ok": true, "status": 200, "body": {"metadata": {"name": name}}});
    let (b, w) = fixture(
        json!({"entity": {"spec": {"profiles": [{"uid": "p1"}, {"uid": "p2"}]}},
            "_responses": {"GET /v1/clusterprofiles/p1": ok("base"),
                "GET /v1/clusterprofiles/p2": {"ok": false, "status": 404, "body": {"code": "ResourceNotFound"}}}}),
    );
    let r = each_check(
        ".entity.spec.profiles",
        "/v1/clusterprofiles/{{ $item.uid }}",
        r#"[.responses[] | "\(.item.uid) \(.response.status) \(.response.body.metadata.name // .response.body.code)"]"#,
    );
    let out = run_check(&r, &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::Failed);
    assert_eq!(out.messages, vec!["p1 200 base", "p2 404 ResourceNotFound"]);
    assert_eq!(
        out.ran,
        Ran::Requests(vec![
            Call::get("/v1/clusterprofiles/p1", Some("HTTP 200")),
            Call::get("/v1/clusterprofiles/p2", Some("HTTP 404")),
        ])
    );
    assert_eq!(out.ran.text(), None);
}

#[test]
fn a_post_with_each_fills_its_body_from_the_item() {
    struct Echo(std::cell::RefCell<Vec<(String, Value)>>);
    impl Backend for Echo {
        fn palette(&self, _m: Method, path: &str, body: Option<&Value>) -> Result<Response> {
            self.0
                .borrow_mut()
                .push((path.to_string(), body.cloned().unwrap_or(Value::Null)));
            Ok(Response {
                ok: true,
                status: 200,
                body: Value::Null,
            })
        }
        fn has_tool(&self, _: &str) -> bool {
            true
        }
        fn exec(&self, _: &[String], _: Duration) -> Result<ExecOutput> {
            unreachable!("no command in this test")
        }
        fn has_palette(&self) -> bool {
            true
        }
    }
    let echo = Echo(Default::default());
    let r = rule(Check::Palette(PaletteCheck {
        method: Method::Post,
        path: "/v1/capacity?arn={{ $item.arn }}".into(),
        body: Some("{arn: $item.arn, size: ($item.size + 1)}".into()),
        each: Some("[.pools[] | select(.arn != null)]".into()),
        expr: "[.responses[] | select(.response.ok | not) | .item.arn]".into(),
    }));
    let world = json!({"pools": [{"arn": "a1", "size": 1}, {"size": 9}, {"arn": "a2", "size": 2}]});
    let out = run_check(&r, &world, &echo, &Jq::new());
    assert_eq!(out.verdict, Verdict::Passed);
    assert_eq!(
        *echo.0.borrow(),
        vec![
            (
                "/v1/capacity?arn=a1".to_string(),
                json!({"arn": "a1", "size": 2})
            ),
            (
                "/v1/capacity?arn=a2".to_string(),
                json!({"arn": "a2", "size": 3})
            ),
        ]
    );
}

#[test]
fn a_check_with_each_over_no_item_asks_nothing_and_passes_the_empty_list() {
    let jq = Jq::new();
    let (b, w) = fixture(json!({"hosts": []}));
    let r = each_check(
        ".hosts",
        "/v1/edgehosts/{{ $item }}",
        "[.responses[] | .item]",
    );
    let out = run_check(&r, &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::Passed);
    assert_eq!(out.ran, Ran::Requests(vec![]));
}

#[test]
fn a_check_with_each_is_an_error_when_each_gives_no_list_or_a_request_fails() {
    let jq = Jq::new();
    let (b, w) = fixture(json!({"hosts": "h1", "list": ["h1", "h2"],
            "_responses": {"GET /v1/edgehosts/h1": {"ok": true, "status": 200, "body": {}}}}));
    let out = run_check(
        &each_check(".hosts", "/v1/edgehosts/{{ $item }}", "true"),
        &w,
        &b,
        &jq,
    );
    assert_eq!(out.verdict, Verdict::Error);
    assert_eq!(out.messages, vec![r#"`each` gave "h1"; give a list"#]);
    let out = run_check(
        &each_check(".missing", "/v1/edgehosts/{{ $item }}", "true"),
        &w,
        &b,
        &jq,
    );
    assert_eq!(out.verdict, Verdict::Error);
    assert_eq!(out.messages, vec!["`each` gave null; give a list"]);
    // the second request has no answer: the check is an error, and the report keeps both requests
    let out = run_check(
        &each_check(".list", "/v1/edgehosts/{{ $item }}", "true"),
        &w,
        &b,
        &jq,
    );
    assert_eq!(out.verdict, Verdict::Error);
    assert!(
        out.messages[0].contains("GET /v1/edgehosts/h2"),
        "{:?}",
        out.messages
    );
    assert_eq!(
        out.ran,
        Ran::Requests(vec![
            Call::get("/v1/edgehosts/h1", Some("HTTP 200")),
            Call::get("/v1/edgehosts/h2", None),
        ])
    );
}

#[test]
fn a_401_a_403_and_a_429_are_errors_of_the_check_and_a_404_is_its_answer() {
    for status in [401u16, 403, 429] {
        let api = strata::api::MemoryApi::new().failing("/v1/x", status, "{}");
        let b = LiveBackend::new(Some(&api), false);
        let err = b
            .palette(Method::Get, "/v1/x", None)
            .unwrap_err()
            .to_string();
        assert!(err.contains(&format!("HTTP {status}")), "{status}: {err}");
    }
    let api = strata::api::MemoryApi::new().failing("/v1/x", 404, r#"{"code": "NotFound"}"#);
    let b = LiveBackend::new(Some(&api), false);
    let r = b.palette(Method::Get, "/v1/x", None).unwrap();
    assert!(!r.ok);
    assert_eq!(r.status, 404);
    assert_eq!(r.body["code"], "NotFound");
}

#[test]
fn the_backend_sends_a_request_one_time_and_gives_the_same_answer_to_the_rule_that_asks_again() {
    let api = strata::api::MemoryApi::new()
        .with("/v1/edgehosts/h1", json!({"metadata": {"uid": "h1"}}))
        .failing("/v1/edgehosts/h2", 404, r#"{"code": "NotFound"}"#);
    let b = LiveBackend::new(Some(&api), false);
    for _ in 0..6 {
        let found = b.palette(Method::Get, "/v1/edgehosts/h1", None).unwrap();
        assert_eq!((found.ok, found.status), (true, 200));
        let gone = b.palette(Method::Get, "/v1/edgehosts/h2", None).unwrap();
        assert_eq!((gone.ok, gone.status), (false, 404));
    }
    assert_eq!(api.calls("/v1/edgehosts/h1"), 1, "an answer");
    assert_eq!(api.calls("/v1/edgehosts/h2"), 1, "a 4xx is an answer too");
}

#[test]
fn the_backend_tells_two_posts_with_two_bodies_apart_and_sends_each_body_one_time() {
    let api = strata::api::MemoryApi::new().with("POST /v1/x", json!({"ok": true}));
    let b = LiveBackend::new(Some(&api), false);
    let (one, two) = (json!({"name": "a"}), json!({"name": "b"}));
    for _ in 0..3 {
        b.palette(Method::Post, "/v1/x", Some(&one)).unwrap();
        b.palette(Method::Post, "/v1/x", Some(&two)).unwrap();
    }
    assert_eq!(
        api.posted(),
        vec![("/v1/x".to_string(), one), ("/v1/x".to_string(), two)]
    );
}

#[test]
fn the_backend_does_not_keep_an_error_that_no_check_can_read() {
    for status in [500u16, 503, 401] {
        let api = strata::api::MemoryApi::new().failing("/v1/x", status, "{}");
        let b = LiveBackend::new(Some(&api), false);
        assert!(b.palette(Method::Get, "/v1/x", None).is_err());
        assert!(b.palette(Method::Get, "/v1/x", None).is_err());
        assert_eq!(api.calls("/v1/x"), 2, "HTTP {status} is asked again");
    }
}

#[test]
fn the_world_and_the_checks_share_the_answers_of_the_backend() {
    let api = strata::api::MemoryApi::new()
        .with("/v1/clusterprofiles/p1", json!({"metadata": {"uid": "p1"}}));
    let b = LiveBackend::new(Some(&api), false);
    // the world reads through the backend as a client of the library
    let read = strata::api::Palette::new(&b);
    assert_eq!(
        read.api().get_json("/v1/clusterprofiles/p1").unwrap()["metadata"]["uid"],
        "p1"
    );
    let again = b
        .palette(Method::Get, "/v1/clusterprofiles/p1", None)
        .unwrap();
    assert!(again.ok);
    assert_eq!(api.calls("/v1/clusterprofiles/p1"), 1);
    // an answer that is an HTTP error is the error that Palette gave, with its body as it was
    let api = strata::api::MemoryApi::new().failing("/v1/gone", 404, "not json");
    let b = LiveBackend::new(Some(&api), false);
    let err = b.get_json("/v1/gone").unwrap_err();
    assert!(
        matches!(&err, strata::Error::Api { status: 404, body, .. } if body == "not json"),
        "{err}"
    );
    assert_eq!(
        b.palette(Method::Get, "/v1/gone", None).unwrap().body["message"],
        "not json"
    );
}

#[test]
fn only_the_cluster_validate_route_has_its_refusal_with_http_500_as_an_answer() {
    let refusal = r#"{"code":"ClusterProfilesNotSupported","message":"not supported"}"#;
    let api = strata::api::MemoryApi::new()
        .failing("POST /v1/spectroclusters/aws/validate", 500, refusal)
        .failing("POST /v1/clouds/aws/account/validate", 500, refusal);
    let b = LiveBackend::new(Some(&api), false);
    let body = json!({});
    let answer = b
        .palette(
            Method::Post,
            "/v1/spectroclusters/aws/validate",
            Some(&body),
        )
        .unwrap();
    assert_eq!((answer.ok, answer.status), (false, 500));
    assert_eq!(answer.body["code"], "ClusterProfilesNotSupported");
    // the same body from another route is an error of Palette, not an answer
    assert!(
        b.palette(Method::Post, "/v1/clouds/aws/account/validate", Some(&body))
            .is_err()
    );
}

#[test]
fn a_palette_check_with_no_key_is_not_run() {
    let jq = Jq::new();
    let (b, w) = fixture(json!({"cloud": "gke", "_palette": false}));
    let r = rule(Check::Palette(PaletteCheck {
        method: Method::Get,
        path: "/v1/x".into(),
        body: None,
        each: None,
        expr: "true".into(),
    }));
    assert_eq!(run_check(&r, &w, &b, &jq).verdict, Verdict::NotRun);
}

#[test]
fn an_exec_check_needs_its_tool_and_reads_the_output() {
    let jq = Jq::new();
    let (b, w) = fixture(json!({"region": "us-east-2", "_tools": ["aws"],
            "_exec": {"aws ec2 describe-subnets --region us-east-2": {"exit": 0, "stdout": {"Subnets": [{"AvailabilityZone": "us-east-2a"}]}, "stderr": ""}}}));
    let r = rule(Check::Exec(ExecCheck {
        argv: vec!["aws".into(), "ec2".into(), "describe-subnets".into(), "--region".into(), "{{ .region }}".into()],
        needs: vec!["aws".into()],
        timeout: None,
        palette: None,
        expr: r#"[.exec.stdout.Subnets | map(.AvailabilityZone) | unique | select(length < 2) | "one zone: \(.)"]"#.into(),
    }));
    let out = run_check(&r, &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::Failed);
    assert_eq!(out.messages, vec![r#"one zone: ["us-east-2a"]"#]);
    assert_eq!(
        out.ran,
        Ran::Exec {
            line: "aws ec2 describe-subnets --region us-east-2".into(),
            answer: Some("exit 0".into()),
            request: None,
        }
    );
    let missing = rule(Check::Exec(ExecCheck {
        argv: vec!["gcloud".into(), "services".into(), "list".into()],
        needs: vec!["gcloud".into()],
        timeout: None,
        palette: None,
        expr: "true".into(),
    }));
    let out = run_check(&missing, &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::NotChecked);
    assert!(
        out.messages[0].contains("gcloud not on this machine"),
        "{:?}",
        out.messages
    );
}

/// An exec check that reads the cpu of its type from Palette, and the quota from the tool.
fn exec_with_a_read() -> Rule {
    rule(Check::Exec(ExecCheck {
        argv: vec![
            "aws".into(),
            "service-quotas".into(),
            "{{ .region }}".into(),
        ],
        needs: vec!["aws".into()],
        timeout: None,
        palette: Some(PaletteRead {
            path: "/v1/clouds/aws/regions/{{ .region }}/instancetypes".into(),
        }),
        expr: r#".exec.stdout.Quota.Value as $quota
            | [ .response.body.instanceTypes[] | select(.type == "t3.xlarge" and .cpu > $quota)
                | "\(.type) asks \(.cpu) vCPUs, and the quota is \($quota)" ]"#
            .into(),
    }))
}

#[test]
fn an_exec_check_with_a_read_gives_the_program_the_response_and_the_output() {
    let jq = Jq::new();
    let (b, w) = fixture(json!({"region": "us-east-2",
        "_exec": {"aws service-quotas us-east-2": {"exit": 0, "stdout": {"Quota": {"Value": 2.0}}, "stderr": ""}},
        "_responses": {"GET /v1/clouds/aws/regions/us-east-2/instancetypes": {"ok": true, "status": 200,
            "body": {"instanceTypes": [{"type": "t3.xlarge", "cpu": 4.0}]}}}}));
    let out = run_check(&exec_with_a_read(), &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::Failed, "{:?}", out.messages);
    assert_eq!(
        out.messages,
        vec!["t3.xlarge asks 4.0 vCPUs, and the quota is 2.0"]
    );
    assert_eq!(
        out.ran,
        Ran::Exec {
            line: "aws service-quotas us-east-2".into(),
            answer: Some("exit 0".into()),
            request: Some(Call::get(
                "/v1/clouds/aws/regions/us-east-2/instancetypes",
                Some("HTTP 200")
            )),
        }
    );
    // the report shows the command, and the one request beside it
    assert_eq!(
        out.ran.text().as_deref(),
        Some("aws service-quotas us-east-2")
    );
    assert_eq!(out.ran.requests().len(), 1);
}

#[test]
fn an_exec_check_with_a_read_and_no_key_is_not_run_and_runs_no_command() {
    let jq = Jq::new();
    // the fixture has no output for the command: a command that ran would be an error
    let (b, w) = fixture(json!({"region": "us-east-2", "_palette": false}));
    let out = run_check(&exec_with_a_read(), &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::NotRun);
    assert!(out.messages[0].contains("no key"), "{:?}", out.messages);
    assert_eq!(out.ran, Ran::Nothing);
}

#[test]
fn an_exec_check_with_a_read_and_no_tool_asks_palette_nothing() {
    let jq = Jq::new();
    // the fixture has no answer for the request: a request that went out would be an error
    let (b, w) = fixture(json!({"region": "us-east-2", "_tools": []}));
    let out = run_check(&exec_with_a_read(), &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::NotChecked);
    assert_eq!(
        out.ran,
        Ran::Exec {
            line: "aws service-quotas us-east-2".into(),
            answer: None,
            request: None,
        }
    );
}

#[test]
fn an_exec_check_whose_read_fails_is_an_error_that_names_the_request() {
    let jq = Jq::new();
    let (b, w) = fixture(json!({"region": "us-east-2"}));
    let out = run_check(&exec_with_a_read(), &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::Error);
    assert_eq!(
        out.ran,
        Ran::Request(Call::get(
            "/v1/clouds/aws/regions/us-east-2/instancetypes",
            None
        ))
    );
}

#[test]
fn a_real_command_runs_with_a_time_limit() {
    let ok = run_command(
        &["sh".into(), "-c".into(), "echo '{\"a\":1}'".into()],
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(ok.exit, 0);
    assert_eq!(ok.stdout, json!({"a": 1}));
    let text = run_command(
        &[
            "sh".into(),
            "-c".into(),
            "echo hi; echo err >&2; exit 3".into(),
        ],
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(
        (text.exit, text.stdout.as_str(), text.stderr.as_str()),
        (3, Some("hi"), "err")
    );
    let slow = run_command(&["sleep".into(), "5".into()], Duration::from_millis(200)).unwrap();
    assert_eq!(slow.exit, -1);
    assert!(slow.stderr.starts_with("stopped after"), "{}", slow.stderr);
    assert!(run_command(&["no-such-tool-xyz".into()], Duration::from_secs(1)).is_err());
}

#[test]
fn a_message_with_a_line_break_prints_on_one_line() {
    let jq = Jq::new();
    let (b, w) = fixture(json!({"e": "aws failed: \nNoCredentials\n"}));
    let out = run_check(
        &rule(Check::Static {
            expr: r#"["tool said: \(.e)"]"#.into(),
        }),
        &w,
        &b,
        &jq,
    );
    assert_eq!(out.messages, vec!["tool said: aws failed: NoCredentials"]);
}

#[test]
fn shell_words_quotes_what_needs_it() {
    assert_eq!(
        shell_words(&["a".into(), "b c".into(), "d'e".into()]),
        "a 'b c' 'd'\\''e'"
    );
}
