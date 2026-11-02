use super::*;
use strata::api::Method;

const GOOD: &str = r#"
section: compute
title: Compute and pools
rules:
  - id: gke-pool-multiple-3
    title: A GKE worker pool size is a multiple of 3
    engines: [gke]
    tier: static
    status: seen
    what: Palette refuses the create with InvalidParam, after a full apply.
    check:
      kind: static
      expr: '[.entity.spec.machinepoolconfig[] | select(.poolConfig.size % 3 != 0) | .poolConfig.name]'
    tests:
      - fault: fixtures/gke/pool-4.json
        clean: fixtures/gke/clean.json
"#;

#[test]
fn a_section_parses_and_names_its_rules() {
    let s = Catalog::parse_section(GOOD, Path::new("rules/compute.yaml")).unwrap();
    assert_eq!(s.rules[0].section, "compute");
    assert_eq!(s.rules[0].level, Level::Probe);
    assert_eq!(s.rules[0].severity, Severity::Fail);
    assert_eq!(
        s.rules[0].tests[0].fault.as_deref(),
        Some(Path::new("rules/fixtures/gke/pool-4.json"))
    );
    assert!(s.rules[0].applies_to("gke"));
    assert!(!s.rules[0].applies_to("eks"));
}

#[test]
fn a_rule_names_all_each_cloud_and_the_lab_as_its_engines() {
    let names: Vec<&str> = engines().collect();
    assert_eq!(names.first(), Some(&CLOUD_ALL));
    assert_eq!(names.last(), Some(&ENGINE_LAB));
    assert_eq!(names[1..names.len() - 1], clouds().collect::<Vec<_>>());
    assert_eq!(
        engine_list(),
        "all, aws, eks, gcp, gke, azure, aks, maas, edge, vsphere, lab"
    );
    assert!(names.iter().all(|e| is_engine(e)));
    assert!(!is_engine("openstack"));
}

#[test]
fn a_check_of_palette_reads_unless_it_names_a_method() {
    let check: Check = serde_yaml::from_str("kind: palette\npath: /v1/x\nexpr: 'true'").unwrap();
    assert!(matches!(
        check,
        Check::Palette(PaletteCheck {
            method: Method::Get,
            ..
        })
    ));
    let check: Check =
        serde_yaml::from_str("kind: palette\nmethod: POST\npath: /v1/x\nexpr: 'true'").unwrap();
    assert!(matches!(
        check,
        Check::Palette(PaletteCheck {
            method: Method::Post,
            ..
        })
    ));
    let err =
        serde_yaml::from_str::<Check>("kind: palette\nmethod: get\npath: /v1/x\nexpr: 'true'")
            .unwrap_err();
    assert!(err.to_string().contains("unknown variant"), "{err}");
}

#[test]
fn the_format_refuses_what_it_cannot_run() {
    let bad = |text: &str, needle: &str| {
        let err = Catalog::parse_section(text, Path::new("x.yaml")).unwrap_err();
        assert!(err.to_string().contains(needle), "{err}");
    };
    bad(
        &GOOD.replace("tier: static", "tier: palette"),
        "the check is of kind static",
    );
    bad(
        &GOOD.replace("engines: [gke]", "engines: [gkee]"),
        "engine 'gkee'",
    );
    bad(
        &GOOD.replace("status: seen", "status: seen\n    level: preflight"),
        "names the runs of its promotion",
    );
    bad(
        &GOOD.replace("status: seen", "status: seen\n    releases: banana"),
        "releases 'banana'",
    );
    bad(
        &GOOD.replace("id: gke-pool-multiple-3", "id: Gke_pool"),
        "not kebab-case",
    );
}

#[test]
fn a_rule_file_with_a_program_that_nests_too_deep_is_refused_with_the_catalog_kind() {
    let deep = format!("{}true{}", "(".repeat(5000), ")".repeat(5000));
    let text = GOOD.replace(
        "expr: '[.entity.spec.machinepoolconfig[] | select(.poolConfig.size % 3 != 0) | .poolConfig.name]'",
        &format!("expr: '{deep}'"),
    );
    let err = Catalog::parse_section(&text, Path::new("x.yaml")).unwrap_err();
    assert_eq!(err.kind(), "catalog");
    assert!(
        err.to_string()
            .contains("`expr`: the program nests more than 64 levels"),
        "{err}"
    );
}

#[test]
fn a_check_of_palette_with_each_reads_item_and_a_check_without_it_refuses_item() {
    let palette = |extra: &str, path: &str| {
        GOOD.replace("tier: static", "tier: palette").replace(
                "check:\n      kind: static\n      expr: '[.entity.spec.machinepoolconfig[] | select(.poolConfig.size % 3 != 0) | .poolConfig.name]'",
                &format!("check:\n      kind: palette\n      path: {path}\n{extra}      expr: 'true'"),
            )
    };
    let each = "      each: '[.entity.spec.profiles[]]'\n";
    let section = Catalog::parse_section(
        &palette(each, "/v1/clusterprofiles/{{ $item.uid }}"),
        Path::new("x.yaml"),
    )
    .unwrap();
    assert!(matches!(
        &section.rules[0].check,
        Some(Check::Palette(PaletteCheck { each: Some(e), .. })) if e == "[.entity.spec.profiles[]]"
    ));
    let bad = |text: String, needle: &str| {
        let err = Catalog::parse_section(&text, Path::new("x.yaml")).unwrap_err();
        assert!(err.to_string().contains(needle), "{err}");
    };
    bad(palette("      each: ' '\n", "/v1/x"), "`each` is empty");
    bad(
        palette("", "/v1/clusterprofiles/{{ $item.uid }}"),
        "`path`: the program does not compile: undefined variable $item",
    );
}

#[test]
fn a_check_of_palette_names_the_codes_of_a_5xx_that_it_reads_as_an_answer() {
    let palette = |extra: &str| {
        GOOD.replace("tier: static", "tier: palette").replace(
                "check:\n      kind: static\n      expr: '[.entity.spec.machinepoolconfig[] | select(.poolConfig.size % 3 != 0) | .poolConfig.name]'",
                &format!("check:\n      kind: palette\n      path: /v1/x\n{extra}      expr: 'true'"),
            )
    };
    let refusal_codes = |text: &str| match Catalog::parse_section(text, Path::new("x.yaml"))
        .unwrap()
        .rules[0]
        .check
        .clone()
    {
        Some(Check::Palette(PaletteCheck { refusal_codes, .. })) => refusal_codes,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        refusal_codes(&palette(
            "      refusal_codes: [AwsKeyPairNotFound, FeatureNotSupported]\n"
        )),
        ["AwsKeyPairNotFound", "FeatureNotSupported"]
    );
    assert!(
        refusal_codes(&palette("")).is_empty(),
        "each 5xx is an error"
    );
    let err = Catalog::parse_section(
        &palette("      refusal_codes: [AwsKeyPairNotFound, ' ']\n"),
        Path::new("x.yaml"),
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("`refusal_codes` has an empty code"),
        "{err}"
    );
}

#[test]
fn a_check_of_palette_cannot_name_a_code_that_the_library_does_not_list_as_a_refusal() {
    let palette = |codes: &str| {
        GOOD.replace("tier: static", "tier: palette").replace(
                "check:\n      kind: static\n      expr: '[.entity.spec.machinepoolconfig[] | select(.poolConfig.size % 3 != 0) | .poolConfig.name]'",
                &format!("check:\n      kind: palette\n      path: /v1/x\n      refusal_codes: [{codes}]\n      expr: 'true'"),
            )
    };
    // each code that the library lists is right: a refusal of another route, and a check of a
    // validate route
    let listed =
        "AwsKeyPairNotFound, FeatureNotSupported, OperationForbidden, ClusterProfilesNotSupported";
    assert!(Catalog::parse_section(&palette(listed), Path::new("x.yaml")).is_ok());
    // a 5xx with another code is sent again, so the check would read it only after the retries
    let err = Catalog::parse_section(
        &palette("AwsKeyPairNotFound, InternalError"),
        Path::new("x.yaml"),
    )
    .unwrap_err()
    .to_string();
    assert!(
        err.contains(
            "`refusal_codes` names InternalError, and the library of strata does not list it"
        ),
        "{err}"
    );
}

#[test]
fn a_program_that_runs_with_no_item_cannot_read_item_in_any_place() {
    let each = "      each: '[.entity.spec.profiles[]]'\n";
    let palette = |extra: &str, expr: &str| {
        GOOD.replace("tier: static", "tier: palette").replace(
                "check:\n      kind: static\n      expr: '[.entity.spec.machinepoolconfig[] | select(.poolConfig.size % 3 != 0) | .poolConfig.name]'",
                &format!("check:\n      kind: palette\n      path: /v1/clusterprofiles/{{{{ $item.uid }}}}\n{each}{extra}      expr: '{expr}'"),
            )
    };
    let bad = |text: String, needle: &str| {
        let err = Catalog::parse_section(&text, Path::new("x.yaml")).unwrap_err();
        assert!(err.to_string().contains(needle), "{err}");
    };
    // the program over the responses runs once over the world, with no item
    bad(
        palette("", "[$item]"),
        "`expr`: the program does not compile: undefined variable $item",
    );
    bad(
        palette("", "true").replace(
            "      each: '[.entity.spec.profiles[]]'",
            "      each: '[$item]'",
        ),
        "`each`: the program does not compile: undefined variable $item",
    );
    bad(
        palette("", "true").replace("status: seen", "status: seen\n    when: '$item != null'"),
        "`when`: the program does not compile: undefined variable $item",
    );
    bad(
        palette("", "true").replace(
            "    tests:",
            "    reproduce:\n      - \"echo {{ $item }}\"\n    tests:",
        ),
        "`reproduce`: the program does not compile: undefined variable $item",
    );
    // a variable with a name that begins as $item is not $item
    bad(
        palette("", "true").replace("{{ $item.uid }}", "{{ $items.uid }}"),
        "`path`: the program does not compile: undefined variable $items",
    );
    // a program that does not parse is refused in the same way
    bad(palette("", "[.a"), "`expr`: the program does not parse");
}

#[test]
fn a_post_with_each_needs_a_body_that_reads_item() {
    let post = |body: &str| {
        GOOD.replace("tier: static", "tier: palette").replace(
                "check:\n      kind: static\n      expr: '[.entity.spec.machinepoolconfig[] | select(.poolConfig.size % 3 != 0) | .poolConfig.name]'",
                &format!("check:\n      kind: palette\n      method: POST\n      path: /v1/x\n      each: '[.entity.spec.profiles[]]'\n{body}      expr: 'true'"),
            )
    };
    let err = Catalog::parse_section(&post(""), Path::new("x.yaml")).unwrap_err();
    assert!(
        err.to_string()
            .contains("a POST with `each` needs a `body`"),
        "{err}"
    );
    let ok = post("      body: '{profile: $item}'\n");
    assert!(Catalog::parse_section(&ok, Path::new("x.yaml")).is_ok());
    // a GET has no body to send
    let get = ok
        .replace("method: POST", "method: GET")
        .replace("      body: '{profile: $item}'\n", "");
    assert!(Catalog::parse_section(&get, Path::new("x.yaml")).is_ok());
}

#[test]
fn an_evidence_entry_names_a_kind_of_the_set_and_the_loader_refuses_another() {
    let with = |kind: &str| {
        GOOD.replace(
            "status: seen",
            &format!("status: seen\n    evidence:\n      - kind: {kind}\n        ref: x"),
        )
    };
    for kind in EvidenceKind::ALL {
        let section = Catalog::parse_section(&with(kind.as_str()), Path::new("x.yaml"));
        assert_eq!(section.unwrap().rules[0].evidence[0].kind, *kind);
    }
    let err = Catalog::parse_section(&with("blog"), Path::new("x.yaml")).unwrap_err();
    assert!(err.to_string().contains("unknown variant `blog`"), "{err}");
}

#[test]
fn a_catalog_refuses_one_id_two_times() {
    let mut c = Catalog::default();
    let s = Catalog::parse_section(GOOD, Path::new("a.yaml")).unwrap();
    c.push(s.clone(), Path::new("a.yaml")).unwrap();
    let err = c.push(s, Path::new("probes.yaml")).unwrap_err();
    assert!(
        err.to_string().contains("is in the catalog already"),
        "{err}"
    );
}

#[test]
fn a_pre_release_is_below_its_version_in_a_release_range() {
    // the order is that of strata::reference::version::compare
    assert!(!release_in_range("1.0.1-g0123456789ab", ">=1.0.1"));
    assert!(release_in_range("1.0.1-g0123456789ab", "<1.0.1"));
    assert!(release_in_range("1.0.1", ">=1.0.1"));
    assert!(release_in_range("1.0.1+2", "=1.0.1"));
    assert!(release_in_range("1.0.1-rc.2", ">1.0.1-rc.1 <1.0.1"));
    assert!(!release_in_range("1.0.1-rc.11", "<1.0.1-rc.2"));
    // a bound that names fewer than three numbers is a release line: only its numbers count
    assert!(release_in_range("4.10.0-rc.1", "=4.10"));
    assert!(!release_in_range("4.11.0-rc.1", "<4.11"));
    // a release that does not parse is in each range
    assert!(release_in_range("next", ">=1.0.1"));

    let section = GOOD.replace("status: seen", "status: seen\n    releases: '>=1.0.1'");
    let rule = &Catalog::parse_section(&section, Path::new("x.yaml"))
        .unwrap()
        .rules[0];
    assert!(rule.applies_to_release(Some("1.0.1")));
    assert!(!rule.applies_to_release(Some("1.0.1-g0123456789ab")));
    assert!(rule.applies_to_release(None));
}

#[test]
fn a_release_that_is_not_a_version_is_refused_and_one_that_is_a_version_is_not() {
    for release in ["4.10", "4.10.12", "v4.10.0-rc.1", "4.10.3+build.5", "4"] {
        assert!(require_release(release).is_ok(), "{release}");
    }
    for release in ["4.11.x", "", "next", "latest", "4.10.", "4.x", ".5"] {
        let err = require_release(release).unwrap_err();
        assert!(matches!(err, Error::Usage { .. }), "{release}: {err}");
        assert_eq!(
            err.to_string(),
            format!("--release {release}: give a release such as 4.10 or 4.10.12")
        );
    }
}

#[test]
fn release_ranges() {
    assert!(release_in_range("4.10.3", "<4.11"));
    assert!(!release_in_range("4.11.0", "<4.11"));
    assert!(release_in_range("4.10", ">=4.9 <4.11"));
    assert!(!release_in_range("4.8", ">=4.9 <4.11"));
    assert!(release_in_range("4.10.3", "=4.10"));
    assert!(release_in_range("v4.10.0-rc1", ">=4.10"));
    // an unknown release applies to every rule
    let r = Catalog::parse_section(GOOD, Path::new("x.yaml")).unwrap();
    assert!(r.rules[0].applies_to_release(None));
}

/// The rule `GOOD` with a check of kind `exec` that runs two commands.
fn exec_with_also(also: &str) -> String {
    GOOD.replace("tier: static", "tier: cloud").replace(
        "check:\n      kind: static\n      expr: '[.entity.spec.machinepoolconfig[] | select(.poolConfig.size % 3 != 0) | .poolConfig.name]'",
        &format!(
            "check:\n      kind: exec\n      argv: [aws, iam, list-attached-role-policies]\n      also: {also}\n      expr: 'true'"
        ),
    )
}

#[test]
fn an_exec_check_takes_a_second_command_and_the_loader_checks_its_templates() {
    let section = Catalog::parse_section(
        &exec_with_also("[aws, service-quotas, get-service-quota, '{{ .region }}']"),
        Path::new("x.yaml"),
    )
    .unwrap();
    let Some(Check::Exec(exec)) = &section.rules[0].check else {
        panic!("{:?}", section.rules[0].check);
    };
    assert_eq!(exec.also[0], "aws");
    assert_eq!(exec.also.len(), 4);
    // a check with no `also` runs one command
    let one = exec_with_also("[]");
    let section = Catalog::parse_section(&one, Path::new("x.yaml")).unwrap();
    let Some(Check::Exec(exec)) = &section.rules[0].check else {
        panic!("{:?}", section.rules[0].check);
    };
    assert!(exec.also.is_empty());
    let err = Catalog::parse_section(
        &exec_with_also("[aws, '{{ .region | nope( }}']"),
        Path::new("x.yaml"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("`also`"), "{err}");
}
