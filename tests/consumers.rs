//! What `consumers` searched, and the projects that it could not read (#621).
//!
//! Each project can use a profile of the tenant. The searches at tenant scope give only the
//! projects where a role of the key gives the listing. So strata also searches each project of
//! the tenant listing. A project that Palette refuses is not checked, and an answer with no
//! consumer then exits `2`.

use std::cell::RefCell;

use serde_json::{Value, json};
use strata::api::codes::Denial;
use strata::api::{MemoryApi, Palette};
use strata::ops::{self, ConsumersReport};
use strata::reference::Ref;
use strata::render::{self, Format, Reading};
use strata::term::color::Paint;
use strata::term::progress::Progress;

use crate::common::{
    self, CLUSTER_SEARCH, CLUSTER_SEARCH_SCHEMA, EDGE_HOST_SEARCH, PROJECT_LISTING, UID_A,
    access_denied_message, cluster_route, error_body, post, profile_document, profile_route,
    project_denied_message, project_listing, template,
};

/// The projects of the tenant of these tests, as `(name, uid)`.
const PROJECTS: [(&str, &str); 3] = [("dev", "p1"), ("prod", "p2"), ("staging", "p3")];

/// The document of `base@2.0.0`, a profile of the scope `scope`. A profile of a project is in
/// `p1`. The consumer list names no cluster.
fn base(scope: &str) -> Value {
    let packs = json!([{"name": "cert-manager", "layer": "addon", "type": "spectro",
        "tag": "1.14.0", "version": "1.14.0"}]);
    let mut doc = profile_document(
        UID_A,
        "base",
        "2.0.0",
        template("aws", "cluster", packs),
        None,
    );
    doc["metadata"]["annotations"] = json!({"scope": scope});
    if scope == "project" {
        doc["metadata"]["annotations"]["projectUid"] = json!("p1");
    }
    doc["status"]["inUseClusters"] = json!([]);
    doc
}

/// A scope whose cluster search takes `clusterProfiles` and answers `items`, and whose edge host
/// search answers no host.
fn searched(api: MemoryApi, items: Value) -> MemoryApi {
    api.with(
        CLUSTER_SEARCH_SCHEMA,
        json!({"schema": {"properties": [{"name": "clusterProfiles", "type": "string"}]}}),
    )
    .with(
        post(CLUSTER_SEARCH),
        json!({"items": items, "listmeta": {"continue": ""}}),
    )
    .with(
        post(EDGE_HOST_SEARCH),
        json!({"items": [], "listmeta": {"continue": ""}}),
    )
}

/// The tenant: the profile `doc`, the listing of [`PROJECTS`], and a search that names no cluster.
fn tenant(doc: Value) -> MemoryApi {
    searched(MemoryApi::new(), json!([]))
        .with(profile_route(UID_A), doc)
        .with(PROJECT_LISTING, project_listing(&PROJECTS))
}

/// The project `uid`, whose search names no cluster.
fn project(uid: &str) -> MemoryApi {
    searched(MemoryApi::new().in_project(uid), json!([]))
}

/// The project `staging`, which refuses the cluster search as Palette refuses a key with no role
/// there: HTTP 500 with `ResourceAccessDenied`, and the name of the project.
fn staging_denied() -> MemoryApi {
    project("p3").failing(
        post(CLUSTER_SEARCH),
        500,
        error_body(Denial::Project.as_str(), &project_denied_message("staging")),
    )
}

/// The answer of `consumers` for `base@2.0.0`, by uid. `tenant` answers at tenant scope and in the
/// scope of the key, and `projects` in each project. `list_only` asks for `--list-only`. Each
/// scope that strata connects to goes into `scopes`.
fn consumers_of(
    tenant: &MemoryApi,
    projects: &[MemoryApi; 3],
    list_only: bool,
    scopes: &RefCell<Vec<String>>,
) -> ConsumersReport {
    ops::consumers_with(
        &Ref::Uid(UID_A.into()),
        &ops::ConsumersOptions { list_only },
        |scope| {
            scopes.borrow_mut().push(scope.unwrap_or("-").to_string());
            Ok(Palette::new(match scope {
                Some("p1") => &projects[0],
                Some("p2") => &projects[1],
                Some("p3") => &projects[2],
                _ => tenant,
            }))
        },
        "t1",
        &Progress::silent(),
    )
    .unwrap()
}

/// The JSON of `r`, as `--json` writes it.
fn json_of(r: &ConsumersReport) -> Value {
    let text = render::renderer(Format::Json, Paint::off(), true)
        .render_consumers(r)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn a_profile_of_the_tenant_names_each_project_that_denies_its_search_as_not_checked() {
    let api = tenant(base("tenant"));
    let projects = [project("p1"), project("p2"), staging_denied()];
    let r = consumers_of(&api, &projects, false, &RefCell::default());

    assert_eq!(r.count, 0);
    let read = r.projects_read.as_ref().expect("each project was searched");
    assert_eq!(read.read, 3);
    let unread: Vec<(&str, bool)> = read
        .unread
        .iter()
        .map(|p| (p.name.as_str(), p.denied))
        .collect();
    assert_eq!(unread, [("staging", true)]);
    assert_eq!(
        r.exit_code(),
        2,
        "a consumer in staging can be missing: the gate must not open"
    );
    for p in &projects[..2] {
        let searches = p
            .posted()
            .iter()
            .filter(|(path, _)| path == CLUSTER_SEARCH)
            .count();
        assert_eq!(searches, 1, "one search in each project that the key reads");
    }

    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.starts_with(&format!(
            "base@2.0.0 (uid {UID_A}): no consumer found, and a reading failed\n"
        )),
        "{text}"
    );
    assert!(
        text.ends_with(
            "  scope: tenant profile, usable in every project; 2 of 3 projects searched\n  \
             NOT CHECKED: staging (this key cannot read it)\n"
        ),
        "{text}"
    );
    let md = common::markdown().render_consumers(&r).unwrap();
    assert!(
        md.ends_with(
            "\nScope: tenant profile, usable in every project; 2 of 3 projects searched.\n\n\
             **NOT CHECKED: staging (this key cannot read it).**\n"
        ),
        "{md}"
    );

    let json = json_of(&r);
    assert_eq!(json["profile"]["scope"], "tenant");
    assert_eq!(json["projectsRead"]["read"], 3);
    assert_eq!(json["projectsRead"]["unread"][0]["uid"], "p3");
    assert_eq!(json["projectsRead"]["unread"][0]["name"], "staging");
    assert_eq!(json["projectsRead"]["unread"][0]["denied"], true);
    assert_eq!(json["projectsRead"]["unread"][0]["error"]["kind"], "api");
}

#[test]
fn a_profile_of_the_tenant_that_each_project_answers_exits_zero_and_counts_the_projects() {
    let api = tenant(base("tenant"));
    let projects = [project("p1"), project("p2"), project("p3")];
    let r = consumers_of(&api, &projects, false, &RefCell::default());
    assert_eq!((r.count, r.exit_code()), (0, 0));
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.ends_with(
            "  scope: tenant profile, usable in every project; 3 of 3 projects searched\n"
        ),
        "{text}"
    );
    assert!(!text.contains("NOT CHECKED"), "{text}");
}

#[test]
fn a_cluster_that_only_the_search_of_its_project_names_joins_the_answer_and_is_read_once() {
    // the search of dev names payments, on another version, and checkout, on this one. The search
    // at tenant scope names payments too
    let runs = |uid: &str, name: &str, profile: &str| {
        json!({
            "metadata": {"uid": uid, "name": name, "annotations": {"projectUid": "p1"}},
            "spec": {"clusterProfileTemplates": [
                {"uid": profile, "name": "base", "profileVersion": "2.0.0", "type": "cluster"}]},
            "status": {"state": "Running"},
        })
    };
    let item = |uid: &str, name: &str| {
        json!({"metadata": {"uid": uid, "name": name},
            "specSummary": {"projectMeta": {"uid": "p1", "name": "dev"}},
            "status": {"state": "Running"}})
    };
    let api = searched(MemoryApi::new(), json!([item("c1", "payments")]))
        .with(profile_route(UID_A), base("tenant"))
        .with(PROJECT_LISTING, project_listing(&PROJECTS));
    let dev = searched(
        MemoryApi::new().in_project("p1"),
        json!([item("c1", "payments"), item("c2", "checkout")]),
    )
    .with(cluster_route("c1"), runs("c1", "payments", "other-uid"))
    .with(cluster_route("c2"), runs("c2", "checkout", UID_A));
    let projects = [dev, project("p2"), project("p3")];
    let r = consumers_of(&api, &projects, false, &RefCell::default());

    assert_eq!((r.count, r.exit_code()), (1, 1));
    assert_eq!(r.consumers[0].name, "checkout");
    assert!(r.consumers[0].unlisted);
    assert_eq!(
        projects[0].calls(&cluster_route("c1")),
        1,
        "two searches name it"
    );
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.contains(
            "    cluster search   2 other clusters with the profile: 1 on this version, 1 on other \
             versions\n"
        ),
        "{text}"
    );
}

#[test]
fn a_key_that_cannot_search_the_tenant_leaves_each_project_that_it_cannot_list_not_checked() {
    // without project.list in the tenant, Palette refuses the search at tenant scope, and lists
    // only the projects of the roles of the key
    let api = tenant(base("tenant"))
        .with(PROJECT_LISTING, project_listing(&PROJECTS[..1]))
        .failing(
            post(CLUSTER_SEARCH),
            403,
            error_body(
                Denial::Route.as_str(),
                &access_denied_message("project.list", "tenant"),
            ),
        );
    let projects = [project("p1"), project("p2"), project("p3")];
    let r = consumers_of(&api, &projects, false, &RefCell::default());

    let read = r.projects_read.as_ref().unwrap();
    assert_eq!(read.read, 1);
    assert!(read.unread.is_empty());
    assert!(read.tenant_error.is_some());
    assert!(r.clusters_read.error.is_none(), "dev was searched");
    assert_eq!(r.exit_code(), 2);
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.ends_with(
            "  scope: tenant profile, usable in every project; 1 of 1 project searched\n  \
             NOT CHECKED: each project that this key cannot list\n"
        ),
        "{text}"
    );
    assert_eq!(json_of(&r)["projectsRead"]["tenantError"]["status"], 403);
}

#[test]
fn a_project_listing_that_strata_cannot_read_leaves_each_project_not_checked() {
    let api = tenant(base("tenant")).failing(PROJECT_LISTING, 500, "boom");
    let projects = [project("p1"), project("p2"), project("p3")];
    let scopes = RefCell::default();
    let r = consumers_of(&api, &projects, false, &scopes);
    assert!(r.projects_read.as_ref().unwrap().error.is_some());
    assert_eq!(r.exit_code(), 2);
    assert!(
        !scopes.borrow().iter().any(|s| s.starts_with('p')),
        "no project was searched: {scopes:?}"
    );
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.contains("  NOT CHECKED: each project of the tenant (GET /v1/dashboard/projects/metadata failed with HTTP 500: boom"),
        "{text}"
    );
}

#[test]
fn a_profile_of_a_project_names_its_project_and_searches_no_other_project() {
    let api = tenant(base("project"));
    let projects = [project("p1"), project("p2"), staging_denied()];
    let scopes = RefCell::default();
    let r = consumers_of(&api, &projects, false, &scopes);
    assert!(r.projects_read.is_none());
    assert_eq!((r.count, r.exit_code()), (0, 0));
    assert_eq!(
        (
            r.profile.project_uid.as_str(),
            r.profile.project_name.as_str()
        ),
        ("p1", "dev")
    );
    assert!(
        !scopes.borrow().iter().any(|s| s == "p2" || s == "p3"),
        "only dev can use the version: {scopes:?}"
    );
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.ends_with("  scope: project profile, so only project dev can use it\n"),
        "{text}"
    );
    let json = json_of(&r);
    assert_eq!(json["profile"]["scope"], "project");
    assert_eq!(json["profile"]["projectName"], "dev");
    assert!(json.get("projectsRead").is_none());
}

#[test]
fn list_only_searches_no_project_of_a_profile_of_the_tenant() {
    let api = tenant(base("tenant"));
    let projects = [project("p1"), project("p2"), staging_denied()];
    let scopes = RefCell::default();
    let r = consumers_of(&api, &projects, true, &scopes);
    assert!(r.projects_read.is_none());
    assert_eq!(r.exit_code(), 0, "the list of Palette alone, as before");
    assert!(
        !scopes.borrow().iter().any(|s| s.starts_with('p')),
        "{scopes:?}"
    );
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.ends_with("  scope: tenant profile, usable in every project\n"),
        "{text}"
    );
}

/// `no consumers` is never followed by a line that reads as a consumer (F-017). The search names
/// two clusters on other versions, and no line after the heading names them.
#[test]
fn no_line_after_no_consumers_names_a_cluster_that_runs_another_version() {
    let item = |uid: &str, name: &str| {
        json!({"metadata": {"uid": uid, "name": name},
            "specSummary": {"projectMeta": {"uid": "p1", "name": "dev"}},
            "status": {"state": "Running"}})
    };
    let other = |uid: &str, name: &str| {
        json!({
            "metadata": {"uid": uid, "name": name, "annotations": {"projectUid": "p1"}},
            "spec": {"clusterProfileTemplates": [
                {"uid": "other-uid", "name": "base", "profileVersion": "1.0.0", "type": "cluster"}]},
            "status": {"state": "Running"},
        })
    };
    let api = searched(
        MemoryApi::new().in_project("p1"),
        json!([item("c1", "alpha"), item("c2", "beta")]),
    )
    .with(profile_route(UID_A), base("project"))
    .with(PROJECT_LISTING, project_listing(&PROJECTS[..1]))
    .with(cluster_route("c1"), other("c1", "alpha"))
    .with(cluster_route("c2"), other("c2", "beta"));
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!((r.count, r.exit_code()), (0, 0));
    for text in [
        common::terminal().render_consumers(&r).unwrap(),
        common::markdown().render_consumers(&r).unwrap(),
    ] {
        assert!(text.contains("no consumers"), "{text}");
        assert!(
            !text.contains("alpha") && !text.contains("beta"),
            "no line names a cluster that does not run the version: {text}"
        );
        assert!(
            text.contains(
                "2 other clusters with the profile: 0 on this version, 2 on other versions"
            ),
            "{text}"
        );
    }
}

/// The rows of the terminal block under `searched:`, each as its label and its text.
fn searched_rows(text: &str) -> Vec<(String, String)> {
    text.lines()
        .skip_while(|l| *l != "  searched:")
        .skip(1)
        .take_while(|l| l.starts_with("    "))
        .map(|l| {
            let (label, rest) = l.trim_start().split_once("  ").unwrap_or((l, ""));
            (label.to_string(), rest.trim_start().to_string())
        })
        .collect()
}

#[test]
fn each_row_under_searched_names_a_reading_in_one_column() {
    let api = tenant(base("tenant"));
    let projects = [project("p1"), project("p2"), staging_denied()];
    let r = consumers_of(&api, &projects, false, &RefCell::default());
    let text = common::terminal().render_consumers(&r).unwrap();
    let rows = searched_rows(&text);
    let readings: Vec<Option<Reading>> = rows.iter().map(|(l, _)| Reading::from_text(l)).collect();
    assert_eq!(
        readings,
        [
            Some(Reading::ConsumerList),
            Some(Reading::ClusterSearch),
            Some(Reading::ClusterGroups),
            Some(Reading::EdgeHosts),
        ],
        "{text}"
    );
    // the text of each row starts in one column, the width of the longest label and two spaces
    let width = Reading::ALL.iter().map(|w| w.as_str().len()).max().unwrap();
    for line in text.lines().filter(|l| l.starts_with("    ")) {
        assert_eq!(&line[4 + width..4 + width + 2], "  ", "{line}");
        assert_ne!(line.as_bytes()[4 + width + 2], b' ', "{line}");
    }
}
