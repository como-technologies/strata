//! The seed corpus of each byte target, and the checks that keep it useful.
//!
//! `just fuzz-seed` writes the seeds into `src/__fuzz__/<module>__<test>/corpus/`, where bolero
//! reads them. Each seed is a real input of the target. It is a file of `examples/`, a file of the
//! catalog of strata-preflight, or a file or an output of the book. It can also be a report that
//! the contract test of the observer writes. A seed can also be a document that the library
//! builds from them. A file has the name `fingerprint` gives its bytes, so a second run changes
//! no file. A seed names [`SEED_VERSION`] where a document names the version of strata.
//!
//! A seed makes a fuzzer start in the code, and not at the first byte that a parser refuses. A
//! check of [`corpus`] fails when the corpus does not hold a seed that this file gives. Another
//! fails when the seeds of a target no longer reach its code. A change of a schema id is one
//! example.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use strata::api::codes::{
    PACK_FORBIDDEN_CODE, PACK_MACRO_UNDEFINED_CODE, PROFILE_EXISTS_CODE,
    PROFILE_IMPORT_MATCH_CODES, PROJECT_SCOPE_CODE, ValidateRoute,
};
use strata::api::{RawCluster, RawProfile};
use strata::ops;
use strata::snapshot::{Snapshot, Tool};

use crate::routes::error_body;
use crate::support::{AT, cluster_files, profile_files, raw_profile, scratch, utf8};
use crate::{
    behind, candidate, canon, config, download, mask, palette, preflight, record, reference,
    report, unknown,
};

mod catalog;
mod corpus;
mod downloads;
mod regressions;
mod since;
mod state;

/// The most bytes of a seed that a file gives. A longer file is a poor start for a fuzzer: each
/// input of it costs more, and libFuzzer keeps its inputs short.
const SEED_LIMIT: usize = 8 * 1024;

/// The version of strata that a seed names. A document that strata writes names the version of
/// the build, and each release changes that version. A seed names this version in its place, so a
/// release makes no seed stale. It is the version of the build when the corpus got this check, so
/// the seeds of that time stay current.
const SEED_VERSION: &str = "0.32.2";

/// Give `tool` the version [`SEED_VERSION`].
fn pin_version(tool: &mut Tool) {
    tool.version = SEED_VERSION.to_string();
}

/// A byte target: the name of its test, and what it does with one input.
struct Target {
    /// `<module>::<test>`, which bolero turns into the directory of the target.
    test: &'static str,
    /// Run the target on one input. True when the input reached the code that the target fuzzes.
    run: fn(&[u8]) -> bool,
    /// The seeds of the target.
    seeds: fn() -> Vec<Vec<u8>>,
}

/// A regression case: the target, the name of the file in `crashes/`, and the input.
type Case = (&'static str, &'static str, Vec<u8>);

/// Each target that reads bytes. The property tests of `properties.rs` build their input.
const TARGETS: [Target; 21] = [
    Target {
        test: "candidate::a_candidate_file_loads_as_the_candidate_it_writes_or_is_refused_with_a_kind",
        run: candidate::file,
        seeds: candidate_files,
    },
    Target {
        test: "canon::any_text_has_a_stable_canonical_form_as_values_and_as_a_manifest",
        run: canon::values,
        seeds: yaml_texts,
    },
    Target {
        test: "download::a_download_of_a_profile_gives_the_archive_of_each_pack_or_is_refused_with_a_kind",
        run: download::file,
        seeds: downloads::files,
    },
    Target {
        test: "mask::a_draft_pack_keeps_no_secret_in_clear_and_nothing_else_changes",
        run: mask::draft,
        seeds: yaml_texts,
    },
    Target {
        test: "mask::a_saved_cluster_keeps_no_password_of_its_group_values_in_clear",
        run: mask::group,
        seeds: yaml_texts,
    },
    Target {
        test: "reference::the_text_of_a_ref_parses_as_a_ref_that_prints_the_same_text",
        run: reference::parse,
        seeds: refs,
    },
    Target {
        test: "reference::the_version_order_is_a_total_preorder",
        run: reference::order,
        seeds: versions,
    },
    Target {
        test: "record::a_record_file_holds_what_each_record_holds_or_is_refused_with_a_kind",
        run: record::snapshot,
        seeds: records,
    },
    Target {
        test: "record::a_comparison_writes_the_document_it_reads",
        run: record::comparison,
        seeds: comparisons,
    },
    Target {
        test: "record::a_raw_directory_of_a_profile_builds_a_record_or_is_refused_with_a_kind",
        run: record::profile_dir,
        seeds: profile_dirs,
    },
    Target {
        test: "record::a_raw_directory_of_a_cluster_builds_a_record_or_is_refused_with_a_kind",
        run: record::cluster_dir,
        seeds: cluster_dirs,
    },
    Target {
        test: "palette::an_answer_of_palette_of_any_shape_gives_a_record_or_an_error_with_a_kind",
        run: palette::answers,
        seeds: answers,
    },
    Target {
        test: "palette::an_error_of_palette_is_read_as_a_refusal_or_gives_an_error_with_a_kind",
        run: palette::errors,
        seeds: palette_errors,
    },
    Target {
        test: "config::a_lookup_of_settings_gives_a_config_that_hides_its_key_or_is_refused_with_a_kind",
        run: config::lookup,
        seeds: settings,
    },
    Target {
        test: "report::a_request_body_is_refused_with_a_status_or_is_a_report_that_writes_as_itself",
        run: report::body,
        seeds: state::report_bodies,
    },
    Target {
        test: "behind::a_behind_report_is_the_since_of_a_run_or_is_refused_with_a_kind",
        run: behind::since,
        seeds: since::reports,
    },
    Target {
        test: "unknown::a_log_of_unknown_values_gives_a_report_that_the_endpoint_takes_or_is_refused_with_a_kind",
        run: unknown::log,
        seeds: state::logs,
    },
    Target {
        test: "unknown::an_answer_file_gives_a_choice_that_it_keeps_and_only_the_yes_sends",
        run: unknown::answer_file,
        seeds: state::answers,
    },
    Target {
        test: "preflight::a_rule_file_loads_as_the_section_it_writes_or_is_refused_with_a_kind",
        run: preflight::rule_file,
        seeds: catalog::rule_files,
    },
    Target {
        test: "preflight::a_jq_program_compiles_and_runs_or_is_refused_with_a_kind",
        run: preflight::jq_program,
        seeds: catalog::jq_programs,
    },
    Target {
        test: "preflight::a_world_file_gives_the_document_of_a_run_or_is_refused_with_a_kind",
        run: preflight::world_file,
        seeds: catalog::world_files,
    },
];

// ---- the files of the repository ------------------------------------------------------------

/// The path `relative` in the repository.
fn repo(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

/// The bytes of the file `relative` of the repository.
fn file(relative: &str) -> Vec<u8> {
    fs::read(repo(relative)).unwrap_or_else(|e| panic!("the seed file {relative}: {e}"))
}

/// The bytes of the file `relative`, when it is short enough to be a seed.
fn short(relative: &str) -> Option<Vec<u8>> {
    Some(file(relative)).filter(|bytes| bytes.len() <= SEED_LIMIT)
}

/// The output `name` of the book without its first line, which is the command that wrote it.
fn book_output(name: &str) -> Vec<u8> {
    let bytes = file(&format!("docs/src/output/{name}.txt"));
    let start = bytes.iter().position(|&b| b == b'\n').map_or(0, |i| i + 1);
    bytes[start..].to_vec()
}

/// The texts that a case of YAML needs and no file of the repository has.
const YAML_CASES: [&str; 22] = [
    "adminPassword: s3cr3t\nreplicas: 3\n",
    "charts:\n  app:\n    adminPassword: pw\n    db:\n      password: hunter2\n      host: db\n",
    "dbPassword: \"{{ .spectro.var.db_password }}\"\nuser: fred\n",
    "replicas: {{ .spectro.var.replicas }}\n",
    "adminPassword: \"{{ .spectroévar.x }}\"\n",
    "a: 1\n---\nb: 2\n",
    "a: &x [1, 2]\nb: *x\nc: &y {k: v}\nd: *y\n",
    "password: |\n  line one\n  line two\nname: x\n",
    "password: >-\n  folded\n  text\n",
    "{password: x, list: [a, b], nested: {adminPassword: y}}",
    "p: !!str 12\nq: !!binary aGk=\nr: !custom tagged\n",
    "password: a\r\nname: b\r\n",
    "\"password\": 'x'\n'adminPassword': \"y\"\n",
    "password: 12345\nflag: true\nnothing: ~\n1: a\ntrue: b\nnull: c\n",
    "a.b: 1\na:\n  b: 2\n  c.d: 3\n",
    "? [a, b]\n: c\n",
    "- password: a\n- adminPassword: b\n  rootPassword: ''\n",
    "# only a comment\n",
    "",
    "---\n...\n",
    "key: \"\\u00e9\\U0001F600 \\x41\"\nname: ünï\n",
    "a: 1.0\nb: 1e3\nc: .inf\nd: -.inf\ne: .nan\nf: 0x1F\ng: 0o17\nh: 1_000\n",
];

/// The files of the repository that hold YAML that a pack can hold: values, manifests, and the
/// files of a workflow.
const YAML_FILES: [&str; 8] = [
    "examples/live/chart/strata-live-chart/values.yaml",
    "examples/live/chart/strata-live-chart/templates/configmap.yaml",
    "examples/live/pack/strata-live-pack/values.yaml",
    "examples/live/pack/strata-live-dep/values.yaml",
    "examples/live/pack/strata-live-pack/manifests/configmap.yaml",
    "examples/live/zarf/strata-live-zarf/zarf.yaml",
    "examples/live/preflight/lab-probes.yaml",
    ".github/workflows/session-image.yml",
];

/// The seeds of a target that reads the YAML text of values.
fn yaml_texts() -> Vec<Vec<u8>> {
    let mut seeds: Vec<Vec<u8>> = YAML_CASES.iter().map(|t| t.as_bytes().to_vec()).collect();
    seeds.extend(YAML_FILES.iter().filter_map(|f| short(f)));
    seeds.extend(["candidate-yaml", "candidate-preset-yaml"].map(book_output));
    seeds
}

// ---- the candidate --------------------------------------------------------------------------

/// A candidate with its content in the file, and the other shapes of file that the check refuses
/// or reads another way.
const CANDIDATE_CASES: [&str; 6] = [
    "name: demo\nversion: 1.0.0\ncloudType: aws\nregistry: Public Repo\nlayers:\n- name: kubernetes\n  \
     version: 1.36.2\n  layer: k8s\n  tag: 1.36.2\n  values: |\n    pack:\n      k8sHardening: true\n\
     - name: app\n  version: 1.0.0\n  manifests:\n  - name: cm\n    content: |\n      kind: ConfigMap\n",
    "name: demo\nversion: 1.0.0\nlayers:\n- name: cfg\n  version: 1.0.0\n  type: manifest\n  \
     manifests:\n  - name: cm\n    content: 'kind: ConfigMap'\n",
    "name: demo\nversion: 1.0.0\nregistry: a\nregistryUid: b\nlayers:\n- name: x\n  version: '1'\n",
    "name: ''\nversion: ''\nlayers: []\n",
    "{\"metadata\": {\"name\": \"exported\"}, \"spec\": {\"version\": \"1.0.0\", \"template\": {\"type\": \
     \"cluster\", \"cloudType\": \"aws\", \"packs\": [{\"name\": \"kubernetes\", \"type\": \
     \"spectro\", \"layer\": \"k8s\", \"version\": \"1.36.2\", \"tag\": \"1.36.2\", \"values\": \
     \"a: 1\", \"registry\": {\"metadata\": {\"name\": \"Public Repo\", \"uid\": \"r1\"}}}]}, \
     \"variables\": []}}",
    "name: demo\nversion: 1.0.0\nunknownField: true\nlayers:\n- name: x\n  version: '1'\n",
];

/// The seeds of the candidate file: the files of the book, and the cases.
fn candidate_files() -> Vec<Vec<u8>> {
    let mut seeds: Vec<Vec<u8>> = CANDIDATE_CASES
        .iter()
        .map(|t| t.as_bytes().to_vec())
        .collect();
    seeds.extend(
        [
            "candidate-yaml",
            "candidate-preset-yaml",
            "review-candidate",
            "nodigest-chart-file",
        ]
        .map(book_output),
    );
    seeds
}

// ---- refs and versions ----------------------------------------------------------------------

/// One ref of each shape that `Ref::parse` knows, and the texts that sit near a shape.
const REF_CASES: [&str; 42] = [
    "aws-stack@1.1.1",
    "aws-stack@latest",
    "64e2000000000000000000a1",
    "64e2000000000000000000a1@draft",
    "aws-stack@1.1.1@draft",
    "cluster:64f1000000000000000000e1",
    "cluster:64f1000000000000000000e1/aws-stack",
    "cluster:64f1000000000000000000e1@pending",
    "cluster:64f1000000000000000000e1/aws-stack@pending",
    "cluster:64f1000000000000000000e1@pending/",
    "cluster:",
    "cluster:/aws-stack",
    "edgehost:64f1000000000000000000e1/aws-stack",
    "edgehost:",
    "clustergroup:64f1000000000000000000e1",
    "clustergroup:a/b",
    "appprofile:shop@1.0.0",
    "appprofile:64f1000000000000000000e1",
    "appprofile:a/b",
    "pack:cert-manager@1.21.1",
    "pack:Public Repo/cert-manager@1.21.1",
    "pack:@1",
    "pack:/x@1",
    "tag:env=prod",
    "tag:env=prod+team=a",
    "tag:+",
    "tag:",
    "filter:prod-clusters",
    "filter:",
    "releases/1.1.1/dev.json",
    "./record.json",
    "../../etc/passwd",
    "C:\\records\\dev.json",
    "@",
    "@@",
    "a@",
    "@1",
    "a@b@c@draft",
    "@draft",
    "ß@İ",
    "日本語@🙂",
    "",
];

/// The seeds of the ref target: each case.
fn refs() -> Vec<Vec<u8>> {
    REF_CASES.iter().map(|t| t.as_bytes().to_vec()).collect()
}

/// Versions that the order must place, in the forms that Palette gives. The forms are a number, a
/// number with a suffix, a pre-release, and build data. They also include a name with a number, a
/// date, a version with no digit, and a tail after three numbers.
const VERSION_CASES: [&str; 27] = [
    "1.0.0",
    "1.0.0",
    "1.00.0",
    "1.9.0",
    "1.10.0",
    "1.30.6",
    "1.29.10",
    "22.04",
    "20.04",
    "LTS__24.x",
    "LTS__22.04",
    "1.0.0-rc1",
    "1.0.0-rc.2",
    "v1.2.3",
    "1.0.1-rc.1",
    "1.0.1-rc.1+5",
    "1.0.1-g0123456789ab",
    "1.0.1+2",
    "01.2.3",
    "1.2.3.4",
    "1.1.8.develop",
    "10.4.0.1",
    "chart-1.2.3",
    "2026-09-17",
    "latest",
    "x",
    "",
];

/// The seeds of the order: three versions in a row, from the versions of the packs of `examples/`
/// and from the cases.
fn versions() -> Vec<Vec<u8>> {
    let mut all: BTreeSet<String> = VERSION_CASES.iter().map(|v| v.to_string()).collect();
    for pack in fs::read_dir(repo("examples/packs"))
        .expect("examples/packs is there")
        .flatten()
    {
        for version in fs::read_dir(pack.path()).into_iter().flatten().flatten() {
            let name = version.file_name().to_string_lossy().into_owned();
            if let Some(v) = name.strip_suffix(".json").filter(|v| !v.starts_with('_')) {
                all.insert(v.to_string());
            }
        }
    }
    let all: Vec<String> = all.into_iter().collect();
    all.windows(3)
        .map(|w| format!("{}\0{}\0{}", w[0], w[1], w[2]).into_bytes())
        .collect()
}

// ---- records, comparisons, and raw directories ----------------------------------------------

/// The longest text that a string of a real document keeps in a seed.
const TRIM: usize = 700;

/// Cut each long string of `value` at the end of a line. So a document of a real profile is a
/// short seed that still has the shape of the document.
fn trim(value: &mut Value) {
    match value {
        Value::String(s) if s.len() > TRIM => {
            let mut end = TRIM;
            while !s.is_char_boundary(end) {
                end -= 1;
            }
            let cut = s[..end].rfind('\n').map_or(end, |i| i + 1);
            s.truncate(cut);
        }
        Value::Array(items) => items.iter_mut().for_each(trim),
        Value::Object(map) => map.values_mut().for_each(trim),
        _ => {}
    }
}

/// Read the raw directory of a profile or of a cluster of `examples/` with `load`, and trim each
/// document of it.
fn trimmed<T>(
    relative: &str,
    load: fn(&Path) -> strata::Result<T>,
    documents: fn(&mut T) -> [&mut Value; 3],
) -> T {
    let mut raw = load(&repo(relative)).unwrap_or_else(|e| panic!("{relative}: {e}"));
    documents(&mut raw).into_iter().for_each(trim);
    raw
}

/// The raw directories of the profile versions of `examples/`, trimmed.
fn real_profiles() -> Vec<RawProfile> {
    ["aws-stack@1.0.0", "aws-stack@1.1.0", "aws-stack@1.1.1"]
        .iter()
        .map(|version| {
            trimmed(
                &format!("examples/profiles/raw/{version}"),
                RawProfile::load_dir,
                |raw| [&mut raw.profile, &mut raw.manifests, &mut raw.variables],
            )
        })
        .collect()
}

/// The raw directory of the cluster of `examples/`, trimmed.
fn real_cluster() -> RawCluster {
    trimmed(
        "examples/clusters/raw/workload-a",
        RawCluster::load_dir,
        |raw| [&mut raw.cluster, &mut raw.manifests, &mut raw.variables],
    )
}

/// The records that the seeds build. They are one for each short YAML text as the values of one
/// pack, and the profile versions and the cluster of `examples/`. Each names [`SEED_VERSION`].
fn built_records() -> Vec<Snapshot> {
    let mut records: Vec<Snapshot> = yaml_texts()
        .iter()
        .filter_map(|bytes| utf8(bytes))
        .filter(|text| text.len() <= 2048)
        .filter_map(|text| Snapshot::from_raw(&raw_profile(text, text), "seed", AT).ok())
        .collect();
    for raw in real_profiles() {
        records.extend(Snapshot::from_raw(&raw, "seed", AT).ok());
    }
    records.extend(Snapshot::from_cluster(&real_cluster(), None, "seed", AT).ok());
    for record in &mut records {
        pin_version(&mut record.tool);
    }
    records
}

/// The seeds of the record file: each built record, as the JSON that strata writes.
fn records() -> Vec<Vec<u8>> {
    built_records()
        .iter()
        .map(|record| {
            record
                .to_json(false)
                .expect("a record is JSON")
                .into_bytes()
        })
        .collect()
}

/// The seeds of the comparison: the comparison of each record with the next one, which names
/// [`SEED_VERSION`].
fn comparisons() -> Vec<Vec<u8>> {
    let records = built_records();
    records
        .windows(2)
        .map(|pair| {
            let mut diff = ops::compare(&pair[0], &pair[1], AT);
            pin_version(&mut diff.tool);
            serde_json::to_vec(&diff).expect("a comparison is JSON")
        })
        .collect()
}

/// The parts of a raw directory that `save` wrote, in the order of `names`, with a NUL byte
/// between them. This is the input of the target of that directory.
fn directory_input(names: &[String], save: impl FnOnce(&Path)) -> Vec<u8> {
    let dir = scratch().join("seed");
    save(&dir);
    let mut input = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            input.push(0);
        }
        input.extend(fs::read(dir.join(name)).unwrap_or_default());
    }
    input
}

/// The seeds of the raw directory of a profile.
fn profile_dirs() -> Vec<Vec<u8>> {
    let mut raws = real_profiles();
    raws.extend(
        yaml_texts()
            .iter()
            .filter_map(|bytes| utf8(bytes))
            .filter(|text| text.len() <= 1024)
            .take(8)
            .map(|text| raw_profile(text, text)),
    );
    raws.iter()
        .map(|raw| {
            directory_input(profile_files(), |dir| {
                raw.save_dir(dir, AT).expect("a raw directory saves")
            })
        })
        .collect()
}

/// The seeds of the raw directory of a cluster.
fn cluster_dirs() -> Vec<Vec<u8>> {
    let raw = real_cluster();
    vec![directory_input(cluster_files(), |dir| {
        raw.save_dir(dir, AT).expect("a raw directory saves")
    })]
}

// ---- the answers of Palette -----------------------------------------------------------------

/// The seeds of the answers of Palette. They are the documents of a profile version and of a
/// cluster, a document that holds the fields of both, and the listings.
fn answers() -> Vec<Vec<u8>> {
    let profile = real_profiles().remove(2);
    let cluster = real_cluster();
    let mut both = profile.profile.clone();
    if let (Some(spec), Some(from)) = (
        both.get_mut("spec").and_then(Value::as_object_mut),
        cluster.cluster.get("spec").and_then(Value::as_object),
    ) {
        spec.extend(from.clone());
    }
    both["items"] = json!([{"metadata": {"name": "aws-stack", "uid": "u1"},
        "spec": {"version": "1.1.1", "cloudType": "aws", "type": "cluster"}}]);
    both["listmeta"] = json!({"count": 1});
    let documents = [
        profile.profile,
        profile.manifests,
        cluster.cluster,
        cluster.manifests,
        both,
        json!({"items": [], "listmeta": {"count": 0}}),
        json!([{"metadata": {"name": "c", "uid": "c1"}}]),
        json!({"packs": [{"metadata": {"name": "p"}}], "spec": 5}),
    ];
    documents
        .iter()
        .map(|d| serde_json::to_vec(d).expect("a document is JSON"))
        .collect()
}

// ---- the errors of Palette ------------------------------------------------------------------

/// An error of Palette as a validate route or another route gives it: the status, and the body.
/// A code that `strata::api::codes` names comes from there.
fn error_cases() -> Vec<(u16, String)> {
    vec![
        (
            403,
            error_body(PACK_FORBIDDEN_CODE, "the values of a pack are forbidden"),
        ),
        (
            403,
            error_body(
                PROJECT_SCOPE_CODE,
                "the operation is not allowed in the project scope",
            ),
        ),
        (
            409,
            error_body(PROFILE_EXISTS_CODE, "lab-profile 1.0.0 exists"),
        ),
        (
            404,
            error_body(
                PACK_MACRO_UNDEFINED_CODE,
                "macro spectro.var.x is not defined",
            ),
        ),
        (
            422,
            r#"{"code":422,"message":"list items must be unique"}"#.into(),
        ),
        (500, error_body("InternalError", "try again")),
        (
            500,
            r#"{"code":["PackYaml"],"message":"the code is a list"}"#.into(),
        ),
        (500, error_body("PackYaml", "é 日本語 🙂")),
        (502, "<html><body>Bad Gateway</body></html>".into()),
        (429, String::new()),
        (401, r#"{"code":"Unauthorized"}"#.into()),
        (200, r#"{"code":"NotAnError"}"#.into()),
        (0, "not json".into()),
    ]
}

/// The seeds of the error of Palette: the cases above, and each code of each validate route with
/// the status 500.
fn palette_errors() -> Vec<Vec<u8>> {
    let mut cases = error_cases();
    let codes: BTreeSet<&str> = ValidateRoute::ALL
        .into_iter()
        .flat_map(ValidateRoute::refusal_codes)
        .chain(PROFILE_IMPORT_MATCH_CODES)
        .collect();
    cases.extend(
        codes
            .into_iter()
            .map(|code| (500, error_body(code, &format!("{code} on the pack")))),
    );
    cases
        .iter()
        .map(|(status, body)| [&status.to_be_bytes()[..], body.as_bytes()].concat())
        .collect()
}

// ---- the settings of a connection -----------------------------------------------------------

/// The lines of a file `~/.palette.env`, or of an environment, that give a connection or refuse it.
const SETTINGS_CASES: [&str; 11] = [
    "PALETTE_API_KEY=key\n",
    "PALETTE_API_KEY=key\nPALETTE_API=api.example.com/\nPALETTE_PROJECT_UID=64f0000000000000000000c3\n",
    "SPECTROCLOUD_APIKEY=key\nSPECTROCLOUD_HOST=http://localhost:8080//\n",
    "PALETTE_API_KEY=first\nSPECTROCLOUD_APIKEY=second\n",
    "PALETTE_API_KEY=key\nPALETTE_PROJECT_UID=\nPALETTE_PROJECT=dev\n",
    "PALETTE_API_KEY=key\nPALETTE_PROJECT_UID=64f0000000000000000000c3\nPALETTE_PROJECT=dev\n",
    "PALETTE_API_KEY=key\nSTRATA_RETRIES=5\nSTRATA_MAX_RETRY_AFTER=60\n",
    "PALETTE_API_KEY=key\nSTRATA_RETRIES=many\n",
    "PALETTE_API_KEY=key\nSTRATA_MAX_RETRY_AFTER=-1\n",
    "PALETTE_API=api.example.com\n",
    "",
];

/// The seeds of the settings: the cases above.
fn settings() -> Vec<Vec<u8>> {
    SETTINGS_CASES
        .iter()
        .map(|t| t.as_bytes().to_vec())
        .collect()
}

/// The modules of the targets are the files of this crate, and each target is a test of its module.
/// A name in `TARGETS` that no test has sends its seeds to a directory that no test reads.
#[test]
fn each_target_names_a_test_of_its_module() {
    for target in &TARGETS {
        let (module, test) = target.test.split_once("::").expect("module::test");
        let source = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("src/{module}.rs")),
        )
        .unwrap_or_else(|e| panic!("{module}: {e}"));
        assert!(
            source.contains(&format!("\nfn {test}() {{")),
            "{module}.rs has no test {test}"
        );
    }
    for (test, _, _) in regressions::cases() {
        assert!(
            TARGETS.iter().any(|t| t.test == test),
            "{test} is no target"
        );
    }
}

/// The book names each test that calls `bolero::check!`: each target and each property. A test
/// that the book omits is a test that a reader cannot find. The check reads the source of the
/// modules, and the page "The fuzzing" of the book.
#[test]
fn the_book_names_each_target_and_each_property() {
    let book = fs::read_to_string(repo("docs/src/development/build-test.md"))
        .expect("the page of the book reads");
    let sources = fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("src")).expect("src");
    let mut named = 0;
    for entry in sources.flatten().filter(|e| e.path().is_file()) {
        let path = entry.path();
        let module = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("a module name");
        // this module names `bolero::check!` in the text of its checks, and holds no target
        if module == "seed" {
            continue;
        }
        let source = fs::read_to_string(&path).expect("a module reads");
        for test in source.split("\n#[test]\nfn ").skip(1) {
            let (name, rest) = test.split_once("()").expect("a test has no arguments");
            if rest
                .split("\n}\n")
                .next()
                .is_some_and(|body| body.contains("bolero::check!"))
            {
                assert!(
                    book.contains(&format!("`{module}::{name}`")),
                    "the book does not name `{module}::{name}`"
                );
                named += 1;
            }
        }
    }
    assert!(named >= TARGETS.len(), "the check found {named} tests");
}
