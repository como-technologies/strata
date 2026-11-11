//! The project of each command that reads Palette, and of each side of `compare` and
//! `same-release`: a tenant with one project for each environment (#486).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::common::{
    PROJECT_LISTING, Request, Run, UID_A, UID_B, against_closed_port, isolate_palette,
    manifests_route, no_manifests, no_variables, profile_document, profile_route, serve_recorded,
    strata_command, template, variables_route,
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
    let project = |name: &str, uid: &str| json!({"metadata": {"name": name, "uid": uid}});
    let packs = json!([{"name": "cert-manager", "layer": "addon", "type": "spectro",
        "tag": "1.14.0", "version": "1.14.0", "packUid": "pk1", "registryUid": "r1",
        "digest": "sha256:0123", "values": "installCRDs: true"}]);
    let mut routes = BTreeMap::new();
    routes.insert(
        PROJECT_LISTING.to_string(),
        json!({"items": [project("dev", DEV), project("prod", PROD)]}),
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

/// The stand-in of [`tenant`], and the requests that it takes.
struct Stand {
    api: String,
    requests: Arc<Mutex<Vec<Request>>>,
    home: tempfile::TempDir,
}

impl Stand {
    fn new() -> Stand {
        let (api, requests) = serve_recorded(tenant());
        Stand {
            api,
            requests,
            home: tempfile::tempdir().unwrap(),
        }
    }

    /// Run strata with `args` in the folder of the home, with [`ENV_PROJECT`] in
    /// `PALETTE_PROJECT_UID`. The requests of an earlier run are gone.
    fn run(&self, args: &[&str]) -> Run {
        self.requests.lock().unwrap().clear();
        let mut command = strata_command();
        isolate_palette(&mut command, self.home.path())
            .current_dir(self.home.path())
            .env("PALETTE_API", &self.api)
            .env("PALETTE_PROJECT_UID", ENV_PROJECT)
            .env("STRATA_QUIET", "1")
            .args(args);
        Run::of(&mut command)
    }

    /// The project of each request of the last run, with its path.
    fn asked(&self) -> Vec<(String, Option<String>)> {
        let requests = self.requests.lock().unwrap();
        requests
            .iter()
            .map(|r| (r.path.clone(), r.project.clone()))
            .collect()
    }

    /// A record of `UID_A` at tenant scope, in the folder of the home: a record that names no
    /// project.
    fn tenant_record(&self) -> String {
        let r = self.run(&["snapshot", UID_A, "--project", "", "-o", "tenant.json"]);
        assert_eq!(r.code, 0, "{}", r.err);
        "tenant.json".to_string()
    }
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

#[test]
fn each_command_that_reads_palette_reads_the_project_of_project_and_not_of_the_environment() {
    let stand = Stand::new();
    let record = stand.tenant_record();
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
    let stand = Stand::new();
    let record = stand.tenant_record();
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
    let stand = Stand::new();
    let record = stand.tenant_record();
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
    let stand = Stand::new();
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
    let stand = Stand::new();
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
    assert!(r.out.contains("<br>project p-dev -> p-prod |"), "{}", r.out);
}

#[test]
fn a_record_file_on_each_side_sends_no_request_for_the_name_of_a_project() {
    let stand = Stand::new();
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
    let stand = Stand::new();
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
