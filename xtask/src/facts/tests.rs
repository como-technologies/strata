//! The tests of the list of the facts: each rule fails on one fault, and the list of the
//! repository passes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use strata::api::Method;
use strata_preflight::rule::Catalog;

use super::check::{NOT_FACTS, check as faults_of, route_matches, template_matches};
use super::cloud::CloudSources;
use super::from_text;
use super::model::load;
use super::sets::{Set, SetRead};
use super::sources::{
    Sources, api_requests, book_rows, calls_of, query_names, rule_queries, rule_requests,
    section_headings, section_rows, shape,
};

/// A request fact that is right in the world of [`world`].
const FACT: &str = r"
- id: profile-document
  kind: request
  request: GET /v1/clusterprofiles/{uid}
  reads:
    fields: [metadata.annotations.scope]
    codes: [ClusterProfilePackNotAllowed]
  stand_in:
    - v1/clusterprofiles/{uid}
  live: [scope.sh:annotation-tenant]
  pages:
    - reference/palette-api.md#a-profile-version
    - internals/palette/profiles.md#edits-in-place
  code: [src/api/palette/profiles.rs]
  rules: [template-outside]
";

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

/// The world in which [`FACT`] has no fault: the book, the scripts, the rules, and the stand-in
/// all agree with it.
fn world() -> Sources {
    let page = |text: &str| text.to_string();
    Sources {
        cloud: CloudSources::default(),
        facts: from_text(FACT),
        served: Some(set(&["v1/clusterprofiles/64e0000000000000000000a1"])),
        requests: set(&["GET /v1/clusterprofiles/{}"]),
        texts: BTreeMap::from([
            (
                "reference/palette-api.md".to_string(),
                page("ClusterProfilePackNotAllowed"),
            ),
            ("internals/palette/profiles.md".to_string(), page("")),
        ]),
        anchors: BTreeMap::from([
            (
                "reference/palette-api.md".to_string(),
                set(&["a-profile-version"]),
            ),
            (
                "internals/palette/profiles.md".to_string(),
                set(&["edits-in-place"]),
            ),
        ]),
        sections: set(&["internals/palette/profiles.md#edits-in-place"]),
        checks: BTreeMap::from([("scope.sh".to_string(), set(&["annotation-tenant"]))]),
        rows: BTreeMap::from([("scope.sh".to_string(), set(&["annotation-tenant"]))]),
        files: set(&["src/api/palette/profiles.rs"]),
        rule_ids: set(&["template-outside"]),
        rule_requests: BTreeMap::from([(
            "GET /v1/clusterprofiles/{}".to_string(),
            set(&["template-outside"]),
        )]),
        rule_queries: BTreeMap::new(),
        set_reads: Vec::new(),
        closed_sets: Vec::new(),
    }
}

/// Assert that `sources` has exactly one fault, and that its message holds `part`.
fn assert_one_fault(sources: &Sources, part: &str) {
    let found = faults_of(sources);
    assert!(
        found.len() == 1 && found[0].contains(part),
        "expected one fault with `{part}`, got {found:#?}"
    );
}

#[test]
fn a_list_that_agrees_with_the_book_the_scripts_and_the_stand_in_has_no_fault() {
    assert_eq!(faults_of(&world()), Vec::<String>::new());
}

#[test]
fn a_route_that_the_stand_in_serves_and_no_fact_names_is_a_fault() {
    let mut sources = world();
    sources
        .served
        .as_mut()
        .unwrap()
        .insert("v1/spectroclusters/c1".to_string());
    assert_one_fault(
        &sources,
        "the stand-in serves `v1/spectroclusters/c1`, and no fact names this route",
    );
}

#[test]
fn a_route_of_a_fact_that_the_stand_in_does_not_serve_is_a_fault() {
    let mut sources = world();
    sources.facts[0].stand_in = vec![
        "v1/clusterprofiles/{uid}".to_string(),
        "v1/clusterprofiles/{uid}/variables".to_string(),
    ];
    assert_one_fault(
        &sources,
        "serves no route like `v1/clusterprofiles/{uid}/variables`",
    );
}

#[test]
fn a_run_of_the_book_is_not_needed_for_the_other_rules() {
    let mut sources = world();
    sources.served = None;
    sources.facts[0]
        .stand_in
        .push("v1/clusterprofiles/{uid}/variables".to_string());
    assert_eq!(faults_of(&sources), Vec::<String>::new());
}

#[test]
fn a_route_of_the_stand_in_must_be_a_path_with_whole_segments_as_names() {
    for pattern in [
        "/v1/clusterprofiles/{uid}",
        "v1/clusterprofiles/{uid}.json",
        "v1/clusterprofiles//{uid}",
        "v1/clusterprofiles/{u{id}}",
        "clusterprofiles/{uid}",
    ] {
        let mut sources = world();
        sources.served = None;
        sources.facts[0].stand_in = vec![pattern.to_string()];
        assert_one_fault(&sources, "must be a path under v1/");
    }
}

#[test]
fn a_fact_with_no_live_check_and_no_reason_is_a_fault() {
    let mut sources = world();
    sources.facts[0].live.clear();
    assert_one_fault(&sources, "no live check proves it");
}

#[test]
fn a_fact_with_no_live_check_and_a_reason_is_not_a_fault() {
    let mut sources = world();
    sources.facts[0].live.clear();
    sources.facts[0].unchecked = Some("No run covers it.".to_string());
    assert_eq!(faults_of(&sources), Vec::<String>::new());
}

#[test]
fn a_reason_beside_a_live_check_is_a_fault() {
    let mut sources = world();
    sources.facts[0].unchecked = Some("No run covers it.".to_string());
    assert_one_fault(&sources, "`unchecked` must go");
}

#[test]
fn a_live_check_that_no_script_has_is_a_fault() {
    let mut sources = world();
    sources.facts[0].live = vec!["scope.sh:annotation-nowhere".to_string()];
    assert_one_fault(
        &sources,
        "the script `scope.sh` calls no check `annotation-nowhere`",
    );
}

#[test]
fn a_check_that_a_script_builds_from_a_variable_is_matched_by_its_pattern() {
    let mut sources = world();
    sources
        .checks
        .insert("scope.sh".to_string(), set(&["$id-says", "fault-$id"]));
    sources.rows.insert(
        "scope.sh".to_string(),
        set(&["pack-cloud-support-says", "fault-pack-state"]),
    );
    for check in ["pack-cloud-support-says", "fault-pack-state"] {
        sources.facts[0].live = vec![format!("scope.sh:{check}")];
        assert_eq!(faults_of(&sources), Vec::<String>::new(), "{check}");
    }
    sources.facts[0].live = vec!["scope.sh:pack-cloud-support".to_string()];
    assert_one_fault(&sources, "calls no check `pack-cloud-support`");
}

#[test]
fn a_template_matches_the_names_that_its_variables_can_make() {
    assert!(template_matches("$id-says", "pack-state-says"));
    assert!(template_matches("fault-${id}", "fault-pack-state"));
    assert!(template_matches("export-$kind-$name", "export-aws-stack"));
    assert!(template_matches("$id", "anything"));
    assert!(template_matches("export-<name>", "export-aws-stack"));
    assert!(template_matches("export-<name>", "export-$name"));
    assert!(!template_matches("$id-says", "pack-state"));
    assert!(!template_matches("fault-$id", "fault-"));
    assert!(!template_matches("fault.$id", "faultxy"));
    // a name with no variable is not a template: equality is the rule for it
    assert!(!template_matches("fault", "fault"));
}

#[test]
fn a_live_check_of_a_script_that_does_not_exist_is_a_fault() {
    let mut sources = world();
    sources.facts[0].live = vec!["nowhere.sh:annotation-tenant".to_string()];
    assert_one_fault(&sources, "examples/live/ has no script `nowhere.sh`");
}

#[test]
fn a_live_check_with_no_row_in_the_book_is_a_fault() {
    let mut sources = world();
    sources.rows.insert("scope.sh".to_string(), BTreeSet::new());
    assert_one_fault(
        &sources,
        "the book has no row for the check `annotation-tenant` of `scope.sh`",
    );
}

#[test]
fn a_live_check_must_name_its_script() {
    let mut sources = world();
    sources.facts[0].live = vec!["annotation-tenant".to_string()];
    assert_one_fault(&sources, "must be `script.sh:check`");
}

#[test]
fn a_fact_with_no_page_is_a_fault() {
    let mut sources = world();
    sources.facts[0].pages.clear();
    sources.facts[0].reads.codes.clear();
    sources.sections.clear();
    assert_one_fault(&sources, "`pages` is empty");
}

#[test]
fn a_page_that_is_not_in_the_book_is_a_fault() {
    let mut sources = world();
    sources.facts[0]
        .pages
        .push("reference/nowhere.md".to_string());
    assert_one_fault(&sources, "`reference/nowhere.md` is not a page of the book");
}

#[test]
fn an_anchor_that_the_page_does_not_have_is_a_fault() {
    let mut sources = world();
    sources.facts[0].pages[0] = "reference/palette-api.md#a-profile-nowhere".to_string();
    assert_one_fault(&sources, "has no heading `#a-profile-nowhere`");
}

#[test]
fn a_request_of_the_api_page_with_no_fact_is_a_fault() {
    let mut sources = world();
    sources
        .requests
        .insert("GET /v1/clusterprofiles/{}/variables".to_string());
    assert_one_fault(
        &sources,
        "lists `GET /v1/clusterprofiles/{}/variables`, and no fact has this request",
    );
}

#[test]
fn a_request_of_a_rule_with_no_fact_is_a_fault() {
    let mut sources = world();
    sources
        .rule_requests
        .insert("GET /v1/overlords/{}".to_string(), set(&["pcg-running"]));
    assert_one_fault(
        &sources,
        "send `GET /v1/overlords/{}`, and no fact has this request",
    );
}

#[test]
fn the_rules_of_a_request_fact_are_the_rules_that_send_the_request() {
    let mut sources = world();
    sources.facts[0].rules.clear();
    assert_one_fault(
        &sources,
        "the rules that send the request are {\"template-outside\"}",
    );
}

#[test]
fn a_request_that_lacks_a_query_of_the_path_of_its_rules_is_a_fault() {
    let mut sources = world();
    sources.rule_queries.insert(
        "template-outside".to_string(),
        set(&["cloudAccountUid", "region"]),
    );
    sources.facts[0].request = Some("GET /v1/clusterprofiles/{uid}?region={region}".to_string());
    assert_one_fault(
        &sources,
        "fact `profile-document`: the rule `template-outside` sends the query cloudAccountUid in its path",
    );
}

#[test]
fn a_request_that_holds_each_query_of_the_path_of_its_rules_has_no_fault() {
    let mut sources = world();
    sources.rule_queries.insert(
        "template-outside".to_string(),
        set(&["cloudAccountUid", "region"]),
    );
    // the request can hold a query that no rule sends, as the filter of the instance types does
    sources.facts[0].request = Some(
        "GET /v1/clusterprofiles/{uid}?region={region}&cloudAccountUid={cloudAccountUid}&limit=0"
            .to_string(),
    );
    assert_eq!(faults_of(&sources), Vec::<String>::new());
}

#[test]
fn a_rule_that_sends_no_query_asks_nothing_of_the_request_of_its_fact() {
    let mut sources = world();
    sources.facts[0].request = Some("GET /v1/clusterprofiles/{uid}".to_string());
    assert!(sources.rule_queries.is_empty());
    assert_eq!(faults_of(&sources), Vec::<String>::new());
}

#[test]
fn a_rule_that_does_not_exist_is_a_fault() {
    let mut sources = world();
    sources.rule_ids.clear();
    assert_one_fault(&sources, "the rule `template-outside` does not exist");
}

#[test]
fn a_file_of_strata_that_does_not_exist_is_a_fault() {
    let mut sources = world();
    sources.files.clear();
    assert_one_fault(
        &sources,
        "the file `src/api/palette/profiles.rs` does not exist",
    );
}

#[test]
fn a_fact_with_no_code_and_no_rule_is_a_fault() {
    let mut sources = world();
    sources.facts[0].code.clear();
    sources.facts[0].rules.clear();
    sources.rule_requests.clear();
    sources.requests.clear();
    sources.facts[0].request = None;
    sources.facts[0].kind = super::model::Kind::Behavior;
    assert_one_fault(&sources, "`code or rules` is empty");
}

#[test]
fn a_fact_that_reads_nothing_is_a_fault() {
    let mut sources = world();
    sources.facts[0].reads = Default::default();
    assert_one_fault(&sources, "`reads` is empty");
}

#[test]
fn an_error_code_that_no_page_of_the_fact_has_is_a_fault() {
    let mut sources = world();
    sources.facts[0].reads.codes.push("ErrNowhere".to_string());
    assert_one_fault(&sources, "no page of the fact has the code `ErrNowhere`");
}

#[test]
fn an_absent_code_is_stated_on_a_page_of_the_fact_like_an_error_code_that_it_reads() {
    let mut sources = world();
    sources.facts[0].absent_codes = vec!["ClusterProfilePackNotAllowed".to_string()];
    assert_eq!(faults_of(&sources), Vec::<String>::new());
    sources.facts[0].absent_codes = vec!["ErrNowhere".to_string()];
    assert_one_fault(&sources, "no page of the fact has the code `ErrNowhere`");
}

#[test]
fn a_fact_with_no_get_request_that_names_an_absent_code_is_a_fault() {
    let mut sources = world();
    sources.facts[0].absent_codes = vec!["ClusterProfilePackNotAllowed".to_string()];
    sources.facts[0].request = Some("POST /v1/clusterprofiles/{uid}".to_string());
    let found = faults_of(&sources);
    assert!(
        found
            .iter()
            .any(|m| m.contains("only a fact with a GET request has `absent_codes`")),
        "{found:#?}"
    );
}

#[test]
fn a_section_of_the_fact_pages_that_no_fact_names_is_a_fault() {
    let mut sources = world();
    sources
        .sections
        .insert("internals/palette/profiles.md#drafts".to_string());
    assert_one_fault(
        &sources,
        "internals/palette/profiles.md#drafts is a section of internals/palette/, and no fact names it",
    );
}

#[test]
fn a_section_that_states_nothing_about_palette_needs_no_fact() {
    let mut sources = world();
    sources.sections.insert(NOT_FACTS[0].to_string());
    assert_eq!(faults_of(&sources), Vec::<String>::new());
}

#[test]
fn two_facts_with_one_id_are_a_fault() {
    let mut sources = world();
    let mut second = sources.facts[0].clone();
    second.request = None;
    second.kind = super::model::Kind::Behavior;
    second.rules.clear();
    sources.facts.push(second);
    assert_one_fault(&sources, "another fact has this id");
}

#[test]
fn two_facts_with_one_request_are_a_fault() {
    let mut sources = world();
    let mut second = sources.facts[0].clone();
    second.id = "profile-document-again".to_string();
    sources.facts.push(second);
    let found = faults_of(&sources);
    assert!(
        found.iter().any(|m| m.contains("have one request")),
        "{found:#?}"
    );
}

#[test]
fn an_id_must_be_words_with_dashes() {
    let mut sources = world();
    sources.facts[0].id = "Profile_Document".to_string();
    assert_one_fault(&sources, "the id must be words with dashes");
}

#[test]
fn only_a_request_fact_has_a_request_and_each_request_fact_has_one() {
    let mut sources = world();
    sources.facts[0].request = None;
    sources.requests.clear();
    sources.rule_requests.clear();
    let found = faults_of(&sources);
    assert!(
        found
            .iter()
            .any(|m| m.contains("a request fact needs a request")),
        "{found:#?}"
    );
    let mut sources = world();
    sources.facts[0].kind = super::model::Kind::Gap;
    let found = faults_of(&sources);
    assert!(
        found
            .iter()
            .any(|m| m.contains("only a request fact has a request")),
        "{found:#?}"
    );
}

#[test]
fn a_request_must_be_a_method_and_a_path_under_v1() {
    let mut sources = world();
    sources.facts[0].request = Some("DELETE /v1/clusterprofiles/{uid}".to_string());
    let found = faults_of(&sources);
    assert!(
        found.iter().any(|m| m.contains("must be GET or POST")),
        "{found:#?}"
    );
}

#[test]
fn the_shape_of_a_request_ignores_the_names_of_the_placeholders_and_the_query() {
    let same = [
        "GET /v1/clusterprofiles/{uid}/variables",
        "GET /v1/clusterprofiles/<uid>/variables?limit=50",
        "GET /v1/clusterprofiles/{{ .entity.spec.profiles[0].uid }}/variables",
    ];
    for request in same {
        assert_eq!(
            shape(request),
            "GET /v1/clusterprofiles/{}/variables",
            "{request}"
        );
    }
    assert_eq!(
        shape("GET /v1/clouds/{aws,gcp}/regions/{region}/instancetypes?cloudAccountUid="),
        "GET /v1/clouds/{}/regions/{}/instancetypes"
    );
    assert_ne!(shape("GET /v1/packs"), shape("POST /v1/packs"));
}

#[test]
fn the_names_of_the_query_of_a_request_are_the_same_in_the_book_the_list_and_the_rules() {
    let same = [
        "GET /v1/clouds/aws/regions?cloudAccountUid={cloudAccountUid}&region={region}",
        "GET /v1/clouds/aws/regions?cloudAccountUid=&region=",
        "GET /v1/clouds/aws/regions?region=<region>&cloudAccountUid=x",
        "GET /v1/clouds/aws/regions?cloudAccountUid={{ .entity.spec.cloudAccountUid }}&region={{ .entity.spec.cloudConfig.region }}",
    ];
    for request in same {
        assert_eq!(
            query_names(request),
            set(&["cloudAccountUid", "region"]),
            "{request}"
        );
    }
    assert!(query_names("GET /v1/clouds/aws/regions").is_empty());
    assert!(query_names("GET /v1/clouds/aws/regions?").is_empty());
}

#[test]
fn a_program_of_a_rule_that_holds_a_question_mark_or_an_ampersand_makes_no_query() {
    let request = "GET /v1/x/{{ .a? // .b & .c }}/y?region={{ .r }}&limit={{ .l }}";
    assert_eq!(query_names(request), set(&["region", "limit"]));
    assert_eq!(shape(request), "GET /v1/x/{}/y");
}

#[test]
fn a_route_matches_a_pattern_segment_by_segment() {
    assert!(route_matches(
        "v1/clusterprofiles/{uid}",
        "v1/clusterprofiles/u1"
    ));
    assert!(route_matches("v1/packs", "v1/packs"));
    assert!(!route_matches(
        "v1/clusterprofiles/{uid}",
        "v1/clusterprofiles/u1/variables"
    ));
    assert!(!route_matches(
        "v1/clusterprofiles/{uid}/variables",
        "v1/clusterprofiles/u1"
    ));
    assert!(!route_matches(
        "v1/spectroclusters/{uid}",
        "v1/clusterprofiles/u1"
    ));
    assert!(route_matches(
        "v1/clusterprofiles/validate/name",
        "v1/clusterprofiles/validate/name"
    ));
    assert!(!route_matches(
        "v1/clusterprofiles/validate/name",
        "v1/clusterprofiles/validate/packs"
    ));
}

// ---- the files of the list -----------------------------------------------------------------------

fn write(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
}

#[test]
fn the_files_of_the_list_give_their_facts_in_the_order_of_the_names() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    write(
        dir,
        "b.yaml",
        &format!("schema: strata-facts/1\nfacts:{FACT}"),
    );
    write(dir, "a.yaml", "schema: strata-facts/1\nfacts: []\n");
    let (loaded, bad) = load(dir);
    assert!(bad.is_empty(), "{bad:?}");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, "profile-document");
}

#[test]
fn a_file_with_a_field_that_the_schema_lacks_is_an_error_that_names_the_file() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    write(
        dir,
        "a.yaml",
        "schema: strata-facts/1\nfacts:\n  - id: x\n    kind: behavior\n    colour: red\n",
    );
    let (_, bad) = load(dir);
    assert!(bad[0].starts_with("a.yaml: "), "{bad:?}");
    assert!(bad[0].contains("colour"), "{bad:?}");
}

#[test]
fn a_file_that_cannot_be_read_does_not_hide_the_facts_of_the_other_files() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    write(dir, "a.yaml", "schema: strata-facts/1\nfacts: [{broken\n");
    write(
        dir,
        "b.yaml",
        &format!("schema: strata-facts/1\nfacts:{FACT}"),
    );
    let (loaded, bad) = load(dir);
    assert_eq!(loaded.len(), 1);
    assert_eq!(bad.len(), 1);
    assert!(bad[0].starts_with("a.yaml: "), "{bad:?}");
}

#[test]
fn a_file_with_another_schema_is_an_error() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    write(dir, "a.yaml", "schema: strata-facts/2\nfacts: []\n");
    let (_, bad) = load(dir);
    assert!(bad[0].contains("must be `strata-facts/1`"), "{bad:?}");
}

#[test]
fn a_directory_with_no_file_is_an_error() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let (_, bad) = load(dir);
    assert!(bad[0].contains("no file of facts"), "{bad:?}");
}

// ---- the readers ---------------------------------------------------------------------------------

#[test]
fn the_requests_of_the_api_page_are_the_code_spans_of_the_first_column() {
    let page = "\
Prose with `GET /v1/outside` in it.

| Request | Gives |
| --- | --- |
| `GET /v1/a/{uid}` and `POST /v1/b/<name>?limit=50` | The thing, and `GET /v1/in-second-column`. |
| `not a request` | Nothing. |

| Request | Read by | Gives |
| --- | --- | --- |
| `GET /v1/clouds/{aws,gcp}/regions?cloudAccountUid=` | `instance-offering` | The regions. |
";
    assert_eq!(
        api_requests(page),
        set(&["GET /v1/a/{}", "POST /v1/b/{}", "GET /v1/clouds/{}/regions"])
    );
}

#[test]
fn the_checks_of_a_script_are_the_first_words_after_check_at_the_start_of_a_line() {
    let script = "\
check() {
  local id=$1
}
check a1 0 'what' -- cmd
  check \"$1-says\" 0 'what' -- cmd
check dotted.name 1 'what' -- cmd
# check commented 0 'what'
echo check printed 0
";
    assert_eq!(calls_of(script), set(&["a1", "$1-says", "dotted.name"]));
}

#[test]
fn the_rows_of_a_section_are_the_names_in_the_first_column_up_to_the_next_section() {
    let page = "\
## one.sh

| Check | What |
|---|---|
| `a1`, `a1-addon` | `not-a-name` is in the second column |

## two.sh

| `b1` | x |
";
    assert_eq!(section_rows(page, "## one.sh"), set(&["a1", "a1-addon"]));
    assert_eq!(section_rows(page, "## two.sh"), set(&["b1"]));
    assert!(section_rows(page, "## three.sh").is_empty());
}

#[test]
fn the_book_lists_the_checks_of_verify_sh_on_the_page_of_the_live_example() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let development = dir.join("development");
    std::fs::create_dir_all(&development).unwrap();
    write(
        &development,
        "live-runs.md",
        "## in-place.sh\n\n| `a1` | x |\n\n## verify.sh\n\n| `wrong` | x |\n",
    );
    write(
        &development,
        "live-example.md",
        "## The checks of verify.sh\n\n| `state` | x |\n\n## Other\n\n| `other` | x |\n",
    );
    let rows = book_rows(dir);
    assert_eq!(rows["in-place.sh"], set(&["a1"]));
    assert_eq!(rows["verify.sh"], set(&["state"]));
}

#[test]
fn only_the_level_two_headings_are_sections() {
    let page = "# Title\n\n## One thing\n\n### A part\n\n## A second: thing\n";
    assert_eq!(
        section_headings(page),
        set(&["one-thing", "a-second-thing"])
    );
}

#[test]
fn the_requests_of_the_rules_are_the_palette_checks_and_the_reads_of_a_command_check() {
    let rule = |id: &str, check: &str| {
        format!(
            "  - id: {id}\n    title: t\n    engines: [all]\n    tier: palette\n    status: seen\n    what: w\n    check:\n{check}\n"
        )
    };
    let section = format!(
        "section: a\ntitle: A\nrules:\n{}{}{}{}{}{}",
        rule("static-rule", "      kind: static\n      expr: 'true'"),
        rule(
            "reads-then-runs",
            "      kind: exec\n      argv: [aws, service-quotas]\n      palette:\n        path: /v1/clouds/{{ .accountKind }}/regions/{{ .region }}/instancetypes?cloudAccountUid=x\n      expr: 'true'"
        ),
        rule(
            "runs-alone",
            "      kind: exec\n      argv: [aws, ec2]\n      expr: 'true'"
        ),
        rule(
            "reads-profile",
            "      kind: palette\n      path: /v1/clusterprofiles/{{ .entity.spec.profiles[0].uid }}\n      expr: 'true'"
        ),
        rule(
            "validates",
            "      kind: palette\n      method: POST\n      path: /v1/spectroclusters/{{ .routeCloud }}/validate\n      expr: 'true'"
        ),
        rule(
            "reads-profile-again",
            "      kind: palette\n      method: GET\n      path: /v1/clusterprofiles/{{ $item.uid }}\n      each: .entity.spec.profiles\n      expr: 'true'"
        ),
    );
    let catalog = Catalog {
        sections: vec![serde_yaml::from_str(&section).expect("a section")],
    };
    let (ids, requests) = rule_requests(&catalog);
    assert_eq!(
        ids,
        set(&[
            "static-rule",
            "reads-then-runs",
            "runs-alone",
            "reads-profile",
            "validates",
            "reads-profile-again"
        ])
    );
    assert_eq!(
        requests,
        BTreeMap::from([
            (
                "GET /v1/clusterprofiles/{}".to_string(),
                set(&["reads-profile", "reads-profile-again"])
            ),
            (
                "GET /v1/clouds/{}/regions/{}/instancetypes".to_string(),
                set(&["reads-then-runs"])
            ),
            (
                "POST /v1/spectroclusters/{}/validate".to_string(),
                set(&["validates"])
            ),
        ])
    );
    assert_eq!(
        rule_queries(&catalog),
        BTreeMap::from([("reads-then-runs".to_string(), set(&["cloudAccountUid"]))])
    );
}

// ---- the list of the repository ------------------------------------------------------------------

/// The check that `just docs-check` makes needs a run of the book, and adds the stand-in. This
/// test makes the other rules on the repository, so `just test` keeps the list true.
#[test]
fn the_list_of_the_repository_agrees_with_the_book_the_live_scripts_and_the_rules() {
    let found = super::check(&crate::repository(), None);
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn the_request_of_each_fact_holds_the_query_that_its_rules_send() {
    // the facts whose request lacked a query of the path of their rules, with the names that the
    // rules send
    const QUERIES: &[(&str, &[&str])] = &[
        ("validate-key-pairs", &["cloudAccountUid"]),
        ("aws-image-volume-size", &["cloudAccountUid", "region"]),
        ("aws-host-capacity", &["hrgArn"]),
        ("azure-region-list", &["cloudAccountUid", "subscriptionId"]),
        ("azure-resource-group-list", &["cloudAccountUid"]),
        (
            "azure-storage-account-list",
            &["cloudAccountUid", "subscriptionId"],
        ),
        ("azure-network-list", &["cloudAccountUid", "resourceGroup"]),
        (
            "vsphere-compute-resources",
            &["computecluster", "datacenter"],
        ),
        (
            "validate-instance-types",
            &["cloudAccountUid", "cpuGtEq", "memoryGtEq", "subscriptionId"],
        ),
    ];
    let facts = super::load(&crate::repository()).expect("the list of the facts");
    for (id, names) in QUERIES {
        let fact = facts.iter().find(|f| f.id == *id).expect(id);
        let request = fact.request.as_deref().expect(id);
        assert_eq!(query_names(request), set(names), "{id}");
    }
}

#[test]
fn the_list_of_the_repository_prints_as_json_for_a_script() {
    let json = super::print(&crate::repository()).unwrap();
    let document: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(document["schema"], "strata-facts/1");
    let facts = document["facts"].as_array().unwrap();
    assert!(!facts.is_empty());
    for fact in facts {
        for field in [
            "id", "kind", "reads", "stand_in", "live", "pages", "code", "rules",
        ] {
            assert!(fact.get(field).is_some(), "{} has no {field}", fact["id"]);
        }
    }
}

#[test]
fn the_parts_of_a_request_are_its_method_and_its_path() {
    let mut fact = world().facts[0].clone();
    fact.request = Some("GET /v1/clusterprofiles/{uid}".to_string());
    assert_eq!(
        fact.parts(),
        Some((Method::Get, "/v1/clusterprofiles/{uid}"))
    );
    assert_eq!(fact.get_path(), Some("/v1/clusterprofiles/{uid}"));

    fact.request = Some("POST /v1/spectroclusters/search".to_string());
    assert_eq!(
        fact.parts(),
        Some((Method::Post, "/v1/spectroclusters/search"))
    );
    assert_eq!(fact.get_path(), None);

    for request in [Some("PATCH /v1/x"), Some("/v1/x"), None] {
        fact.request = request.map(String::from);
        assert_eq!(fact.parts(), None, "{request:?}");
    }
}

#[test]
fn the_query_of_a_request_is_part_of_its_path_and_not_of_its_shape() {
    let mut fact = world().facts[0].clone();
    fact.request = Some("GET /v1/clouds/aws/regions?cloudAccountUid={cloudAccountUid}".to_string());
    assert_eq!(
        fact.get_path(),
        Some("/v1/clouds/aws/regions?cloudAccountUid={cloudAccountUid}")
    );
    // the book and the rules write the route with their own query, so the shape has none
    assert_eq!(
        shape("GET /v1/clouds/aws/regions?cloudAccountUid={cloudAccountUid}"),
        shape("GET /v1/clouds/aws/regions?cloudAccountUid={{ .entity.spec.cloudAccountUid }}")
    );
    assert_eq!(
        shape("GET /v1/filters/metadata?filterType=tag"),
        "GET /v1/filters/metadata"
    );
}

#[test]
fn a_fact_that_reads_only_names_reads_something_and_reads_a_document() {
    let mut sources = world();
    sources.facts[0].reads = Default::default();
    sources.facts[0].reads.names.insert(
        "schema.properties[].name".to_string(),
        vec!["tags".to_string()],
    );
    assert!(sources.facts[0].reads.reads_a_document());
    let found = faults_of(&sources);
    assert!(
        !found.iter().any(|m| m.contains("`reads` is empty")),
        "{found:?}"
    );
}

// ---- the reads that note a value of a closed set --------------------------------------------

/// A read of the set `ProfileType` at the field `spec.published.type` of the fact of [`FACT`].
fn profile_type_read() -> SetRead {
    SetRead {
        set: "ProfileType".to_string(),
        fact: "profile-document",
        field: "spec.published.type",
    }
}

/// The world of [`world`] with the read of [`profile_type_read`], its list in the fact, and the one
/// call of `opt_set` in the code that the table names. A comment or a test is no call.
fn world_with_a_set_read() -> Sources {
    let mut sources = world();
    sources.set_reads = vec![profile_type_read()];
    sources.closed_sets = vec![Set {
        name: "ProfileType".to_string(),
        values: ["cluster", "add-on"].map(String::from).to_vec(),
    }];
    sources.cloud.code = BTreeMap::from([(
        "src/ops/list.rs".to_string(),
        "p.profile_type = template.and_then(|t| opt_set(t, \"/type\", PROFILE_TEMPLATE));"
            .to_string(),
    )]);
    sources.facts[0].reads.values.insert(
        "spec.published.type".to_string(),
        ["cluster", "add-on"].map(String::from).to_vec(),
    );
    sources.facts[0]
        .reads
        .sets
        .insert("spec.published.type".to_string(), "ProfileType".to_string());
    sources
}

#[test]
fn a_read_of_a_closed_set_with_its_list_in_a_get_fact_has_no_fault() {
    assert_eq!(faults_of(&world_with_a_set_read()), Vec::<String>::new());
}

#[test]
fn a_read_of_a_closed_set_whose_fact_lists_no_values_is_a_fault() {
    let mut sources = world_with_a_set_read();
    sources.facts[0].reads.values.clear();
    let found = faults_of(&sources);
    assert!(
        found.iter().any(|m| m.contains("fact `profile-document`")
            && m.contains("strata notes a value of ProfileType at `spec.published.type`")),
        "{found:?}"
    );
}

#[test]
fn a_list_that_names_a_value_the_set_does_not_have_is_a_fault() {
    let mut sources = world_with_a_set_read();
    sources.facts[0].reads.values.insert(
        "spec.published.type".to_string(),
        ["cluster", "add-on", "edge"].map(String::from).to_vec(),
    );
    assert_one_fault(
        &sources,
        "it lists `edge` at `spec.published.type`, and the set ProfileType has no such value",
    );
}

#[test]
fn a_set_with_a_value_that_the_list_lacks_is_a_fault_unless_only_strata_gives_the_value() {
    let mut sources = world_with_a_set_read();
    sources.facts[0].reads.values.insert(
        "spec.published.type".to_string(),
        vec!["cluster".to_string()],
    );
    assert_one_fault(
        &sources,
        "the set ProfileType has `add-on`, and the list at `spec.published.type` lacks it",
    );
    // a word of strata is the set alone: the list of a listing of packs has no `absent`
    let mut sources = world_with_a_set_read();
    sources.closed_sets = vec![Set {
        name: "PackState".to_string(),
        values: ["deprecated", "absent"].map(String::from).to_vec(),
    }];
    sources.set_reads.clear();
    sources.cloud.code.clear();
    sources.facts[0].reads.sets =
        BTreeMap::from([("spec.published.type".to_string(), "PackState".to_string())]);
    sources.facts[0].reads.values.insert(
        "spec.published.type".to_string(),
        vec!["deprecated".to_string()],
    );
    assert_eq!(faults_of(&sources), Vec::<String>::new());
}

#[test]
fn a_fact_that_names_a_set_names_a_type_of_strata_and_a_field_with_a_list() {
    let mut sources = world_with_a_set_read();
    sources.facts[0]
        .reads
        .sets
        .insert("spec.published.type".to_string(), "ProfileKind".to_string());
    let found = faults_of(&sources);
    assert!(
        found
            .iter()
            .any(|m| m.contains("names ProfileKind at `spec.published.type`")
                && m.contains("no closed set that `closed_sets()`")),
        "{found:?}"
    );
    let mut sources = world_with_a_set_read();
    sources.facts[0]
        .reads
        .sets
        .insert("status.state".to_string(), "ProfileType".to_string());
    assert_one_fault(&sources, "`reads.values` has no list there");
}

#[test]
fn a_read_of_a_closed_set_whose_fact_does_not_name_the_set_is_a_fault() {
    let mut sources = world_with_a_set_read();
    sources.facts[0].reads.sets.clear();
    assert_one_fault(&sources, "so `reads.sets` must name ProfileType there");
}

#[test]
fn a_read_of_a_closed_set_in_a_fact_that_is_no_get_request_or_not_there_is_a_fault() {
    let mut sources = world_with_a_set_read();
    sources.facts[0].request = Some("POST /v1/clusterprofiles/{uid}".to_string());
    // the other rules of the list find a fault in a request that the book and the rules lack
    let found = faults_of(&sources);
    assert!(
        found
            .iter()
            .any(|m| m.contains("must be a GET request that the contract run sends")),
        "{found:?}"
    );

    let mut sources = world_with_a_set_read();
    sources.set_reads[0].fact = "no-such-fact";
    assert_one_fault(&sources, "no fact `no-such-fact`");
}

#[test]
fn a_call_of_opt_set_that_the_table_of_reads_lacks_is_a_fault() {
    let mut sources = world_with_a_set_read();
    sources.cloud.code = BTreeMap::from([(
        "src/ops/list.rs".to_string(),
        "let a = opt_set(t, \"/type\", PROFILE_TEMPLATE);\nlet b = opt_set::<Scope>(t, \"/scope\", PROFILE_TEMPLATE);"
            .to_string(),
    )]);
    assert_one_fault(
        &sources,
        "the code of strata reads 2 sets with `opt_set`, and `reads()` of `xtask/src/facts/sets.rs` names 1",
    );
}

#[test]
fn each_read_of_the_code_of_strata_that_notes_an_unknown_value_has_its_list_in_a_get_fact() {
    let sources = Sources::read(&crate::repository(), None).0;
    assert_eq!(sources.set_reads.len(), 4);
    for read in &sources.set_reads {
        let fact = sources
            .facts
            .iter()
            .find(|f| f.id == read.fact)
            .unwrap_or_else(|| panic!("no fact `{}`", read.fact));
        let listed = &fact.reads.values[read.field];
        assert!(!listed.is_empty(), "{} at {}", read.set, read.field);
        assert_eq!(fact.reads.sets[read.field], read.set, "{}", fact.id);
    }
    let profile_type = sources
        .closed_sets
        .iter()
        .find(|s| s.name == "ProfileType")
        .unwrap();
    assert!(profile_type.values.contains(&"add-on".to_string()));
}

#[test]
fn the_lists_of_the_closed_sets_of_strata_in_the_facts_name_their_type() {
    let sources = Sources::read(&crate::repository(), None).0;
    let named = |id: &str, field: &str| {
        let fact = sources.facts.iter().find(|f| f.id == id).unwrap();
        assert!(
            fact.reads.values.contains_key(field),
            "{id}: no list at {field}"
        );
        fact.reads.sets.get(field).map(String::as_str)
    };
    // the five lists that the issue names, and the sixth that the code of strata notes
    assert_eq!(
        named("cluster-document", "status.state"),
        Some("ClusterState")
    );
    assert_eq!(named("pack-listing", "items[].spec.type"), Some("PackType"));
    assert_eq!(
        named("oci-registry-summary", "items[].spec.providerType"),
        Some("OciProvider")
    );
    assert_eq!(
        named("profile-document", "metadata.annotations.scope"),
        Some("ProfileScope")
    );
    assert_eq!(
        named("profile-document", "spec.published.type"),
        Some("ProfileType")
    );
}

// ---- the placeholders of a text ------------------------------------------------------------------

#[test]
fn the_pieces_of_a_text_are_its_text_its_names_its_alternatives_and_its_unread_groups() {
    use super::model::{Piece, pieces};
    assert_eq!(
        pieces("/v1/{aws,gcp}/regions/{region}/x?a={Account_1}&b={gcp-region}&c={"),
        [
            Piece::Text("/v1/"),
            Piece::Alternatives(vec!["aws", "gcp"]),
            Piece::Text("/regions/"),
            Piece::Name("region"),
            Piece::Text("/x?a="),
            Piece::Name("Account_1"),
            Piece::Text("&b="),
            Piece::Unread("{gcp-region}"),
            Piece::Text("&c="),
            Piece::Unread("{"),
        ]
    );
    assert_eq!(pieces("no placeholder"), [Piece::Text("no placeholder")]);
    assert_eq!(pieces("{}"), [Piece::Unread("{}")]);
    assert_eq!(
        pieces("{1st}{a,}"),
        [Piece::Unread("{1st}"), Piece::Unread("{a,}")]
    );
}

#[test]
fn one_reader_fills_the_names_of_a_request_and_of_a_call_with_the_same_result() {
    use super::model::{fill, names};
    // a capital letter was a name for the run of a request and none for the call of a cloud
    let text = "/v1/{Region}/{region}/{vpc_id}";
    assert_eq!(names(text), ["Region", "region", "vpc_id"]);
    let filled = fill(text, |name| Ok::<_, String>(name.to_uppercase())).unwrap();
    assert_eq!(filled, "/v1/REGION/REGION/VPC_ID");
    let refused = fill(text, |name| {
        if name == "region" {
            Err(name.to_string())
        } else {
            Ok("x".to_string())
        }
    });
    assert_eq!(refused, Err("region".to_string()));
}

#[test]
fn the_alternatives_of_a_request_expand_to_one_text_for_each_choice() {
    use super::model::expand;
    let texts = expand("GET /v1/clouds/{aws,gcp}/regions/{region}/types");
    assert_eq!(
        texts,
        [
            (
                vec!["aws"],
                "GET /v1/clouds/aws/regions/{region}/types".to_string()
            ),
            (
                vec!["gcp"],
                "GET /v1/clouds/gcp/regions/{region}/types".to_string()
            ),
        ]
    );
    assert_eq!(
        expand("GET /v1/x/{uid}"),
        [(Vec::<&str>::new(), "GET /v1/x/{uid}".to_string())]
    );
    assert_eq!(expand("/{a,b}/{c,d}").len(), 4);
}

#[test]
fn a_group_in_braces_that_no_reader_fills_is_a_fault() {
    for (call, group) in [
        ("{gcp-region}", "{gcp-region}"),
        ("--region={", "{"),
        ("{a,b}", "lists alternatives"),
    ] {
        let mut sources = world();
        sources.facts[0].kind = super::model::Kind::Cloud;
        sources.facts[0].request = None;
        sources.facts[0].call = vec!["aws".to_string(), call.to_string()];
        let found = faults_of(&sources);
        assert!(
            found
                .iter()
                .any(|m| m.contains(group) && m.contains("call")),
            "{call}: {found:#?}"
        );
    }
    let mut sources = world();
    sources.facts[0].request = Some("GET /v1/clusterprofiles/{uid}?a={bad-name}".to_string());
    let found = faults_of(&sources);
    assert!(
        found
            .iter()
            .any(|m| m.contains("the request holds `{bad-name}`")),
        "{found:#?}"
    );
    // a name with a capital letter and the alternatives of a request are read
    let mut sources = world();
    sources.facts[0].request = Some("GET /v1/{aws,gcp}/x/{Uid}".to_string());
    let found = faults_of(&sources);
    assert!(
        !found.iter().any(|m| m.contains("no reader fills it")),
        "{found:#?}"
    );
}
