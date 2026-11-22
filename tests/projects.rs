//! The project of each command that reads Palette, and of each side of `compare` and
//! `same-release`: a tenant with one project for each environment (#486). The uid of a project
//! name belongs to the address and the key of each client (#875).

use std::collections::BTreeMap;

use serde_json::{Value, json};
use strata::Error;
use strata::api::codes::Denial;
use strata::api::{Method, project_uid_named};
use strata::config::Config;
use strata::reference::{FILTER_PREFIX, PACK_PREFIX, PROJECT_PREFIX, TAG_PREFIX};
use strata::term::progress::Progress;

use crate::common::{
    PROJECT_LISTING, Run, Stand, UID_A, UID_B, against_closed_port, command_names, error_body,
    manifests_route, no_manifests, no_variables, profile_document, profile_route,
    project_denied_message, project_listing, serve_tenants, strata_command, template,
    variables_route,
};

/// The uids of the projects `dev` and `prod` of the stand-in.
const DEV: &str = "p-dev";
const PROD: &str = "p-prod";
/// The project of the environment of each run. A flag replaces it, so no request names it.
const ENV_PROJECT: &str = "p-env";

/// A tenant with the projects `dev` and `prod`, and `aws-stack@1.0.0` with one content as
/// `UID_A` and as `UID_B`. The stand-in gives each route to each project, so only the header
/// of a request shows the project that strata asked.
fn tenant() -> BTreeMap<String, Value> {
    let packs = json!([{"name": "cert-manager", "layer": "addon", "type": "spectro",
        "tag": "1.14.0", "version": "1.14.0", "packUid": "pk1", "registryUid": "r1",
        "digest": "sha256:0123", "values": "installCRDs: true"}]);
    let mut routes = BTreeMap::new();
    routes.insert(
        PROJECT_LISTING.to_string(),
        project_listing(&[("dev", DEV), ("prod", PROD)]),
    );
    for uid in [UID_A, UID_B] {
        let published = template("aws", "cluster", packs.clone());
        let document = profile_document(uid, "aws-stack", "1.0.0", published, None);
        routes.insert(profile_route(uid), document);
        routes.insert(manifests_route(uid), no_manifests());
        routes.insert(variables_route(uid), no_variables());
    }
    routes
}

/// The stand-in of [`tenant`]. Each run has [`ENV_PROJECT`] in `PALETTE_PROJECT_UID`.
fn tenant_stand() -> Stand {
    Stand::new(tenant(), ENV_PROJECT)
}

/// A record of `UID_A` at tenant scope, in the folder of the home of `stand`: a record that names
/// no project.
fn tenant_record(stand: &Stand) -> String {
    let r = stand.run(&["snapshot", UID_A, "--project", "", "-o", "tenant.json"]);
    assert_eq!(r.code, 0, "{}", r.err);
    "tenant.json".to_string()
}

/// Each command that reads Palette, with the arguments that it needs beside a project. `record`
/// is a record that names no project.
fn commands(record: &str) -> Vec<Vec<&str>> {
    vec![
        vec!["behind", record, "--no-impact"],
        vec!["candidate", UID_A],
        vec!["compare", UID_A, UID_B],
        vec!["consumers", UID_A, "--list-only"],
        vec!["list"],
        vec!["packs", "cert-manager"],
        vec!["same-release", UID_A, UID_B],
        vec!["snapshot", UID_A],
        vec!["verify", record],
    ]
}

/// The commands that read no Palette, so they take no project and [`commands`] does not hold them.
///
/// `preflight` runs `strata-preflight`, which reads the project from the environment. clap adds
/// `help`. The unit tests of `src/cli/args.rs` keep the same list, without `help`.
const READS_NO_PALETTE: [&str; 4] = ["help", "preflight", "report", "schema"];

#[test]
fn each_command_of_strata_is_in_commands_or_is_a_command_that_reads_no_palette() {
    let mut listed: Vec<&str> = commands("r.json")
        .iter()
        .map(|args| args[0])
        .chain(READS_NO_PALETTE)
        .collect();
    listed.sort_unstable();
    let mut names = command_names();
    names.sort_unstable();
    assert_eq!(
        listed, names,
        "put a command that reads Palette in `commands`, and the others in `READS_NO_PALETTE`"
    );
}

#[test]
fn each_command_that_reads_palette_reads_the_project_of_project_and_not_of_the_environment() {
    let stand = tenant_stand();
    let record = tenant_record(&stand);
    for args in commands(&record) {
        stand.run(&[&args[..], &["--project", DEV]].concat());
        let asked = stand.asked();
        assert_eq!(
            asked.first().and_then(|(_, p)| p.as_deref()),
            Some(DEV),
            "{args:?}: {asked:?}"
        );
        // a read of the tenant, such as the second reading of `consumers`, names no project
        assert!(
            asked.iter().all(|(_, p)| p.as_deref() != Some(ENV_PROJECT)),
            "{args:?}: {asked:?}"
        );
    }
}

#[test]
fn each_command_that_reads_palette_reads_the_tenant_scope_with_an_empty_project() {
    let stand = tenant_stand();
    let record = tenant_record(&stand);
    for args in commands(&record) {
        stand.run(&[&args[..], &["--project", ""]].concat());
        let asked = stand.asked();
        assert!(!asked.is_empty(), "{args:?}");
        assert!(
            asked.iter().all(|(_, p)| p.is_none()),
            "{args:?}: no request names a project: {asked:?}"
        );
    }
}

#[test]
fn each_command_that_reads_palette_finds_the_uid_of_project_name_and_reads_that_project() {
    let stand = tenant_stand();
    let record = tenant_record(&stand);
    for args in commands(&record) {
        stand.run(&[&args[..], &["--project-name", "prod"]].concat());
        let asked = stand.asked();
        assert_eq!(
            asked.first(),
            Some(&(PROJECT_LISTING.to_string(), None)),
            "{args:?}: the lookup of the name comes first, at tenant scope: {asked:?}"
        );
        assert_eq!(
            asked.get(1).and_then(|(_, p)| p.as_deref()),
            Some(PROD),
            "{args:?}: {asked:?}"
        );
    }
}

/// The name of the one project of [`lab_tenant`].
const LAB_PROJECT: &str = "lab";

/// The routes of a tenant with one project, [`LAB_PROJECT`], whose uid is `uid`.
fn lab_tenant(uid: &str) -> BTreeMap<String, Value> {
    BTreeMap::from([(
        PROJECT_LISTING.to_string(),
        project_listing(&[(LAB_PROJECT, uid)]),
    )])
}

#[test]
fn a_project_name_gives_the_uid_in_the_tenant_of_the_address_and_the_key_of_each_client() {
    // one address with two tenants: each key reads the projects of its own tenant
    let (first, second) = ("first-key", "second-key");
    let (first_uid, second_uid, other_uid) = ("p-first", "p-second", "p-other");
    let shared = serve_tenants(BTreeMap::from([
        (first.to_string(), lab_tenant(first_uid)),
        (second.to_string(), lab_tenant(second_uid)),
    ]));
    // another address, where the first key has a tenant too
    let other = serve_tenants(BTreeMap::from([(first.to_string(), lab_tenant(other_uid))]));
    let uid = |address: &str, key: &str| {
        project_uid_named(&Config::new(address, key), LAB_PROJECT, Progress::silent())
            .unwrap_or_else(|e| panic!("{key} at {address}: {e}"))
    };
    assert_eq!(uid(&shared, first), first_uid);
    assert_eq!(uid(&shared, second), second_uid, "another key");
    assert_eq!(uid(&other, first), other_uid, "another address");
    assert_eq!(uid(&shared, first), first_uid, "the first client again");
}

/// The project of each request for the routes of the profile `uid`.
fn projects_of(asked: &[(String, Option<String>)], uid: &str) -> Vec<Option<String>> {
    asked
        .iter()
        .filter(|(path, _)| path.contains(uid))
        .map(|(_, p)| p.clone())
        .collect()
}

#[test]
fn compare_reads_each_side_in_its_own_project_and_names_each_project() {
    let stand = tenant_stand();
    let sides = ["--project-name-a", "dev", "--project-name-b", "prod"];
    let r = stand.run(&[&["compare", UID_A, UID_B, "--json"][..], &sides].concat());
    assert_eq!(r.code, 0, "the two copies are one release: {}", r.err);
    let asked = stand.asked();
    assert_eq!(projects_of(&asked, UID_A), vec![Some(DEV.to_string()); 3]);
    assert_eq!(projects_of(&asked, UID_B), vec![Some(PROD.to_string()); 3]);
    let d: Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(
        (&d["a"]["projectUid"], &d["b"]["projectUid"]),
        (&json!(DEV), &json!(PROD))
    );

    let r = stand.run(&[&["compare", UID_A, UID_B][..], &sides].concat());
    assert!(
        r.out.contains("\nproject    p-dev -> p-prod\n"),
        "{}",
        r.out
    );
    let r = stand.run(
        &[
            &["compare", UID_A, UID_B, "--format", "markdown"][..],
            &sides,
        ]
        .concat(),
    );
    assert!(
        r.out.contains("\n| project | p-dev | p-prod |\n"),
        "{}",
        r.out
    );

    // the flag of side B, and --project for side A
    stand.run(&[
        "compare",
        UID_A,
        UID_B,
        "--project",
        DEV,
        "--project-b",
        PROD,
    ]);
    let asked = stand.asked();
    assert_eq!(projects_of(&asked, UID_A)[0].as_deref(), Some(DEV));
    assert_eq!(projects_of(&asked, UID_B)[0].as_deref(), Some(PROD));

    // one project for both sides: no line names it
    let r = stand.run(&["compare", UID_A, UID_B, "--project", DEV]);
    assert!(!r.out.contains("\nproject "), "{}", r.out);
}

#[test]
fn same_release_reads_each_side_in_its_own_project_and_names_each_project() {
    let stand = tenant_stand();
    let sides = ["--project-a", DEV, "--project-b", PROD];
    let r = stand.run(&[&["same-release", UID_A, UID_B, "--json"][..], &sides].concat());
    assert_eq!(r.code, 0, "{}", r.err);
    let asked = stand.asked();
    assert_eq!(projects_of(&asked, UID_A), vec![Some(DEV.to_string()); 3]);
    assert_eq!(projects_of(&asked, UID_B), vec![Some(PROD.to_string()); 3]);
    let c: Value = serde_json::from_str(&r.out).unwrap();
    let projects: Vec<&Value> = (0..2).map(|i| &c["records"][i]["projectUid"]).collect();
    assert_eq!(projects, [&json!(DEV), &json!(PROD)]);
    assert_eq!(c["pairs"][0]["status"], "identical");

    let r = stand.run(&[&["same-release", UID_A, UID_B][..], &sides].concat());
    let lines: Vec<&str> = r.out.lines().collect();
    assert_eq!(
        lines.get(1).map(|l| l.trim()),
        Some("project p-dev -> p-prod"),
        "{}",
        r.out
    );
    let r = stand.run(
        &[
            &["same-release", UID_A, UID_B, "--format", "markdown"][..],
            &sides,
        ]
        .concat(),
    );
    // the markdown writes each `>` as an entity, and GitHub shows `->`
    assert!(
        r.out.contains("<br>project p-dev -&gt; p-prod |"),
        "{}",
        r.out
    );
}

#[test]
fn a_record_file_on_each_side_sends_no_request_for_the_name_of_a_project() {
    let stand = tenant_stand();
    for (project, file) in [(DEV, "dev.json"), (PROD, "prod.json")] {
        let r = stand.run(&["snapshot", UID_A, "--project", project, "-o", file]);
        assert_eq!(r.code, 0, "{}", r.err);
    }
    let file = |name: &str| stand.home.path().join(name).display().to_string();
    let (dev, prod) = (file("dev.json"), file("prod.json"));
    for command in ["compare", "same-release"] {
        let args = [
            command,
            &dev,
            &prod,
            "--project-name-a",
            "dev",
            "--project-name",
            "prod",
        ];
        let r = against_closed_port(strata_command(), &args, &[], true);
        assert_eq!(r.code, 0, "{command}: two files and no request: {}", r.err);
        assert!(r.out.contains("p-dev -> p-prod"), "{command}: {}", r.out);
    }
}

#[test]
fn same_release_refuses_the_flag_of_one_side_for_three_refs_before_any_request() {
    let stand = tenant_stand();
    for flag in ["--project-a", "--project-name-b"] {
        let r = stand.run(&["same-release", UID_A, UID_B, UID_A, flag, "dev", "--json"]);
        assert_eq!(r.code, 2, "{flag}");
        let e: Value = serde_json::from_str(r.err.trim()).unwrap();
        assert_eq!(e["error"]["kind"], "usage", "{flag}: {e}");
        assert!(stand.asked().is_empty(), "{flag}: {:?}", stand.asked());
    }
    // the flag of both sides takes three refs
    let r = stand.run(&["same-release", UID_A, UID_B, UID_A, "--project", DEV]);
    assert_eq!(r.code, 0, "{}", r.err);
}

/// `project:<project>/<reference>`: the ref `reference`, read in the project `project`.
fn in_project(project: &str, reference: &str) -> String {
    format!("{PROJECT_PREFIX}{project}/{reference}")
}

/// Each command that takes a ref in a project, with the refs `one` and `two`. A command that takes
/// one ref gets `one`.
fn ref_commands<'a>(one: &'a str, two: &'a str) -> Vec<Vec<&'a str>> {
    vec![
        vec!["snapshot", one],
        vec!["compare", one, two],
        vec!["same-release", one, two],
        vec!["consumers", one, "--list-only"],
    ]
}

/// The error document that a run with `--json` wrote to standard error.
fn error_of(run: &Run) -> Value {
    let e: Value = serde_json::from_str(run.err.trim())
        .unwrap_or_else(|_| panic!("no error document: {}", run.err));
    e["error"].clone()
}

#[test]
fn each_command_that_takes_a_ref_in_a_project_reads_the_ref_in_that_project() {
    let stand = tenant_stand();
    let (a, b) = (in_project("prod", UID_A), in_project("prod", UID_B));
    for args in ref_commands(&a, &b) {
        let r = stand.run(&args);
        assert!(r.code < 2, "{args:?}: {}", r.err);
        let asked = stand.asked();
        assert_eq!(
            asked.first(),
            Some(&(PROJECT_LISTING.to_string(), None)),
            "{args:?}: the lookup of the project comes first, at tenant scope: {asked:?}"
        );
        assert_eq!(
            projects_of(&asked, UID_A).first(),
            Some(&Some(PROD.to_string())),
            "{args:?}: {asked:?}"
        );
        assert!(
            asked.iter().all(|(_, p)| p.as_deref() != Some(ENV_PROJECT)),
            "{args:?}: no request reads the configured project: {asked:?}"
        );
    }
}

#[test]
fn a_ref_with_no_project_reads_in_the_configured_project_and_a_ref_in_a_project_has_priority() {
    let stand = tenant_stand();
    let a = in_project("dev", UID_A);
    for command in ["compare", "same-release"] {
        let r = stand.run(&[command, &a, UID_B]);
        assert_eq!(r.code, 0, "{command}: {}", r.err);
        let asked = stand.asked();
        assert_eq!(projects_of(&asked, UID_A), vec![Some(DEV.to_string()); 3]);
        assert_eq!(
            projects_of(&asked, UID_B),
            vec![Some(ENV_PROJECT.to_string()); 3],
            "{command}: the side with no project reads the configured project"
        );
        // the project of the ref has priority over the flags of its side
        stand.run(&[command, &a, UID_B, "--project-a", PROD, "--project", PROD]);
        let asked = stand.asked();
        assert_eq!(projects_of(&asked, UID_A)[0].as_deref(), Some(DEV));
        assert_eq!(projects_of(&asked, UID_B)[0].as_deref(), Some(PROD));
    }
}

#[test]
fn the_project_of_a_ref_is_a_uid_or_a_name_and_the_record_names_its_uid_and_the_ref() {
    let stand = tenant_stand();
    for project in [PROD, "prod"] {
        let reference = in_project(project, UID_A);
        let r = stand.run(&["snapshot", &reference]);
        assert_eq!(r.code, 0, "{project}: {}", r.err);
        let asked = stand.asked();
        let listings = asked.iter().filter(|(p, _)| p == PROJECT_LISTING).count();
        assert_eq!(listings, 1, "{project}: one lookup: {asked:?}");
        assert_eq!(projects_of(&asked, UID_A), vec![Some(PROD.to_string()); 3]);
        let record: Value = serde_json::from_str(&r.out).unwrap();
        assert_eq!(record["source"]["projectUid"], PROD, "{project}");
        assert_eq!(record["source"]["ref"], reference.as_str(), "{project}");
    }
}

#[test]
fn a_ref_in_a_project_that_no_visible_project_has_is_project_not_found_with_the_projects() {
    let stand = tenant_stand();
    let missing = in_project("staging", UID_A);
    let not_found = Error::ProjectNotFound {
        name: "staging".into(),
        available: vec!["dev".into(), "prod".into()],
    };
    for args in ref_commands(&missing, UID_B) {
        let r = stand.run(&[&args[..], &["--json"]].concat());
        assert_eq!(r.code, 2, "{args:?}");
        assert_eq!(error_of(&r), not_found.to_json(), "{args:?}");
        assert_eq!(
            stand.asked(),
            [(PROJECT_LISTING.to_string(), None)],
            "{args:?}: the lookup is the one request"
        );
    }
}

#[test]
fn a_ref_in_a_project_refuses_a_file_a_pack_a_tag_a_filter_and_a_second_project_before_any_request()
{
    let stand = tenant_stand();
    let refused = [
        ("record.json".to_string(), "a file belongs to no project"),
        (
            format!("{PACK_PREFIX}cert-manager@1.14.0"),
            "a pack belongs to no project",
        ),
        (
            format!("{TAG_PREFIX}env=prod"),
            "already search each project",
        ),
        (
            format!("{FILTER_PREFIX}production"),
            "already search each project",
        ),
        (in_project("prod", UID_A), "a ref names one project"),
    ];
    for (inner, why) in refused {
        let reference = in_project("dev", &inner);
        for args in ref_commands(&reference, UID_B) {
            let r = stand.run(&[&args[..], &["--json"]].concat());
            assert_eq!(r.code, 2, "{args:?}");
            let e = error_of(&r);
            let message = e["message"].as_str().unwrap();
            let usage = Error::Usage {
                message: message.into(),
            };
            assert_eq!(e, usage.to_json(), "{args:?}");
            assert!(message.contains(why), "{args:?}: {message}");
            assert!(message.contains(&reference), "{args:?}: {message}");
            assert!(stand.asked().is_empty(), "{args:?}: {:?}", stand.asked());
        }
    }
}

#[test]
fn a_ref_in_a_project_that_refuses_the_key_gives_an_error_that_names_the_project() {
    let denied = error_body(Denial::Project.as_str(), &project_denied_message(PROD));
    let stand = Stand::denying(tenant(), ENV_PROJECT, PROD, 500, denied);
    // the kind of the answer of Palette, and of the error that names the project of the ref
    let answer = Error::Api {
        method: Method::Get,
        path: String::new(),
        status: 500,
        body: String::new(),
        retry_after: None,
        key_proved: true,
    };
    let api = answer.kind();
    let refused = Error::ProjectDenied {
        project: "prod".into(),
        source: Box::new(answer),
    };
    let (a, b) = (in_project("dev", UID_A), in_project("prod", UID_B));
    let commands = [
        vec!["snapshot", b.as_str()],
        vec!["compare", &a, &b],
        vec!["same-release", &a, &b],
        vec!["consumers", &b, "--list-only"],
    ];
    for args in commands {
        let r = stand.run(&[&args[..], &["--json"]].concat());
        assert_eq!(r.code, 2, "{args:?}");
        let e = error_of(&r);
        assert_eq!(e["kind"], refused.kind(), "{args:?}: {e}");
        assert_eq!(e["project"], "prod", "{args:?}");
        assert_eq!(e["error"]["kind"], api, "{args:?}");
        assert_eq!(e["error"]["status"], 500, "{args:?}");
        let message = e["message"].as_str().unwrap();
        assert!(message.contains("project 'prod'"), "{args:?}: {message}");
    }
    // a ref with no project gets the answer of Palette as it is
    let r = stand.run(&["snapshot", UID_B, "--project", PROD, "--json"]);
    assert_eq!(error_of(&r)["kind"], api);
}
