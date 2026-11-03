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
            refusal_codes: Vec::new(),
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
        refusal_codes: Vec::new(),
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
        fn palette(
            &self,
            _m: Method,
            path: &str,
            body: Option<&Value>,
            _refusal_codes: &[String],
        ) -> Result<Response> {
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
        refusal_codes: Vec::new(),
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
            .palette(Method::Get, "/v1/x", None, &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains(&format!("HTTP {status}")), "{status}: {err}");
    }
    let api = strata::api::MemoryApi::new().failing("/v1/x", 404, r#"{"code": "NotFound"}"#);
    let b = LiveBackend::new(Some(&api), false);
    let r = b.palette(Method::Get, "/v1/x", None, &[]).unwrap();
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
        let found = b
            .palette(Method::Get, "/v1/edgehosts/h1", None, &[])
            .unwrap();
        assert_eq!((found.ok, found.status), (true, 200));
        let gone = b
            .palette(Method::Get, "/v1/edgehosts/h2", None, &[])
            .unwrap();
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
        b.palette(Method::Post, "/v1/x", Some(&one), &[]).unwrap();
        b.palette(Method::Post, "/v1/x", Some(&two), &[]).unwrap();
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
        assert!(b.palette(Method::Get, "/v1/x", None, &[]).is_err());
        assert!(b.palette(Method::Get, "/v1/x", None, &[]).is_err());
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
        .palette(Method::Get, "/v1/clusterprofiles/p1", None, &[])
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
        b.palette(Method::Get, "/v1/gone", None, &[]).unwrap().body["message"],
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
            &[],
        )
        .unwrap();
    assert_eq!((answer.ok, answer.status), (false, 500));
    assert_eq!(answer.body["code"], "ClusterProfilesNotSupported");
    // the same body from another route is an error of Palette, not an answer
    assert!(
        b.palette(
            Method::Post,
            "/v1/clouds/aws/account/validate",
            Some(&body),
            &[]
        )
        .is_err()
    );
}

#[test]
fn a_fixture_reads_the_refusal_of_the_cluster_validate_route_with_http_500_as_an_answer_with_no_codes()
 {
    let refusal = json!({"code": "ClusterProfilesNotSupported", "message": "not supported"});
    let (b, _) = fixture(json!({"_responses": {
        "POST /v1/spectroclusters/aws/validate":
            {"ok": false, "status": 500, "body": refusal},
        "POST /v1/clouds/aws/account/validate":
            {"ok": false, "status": 500, "body": refusal}}}));
    let body = json!({});
    let answer = b
        .palette(
            Method::Post,
            "/v1/spectroclusters/aws/validate",
            Some(&body),
            &[],
        )
        .unwrap();
    assert_eq!((answer.ok, answer.status), (false, 500));
    // the same body from another route is an error, as on a live run
    assert!(
        b.palette(
            Method::Post,
            "/v1/clouds/aws/account/validate",
            Some(&body),
            &[]
        )
        .is_err()
    );
}

/// What the property route of AWS answers for a key pair that the region lacks: HTTP 500 and this
/// code.
const KEY_PAIR_REFUSAL: &str =
    r#"{"code":"AwsKeyPairNotFound","message":"Aws KeyPair 'k' not found for region us-east-2"}"#;
const PROPERTIES: &str = "/v1/clouds/aws/properties/validate";

fn codes(names: &[&str]) -> Vec<String> {
    names.iter().map(ToString::to_string).collect()
}

/// A check that sends the property route and reports the code of a refusal.
fn properties_check(refusal_codes: &[&str]) -> Rule {
    rule(Check::Palette(PaletteCheck {
        method: Method::Post,
        path: PROPERTIES.into(),
        body: Some("{}".into()),
        each: None,
        refusal_codes: codes(refusal_codes),
        expr: r#"[select(.response.ok | not) | "refused: \(.response.body.code)"]"#.into(),
    }))
}

#[test]
fn a_5xx_with_a_code_that_the_check_names_is_its_answer() {
    let body = json!({});
    for status in [500u16, 503] {
        let api = strata::api::MemoryApi::new().failing(
            format!("POST {PROPERTIES}"),
            status,
            KEY_PAIR_REFUSAL,
        );
        let b = LiveBackend::new(Some(&api), false);
        let answer = b
            .palette(
                Method::Post,
                PROPERTIES,
                Some(&body),
                &codes(&["AwsKeyPairNotFound"]),
            )
            .unwrap();
        assert_eq!((answer.ok, answer.status), (false, status));
        assert_eq!(answer.body["code"], "AwsKeyPairNotFound");
        assert_eq!(api.calls(&format!("POST {PROPERTIES}")), 1);
    }
}

#[test]
fn a_5xx_with_another_code_or_with_no_code_is_an_error_also_when_the_check_names_a_code() {
    let body = json!({});
    for text in [
        r#"{"code":"InternalError","message":"boom"}"#,
        r#"{"message":"boom"}"#,
        r#"{"code":500}"#,
        "not json",
        "[]",
    ] {
        let api = strata::api::MemoryApi::new().failing(format!("POST {PROPERTIES}"), 500, text);
        let b = LiveBackend::new(Some(&api), false);
        let named = codes(&["AwsKeyPairNotFound"]);
        for refusal_codes in [named.as_slice(), &[]] {
            let err = b
                .palette(Method::Post, PROPERTIES, Some(&body), refusal_codes)
                .unwrap_err()
                .to_string();
            assert!(err.contains("HTTP 500"), "{text}: {err}");
        }
        assert_eq!(
            api.calls(&format!("POST {PROPERTIES}")),
            2,
            "{text}: an error is asked again"
        );
    }
}

#[test]
fn a_5xx_with_a_named_code_is_an_error_for_a_check_that_does_not_name_it() {
    let api =
        strata::api::MemoryApi::new().failing(format!("POST {PROPERTIES}"), 500, KEY_PAIR_REFUSAL);
    let b = LiveBackend::new(Some(&api), false);
    let body = json!({});
    assert!(
        b.palette(Method::Post, PROPERTIES, Some(&body), &[])
            .is_err(),
        "no code is named"
    );
    assert!(
        b.palette(
            Method::Post,
            PROPERTIES,
            Some(&body),
            &codes(&["AwsKeyPairNotFound"])
        )
        .is_ok()
    );
    // the answer is kept, and a check that names no code reads the 500 as an error
    let err = b
        .palette(Method::Post, PROPERTIES, Some(&body), &codes(&["Other"]))
        .unwrap_err();
    assert!(err.to_string().contains("HTTP 500"), "{err}");
    let world = b.post_json(PROPERTIES, &body).unwrap_err();
    assert!(
        matches!(&world, strata::Error::Api { status: 500, .. }),
        "{world}"
    );
    assert_eq!(
        api.calls(&format!("POST {PROPERTIES}")),
        2,
        "one request for the error, one for the answer"
    );
}

#[test]
fn a_key_a_rate_and_a_4xx_with_a_named_code_follow_the_rule_of_their_status() {
    let body = json!({});
    for (status, answer) in [(401u16, false), (403, false), (429, false), (404, true)] {
        let api = strata::api::MemoryApi::new().failing(
            format!("POST {PROPERTIES}"),
            status,
            KEY_PAIR_REFUSAL,
        );
        let b = LiveBackend::new(Some(&api), false);
        let got = b.palette(
            Method::Post,
            PROPERTIES,
            Some(&body),
            &codes(&["AwsKeyPairNotFound"]),
        );
        assert_eq!(got.is_ok(), answer, "HTTP {status}");
    }
}

#[test]
fn a_check_with_refusal_codes_fails_on_the_5xx_that_it_names_and_errors_on_another() {
    let jq = Jq::new();
    let world = json!({});
    let run = |refusal_codes: &[&str], text: &str| {
        let api = strata::api::MemoryApi::new().failing(format!("POST {PROPERTIES}"), 500, text);
        let b = LiveBackend::new(Some(&api), false);
        run_check(&properties_check(refusal_codes), &world, &b, &jq)
    };
    let named = run(&["AwsKeyPairNotFound"], KEY_PAIR_REFUSAL);
    assert_eq!(named.verdict, Verdict::Failed, "{:?}", named.messages);
    assert_eq!(named.messages, ["refused: AwsKeyPairNotFound"]);
    assert!(
        matches!(&named.ran, Ran::Request(call) if call.line() == format!("POST {PROPERTIES} -> HTTP 500")),
        "{:?}",
        named.ran
    );
    let other = run(&["AwsKeyPairNotFound"], r#"{"code":"InternalError"}"#);
    assert_eq!(other.verdict, Verdict::Error, "{:?}", other.messages);
    let unnamed = run(&[], KEY_PAIR_REFUSAL);
    assert_eq!(unnamed.verdict, Verdict::Error, "{:?}", unnamed.messages);
    assert!(
        unnamed.messages[0].contains("HTTP 500"),
        "{:?}",
        unnamed.messages
    );
}

#[test]
fn a_fixture_5xx_is_an_answer_only_when_the_check_names_its_code_as_a_live_run_does() {
    let jq = Jq::new();
    let doc = json!({"_responses": {format!("POST {PROPERTIES}"): {
        "ok": false, "status": 500,
        "body": {"code": "AwsKeyPairNotFound", "message": "no key pair"}}}});
    let (b, w) = fixture(doc);
    let named = run_check(&properties_check(&["AwsKeyPairNotFound"]), &w, &b, &jq);
    assert_eq!(named.verdict, Verdict::Failed, "{:?}", named.messages);
    assert_eq!(named.messages, ["refused: AwsKeyPairNotFound"]);
    for refusal_codes in [&[][..], &["Other"]] {
        let out = run_check(&properties_check(refusal_codes), &w, &b, &jq);
        assert_eq!(out.verdict, Verdict::Error, "{:?}", out.messages);
        assert!(out.messages[0].contains("HTTP 500"), "{:?}", out.messages);
    }
}

#[test]
fn a_fixture_reads_each_status_that_is_not_2xx_as_a_live_run_does() {
    let body = json!({"code": "AwsKeyPairNotFound", "message": "no key pair"});
    let named = codes(&["AwsKeyPairNotFound"]);
    // a 4xx that is no key or rate error is the answer. A 401, a 403, a 429, and a 5xx are
    // errors, also when the check names the code. A 5xx is an answer only with the code named
    for (status, answer, answer_with_code) in [
        (400u16, true, true),
        (404, true, true),
        (409, true, true),
        (422, true, true),
        (401, false, false),
        (403, false, false),
        (429, false, false),
        (500, false, true),
        (503, false, true),
    ] {
        let key = format!("POST {PROPERTIES}");
        let api = strata::api::MemoryApi::new().failing(&key, status, body.to_string());
        let live = LiveBackend::new(Some(&api), false);
        let (kept, _) = fixture(json!({"_responses": {
            key: {"ok": false, "status": status, "body": body}}}));
        for (refusal_codes, expected) in [(&[][..], answer), (named.as_slice(), answer_with_code)] {
            let sent = json!({});
            let asked = |backend: &dyn Backend| {
                backend
                    .palette(Method::Post, PROPERTIES, Some(&sent), refusal_codes)
                    .is_ok()
            };
            assert_eq!(
                asked(&live),
                expected,
                "live, HTTP {status}, {refusal_codes:?}"
            );
            assert_eq!(
                asked(&kept),
                expected,
                "fixture, HTTP {status}, {refusal_codes:?}"
            );
        }
    }
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
        refusal_codes: Vec::new(),
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
        also: Vec::new(),
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
            also: None,
            request: None,
        }
    );
    let missing = rule(Check::Exec(ExecCheck {
        argv: vec!["gcloud".into(), "services".into(), "list".into()],
        needs: vec!["gcloud".into()],
        timeout: None,
        palette: None,
        also: Vec::new(),
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
        also: Vec::new(),
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
            also: None,
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

/// An exec check that counts the policies of a role with one command, and reads the quota with a
/// second command. It uses the default quota 20 when the second command fails.
fn exec_with_a_second_command() -> Rule {
    rule(Check::Exec(ExecCheck {
        argv: vec![
            "aws".into(),
            "iam".into(),
            "list-attached-role-policies".into(),
        ],
        needs: vec!["aws".into()],
        timeout: None,
        palette: None,
        also: vec![
            "aws".into(),
            "service-quotas".into(),
            "{{ .region }}".into(),
        ],
        expr: r#"(if .also.exit == 0 then .also.stdout.Quota.Value else 20 end) as $quota
            | [ select(.exec.stdout.Count > $quota) | "\(.exec.stdout.Count) policies, and the quota is \($quota)" ]"#
            .into(),
    }))
}

const POLICIES: &str = "aws iam list-attached-role-policies";
const QUOTA: &str = "aws service-quotas us-east-2";

#[test]
fn an_exec_check_with_a_second_command_gives_the_program_both_outputs() {
    let jq = Jq::new();
    let (b, w) = fixture(json!({"region": "us-east-2", "_exec": {
        POLICIES: {"exit": 0, "stdout": {"Count": 25}, "stderr": ""},
        QUOTA: {"exit": 0, "stdout": {"Quota": {"Value": 30.0}}, "stderr": ""}}}));
    let out = run_check(&exec_with_a_second_command(), &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::Passed, "{:?}", out.messages);
    let (b, w) = fixture(json!({"region": "us-east-2", "_exec": {
        POLICIES: {"exit": 0, "stdout": {"Count": 35}, "stderr": ""},
        QUOTA: {"exit": 0, "stdout": {"Quota": {"Value": 30.0}}, "stderr": ""}}}));
    let out = run_check(&exec_with_a_second_command(), &w, &b, &jq);
    assert_eq!(out.messages, vec!["35 policies, and the quota is 30.0"]);
    // the report shows both commands as a person types them, and the exit code of the first
    assert_eq!(
        out.ran,
        Ran::Exec {
            line: POLICIES.into(),
            answer: Some("exit 0".into()),
            also: Some(QUOTA.into()),
            request: None,
        }
    );
    assert_eq!(
        out.ran.text().as_deref(),
        Some("aws iam list-attached-role-policies; aws service-quotas us-east-2")
    );
}

#[test]
fn an_exec_check_whose_second_command_fails_gives_the_program_the_exit_code() {
    let jq = Jq::new();
    let (b, w) = fixture(json!({"region": "us-east-2", "_exec": {
        POLICIES: {"exit": 0, "stdout": {"Count": 21}, "stderr": ""},
        QUOTA: {"exit": 254, "stdout": "", "stderr": "AccessDeniedException"}}}));
    let out = run_check(&exec_with_a_second_command(), &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::Failed, "{:?}", out.messages);
    assert_eq!(out.messages, vec!["21 policies, and the quota is 20"]);
}

#[test]
fn an_exec_check_with_a_second_command_needs_an_output_for_each_command() {
    let jq = Jq::new();
    // the fixture has no output for the second command: a command that runs is an error
    let (b, w) = fixture(json!({"region": "us-east-2", "_exec": {
        POLICIES: {"exit": 0, "stdout": {"Count": 1}, "stderr": ""}}}));
    let out = run_check(&exec_with_a_second_command(), &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::Error);
    assert!(out.messages[0].contains(QUOTA), "{:?}", out.messages);
}

#[test]
fn an_exec_check_with_a_second_command_and_no_tool_names_both_commands_to_run_by_hand() {
    let jq = Jq::new();
    let (b, w) = fixture(json!({"region": "us-east-2", "_tools": []}));
    let out = run_check(&exec_with_a_second_command(), &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::NotChecked);
    assert_eq!(
        out.messages,
        vec![
            "aws not on this machine; run by hand: aws iam list-attached-role-policies; aws service-quotas us-east-2"
        ]
    );
}
#[test]
fn an_exec_check_with_a_read_and_no_key_is_not_run_and_runs_no_command() {
    let jq = Jq::new();
    // the fixture has no output for the command: a command that runs is an error
    let (b, w) = fixture(json!({"region": "us-east-2", "_palette": false}));
    let out = run_check(&exec_with_a_read(), &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::NotRun);
    assert!(out.messages[0].contains("no key"), "{:?}", out.messages);
    assert_eq!(out.ran, Ran::Nothing);
}

#[test]
fn an_exec_check_with_a_read_and_no_tool_asks_palette_nothing() {
    let jq = Jq::new();
    // the fixture has no answer for the request: a request that goes out is an error
    let (b, w) = fixture(json!({"region": "us-east-2", "_tools": []}));
    let out = run_check(&exec_with_a_read(), &w, &b, &jq);
    assert_eq!(out.verdict, Verdict::NotChecked);
    assert_eq!(
        out.ran,
        Ran::Exec {
            line: "aws service-quotas us-east-2".into(),
            answer: None,
            also: None,
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

/// `path` with each `{{ jq }}` template of it replaced by `x`.
fn without_templates(path: &str) -> String {
    let mut out = String::new();
    let mut rest = path;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        match rest[start..].find("}}") {
            Some(end) => {
                out.push('x');
                rest = &rest[start + end + 2..];
            }
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

#[test]
fn each_rule_that_sends_the_cluster_create_to_the_validate_route_sends_it_where_the_library_names_it()
 {
    use strata::api::codes::ValidateRoute;

    let catalog = crate::catalog::builtin().unwrap();
    let mut create_routes = 0;
    for rule in catalog.rules() {
        let Some(Check::Palette(check)) = &rule.check else {
            continue;
        };
        let path = without_templates(&check.path);
        // a path of the create ends with `/validate`, and the library names it
        if path.starts_with("/v1/spectroclusters/") && path.ends_with("/validate") {
            assert_eq!(
                ValidateRoute::of(check.method, &path),
                Some(ValidateRoute::ClusterCreate),
                "{}: {}",
                rule.id,
                check.path
            );
            create_routes += 1;
        } else {
            // no other route of a rule is a validate route that the library reads as an answer
            assert_eq!(ValidateRoute::of(check.method, &path), None, "{}", rule.id);
        }
    }
    assert!(
        create_routes >= 4,
        "the catalog has {create_routes} rules that validate the create"
    );
}

#[test]
fn a_fixture_gives_the_outputs_that_it_answers_a_command_with() {
    let (b, _) = fixture(json!({"_exec": {
        "aws ec2 describe-y --output json": {"exit": 0, "stdout": {"y": 1}, "stderr": ""},
    }}));
    let outputs = b.outputs();
    assert_eq!(
        outputs.keys().map(String::as_str).collect::<Vec<_>>(),
        ["aws ec2 describe-y --output json"]
    );
    let argv: Vec<String> = ["aws", "ec2", "describe-y", "--output", "json"]
        .map(String::from)
        .into();
    let ran = b.exec(&argv, Duration::from_secs(1)).unwrap();
    assert_eq!(&ran, &outputs["aws ec2 describe-y --output json"]);
    // an output that lacks a field is no output: the backend drops the whole `_exec`
    let (b, _) = fixture(json!({"_exec": {"aws x": {"exit": 0, "stdout": {}}}}));
    assert!(b.outputs().is_empty());
}
