//! The seed corpus of each byte target, and the checks that keep it useful.
//!
//! `just fuzz-seed` writes the seeds into `src/__fuzz__/<module>__<test>/corpus/`, where bolero
//! reads them. Each seed is a real input of the target: a file of `examples/`, a file of the
//! catalog of strata-preflight, a file or an output of the book, or a document that the library
//! builds from them. A file has the name `fingerprint` gives its bytes, so a second run changes no file.
//!
//! A seed makes a fuzzer start in the code, and not at the first byte that a parser refuses. The
//! check at the end of this file fails when the seeds of a target stop reaching its code, for
//! example after a change of a schema id.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use strata::api::{RawCluster, RawProfile};
use strata::ops;
use strata::snapshot::Snapshot;

use crate::support::{AT, cluster_files, fingerprint, profile_files, raw_profile, scratch, utf8};
use crate::{candidate, canon, mask, palette, preflight, record, reference};

mod catalog;

/// The most bytes of a seed that a file gives. A longer file is a poor start for a fuzzer: each
/// input of it costs more, and libFuzzer keeps its inputs short.
const SEED_LIMIT: usize = 8 * 1024;

/// A byte target: the name of its test, and what it does with one input.
struct Target {
    /// `<module>::<test>`, which bolero turns into the directory of the target.
    test: &'static str,
    /// Run the target on one input. True when the input reached the code that the target fuzzes.
    run: fn(&[u8]) -> bool,
    /// The seeds of the target.
    seeds: fn() -> Vec<Vec<u8>>,
}

/// Each target that reads bytes. The property tests of `properties.rs` build their input.
const TARGETS: [Target; 14] = [
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

/// A crash that a fuzzer found, or a bug that a review found, kept as a regression case: the
/// target, the name of the file in `crashes/`, and the input. Each case is a bug that has a fix
/// and a unit test in `src/` or in `preflight/src/`.
fn regressions() -> Vec<(&'static str, &'static str, Vec<u8>)> {
    let draft = "mask::a_draft_pack_keeps_no_secret_in_clear_and_nothing_else_changes";
    let palette =
        "palette::an_answer_of_palette_of_any_shape_gives_a_record_or_an_error_with_a_kind";
    let a_ref = "reference::the_text_of_a_ref_parses_as_a_ref_that_prints_the_same_text";
    let jq = "preflight::a_jq_program_compiles_and_runs_or_is_refused_with_a_kind";
    let world = "preflight::a_world_file_gives_the_document_of_a_run_or_is_refused_with_a_kind";
    // a program of 3000 levels: jaq reads each level with a call, and the stack ends first
    let deep = format!("{}1{}", "(".repeat(3000), ")".repeat(3000));
    // the same, with a keyword or a comment for each level, and no bracket that the guard counts
    let levels = |open: &str, middle: &str, close: &str| {
        format!("{}{middle}{}", open.repeat(3000), close.repeat(3000)).into_bytes()
    };
    vec![
        // The mask of a draft cut the values of a user at a byte that is not a `char` boundary,
        // for text that has a multi-byte letter where a variable of the profile has a dot.
        (
            draft,
            "wave-3-mask-slices-a-char",
            "adminPassword: \"{{ .spectroévar.x }}\"\nsync:\n  password: \"{{ .spectroévar.y }}\"\n"
                .into(),
        ),
        // The write of a field into a document of Palette panicked for a `spec` that is a number.
        (palette, "a-document-that-is-a-number", b"5".to_vec()),
        (
            palette,
            "a-spec-that-is-a-number",
            br#"{"metadata": {"uid": "c1"}, "spec": 5, "packs": [{"metadata": {"name": "pack"}, "spec": 5}]}"#
                .to_vec(),
        ),
        // A listing named its next page by the token of the page itself, and the walk never ended.
        (
            palette,
            "a-listing-that-names-its-own-page-as-the-next",
            br#"{"items": [{"metadata": {"name": "a", "uid": "u"}}], "listmeta": {"continue": "x"}}"#
                .to_vec(),
        ),
        // A ref that prints as the text of another ref: no registry, and a `/` in the name.
        (a_ref, "pack-ref-with-no-registry-and-a-slash", b"pack://x@1".to_vec()),
        (a_ref, "pack-ref-with-a-slash-in-the-name", b"pack:/xergroup:a/b@1".to_vec()),
        // A cluster ref with a slash after `@pending` was the uid `c1@pending`, and it printed as the
        // text of the update that waits for `c1`.
        (a_ref, "cluster-ref-with-a-slash-after-pending", b"cluster:c1@pending/".to_vec()),
        // A tag ref with no label was a set of clusters, and it printed as `tag:`, which is a uid.
        (a_ref, "tag-ref-with-no-label", b"tag:+".to_vec()),
        (jq, "a-program-of-3000-levels", deep.into_bytes()),
        (jq, "a-program-of-3000-tries", levels("try ", "[]", "")),
        (jq, "a-program-of-3000-ands", levels("true and ", "true", "")),
        (jq, "a-program-of-3000-ifs", levels("if . then ", "1", " else 2 end")),
        (jq, "a-program-of-3000-defs", levels("def f: ", ".", "; f")),
        (jq, "a-program-of-3000-comments", levels("(# )\n", "1", ")")),
        // The validate body of an existing cluster wrote its name into a `metadata` that is a number,
        // and the index of `serde_json` panicked.
        (
            world,
            "a-cluster-uid-and-a-metadata-that-is-a-number",
            br#"{"clusterUid": "c1", "entity": {"metadata": 5, "spec": {}}}"#.to_vec(),
        ),
    ]
}

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

/// Versions that the order must place, in the forms that Palette gives: a number, a number with a
/// suffix, a pre-release, build data, a name with a number, a date, and a version with no digit.
const VERSION_CASES: [&str; 24] = [
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

/// Cut each long string of `value` at the end of a line, so that a document of a real profile is
/// a short seed that still has the shape of the document.
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

/// The JSON file `name` of a raw directory of `examples/`, trimmed.
fn trimmed(directory: &str, name: &str) -> Value {
    let mut value: Value = serde_json::from_slice(&file(&format!("{directory}/{name}")))
        .unwrap_or_else(|e| panic!("{directory}/{name} is JSON: {e}"));
    trim(&mut value);
    value
}

/// The raw directories of the profile versions of `examples/`, trimmed.
fn real_profiles() -> Vec<RawProfile> {
    ["aws-stack@1.0.0", "aws-stack@1.1.0", "aws-stack@1.1.1"]
        .iter()
        .map(|version| {
            let dir = format!("examples/profiles/raw/{version}");
            RawProfile {
                profile: trimmed(&dir, "profile.json"),
                manifests: trimmed(&dir, "manifests.json"),
                variables: trimmed(&dir, "variables.json"),
                variables_unavailable: false,
                project_uid: None,
                fetched_at: None,
                manifest_entities: None,
                resolved_values: None,
            }
        })
        .collect()
}

/// The raw directory of the cluster of `examples/`, trimmed.
fn real_cluster() -> RawCluster {
    let dir = "examples/clusters/raw/workload-a";
    RawCluster {
        cluster: trimmed(dir, "cluster.json"),
        manifests: trimmed(dir, "manifests.json"),
        variables: trimmed(dir, "variables.json"),
        variables_unavailable: false,
        project_uid: None,
        fetched_at: None,
        notifications: None,
        repave: None,
        resolved_values: None,
        cloud_config: None,
        rbacs: None,
        namespaces: None,
        errors: strata::api::ReadErrors::default(),
    }
}

/// The records that the seeds build: one for each short YAML text as the values of one pack, and
/// the profile versions and the cluster of `examples/`.
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

/// The seeds of the comparison: the comparison of each record with the next one.
fn comparisons() -> Vec<Vec<u8>> {
    let records = built_records();
    records
        .windows(2)
        .map(|pair| {
            let diff = ops::compare(&pair[0], &pair[1], AT);
            serde_json::to_vec(&diff).expect("a comparison is JSON")
        })
        .collect()
}

/// The parts of a raw directory that `save` wrote, in the order of `names`, with a NUL byte
/// between them: the input of the target of that directory.
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

/// The seeds of the answers of Palette: the documents of a profile version and of a cluster, a
/// document that holds the fields of both, and the listings.
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

// ---- the files of the corpus ----------------------------------------------------------------

/// The directory that bolero reads for the target `test` and the kind of input `kind`: `corpus`
/// for the inputs that a fuzzer grows from, and `crashes` for a regression case.
fn directory(test: &str, kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/__fuzz__")
        .join(test.replace("::", "__"))
        .join(kind)
}

/// The inputs of the target `test` in `kind`.
fn inputs(test: &str, kind: &str) -> Vec<Vec<u8>> {
    fs::read_dir(directory(test, kind))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_file())
        .map(|entry| fs::read(entry.path()).expect("an input reads"))
        .collect()
}

/// Write `bytes` as `name` in `directory`.
fn put(directory: &Path, name: &str, bytes: &[u8]) {
    fs::create_dir_all(directory).expect("the directory of the corpus is made");
    fs::write(directory.join(name), bytes).expect("a seed is written");
}

/// Write the seeds and the regression cases. A seed that is empty or longer than the limit of a
/// seed is not kept, and a file that a fuzzer added stays.
#[test]
#[ignore = "writes the files of the corpus: run it with `just fuzz-seed`"]
fn write_the_seed_corpus() {
    for target in &TARGETS {
        let dir = directory(target.test, "corpus");
        for seed in (target.seeds)() {
            if seed.len() <= SEED_LIMIT * 4 {
                put(&dir, &format!("{:016x}", fingerprint(&seed)), &seed);
            }
        }
    }
    for (test, name, input) in regressions() {
        put(&directory(test, "crashes"), name, &input);
    }
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
    for (test, _, _) in regressions() {
        assert!(
            TARGETS.iter().any(|t| t.test == test),
            "{test} is no target"
        );
    }
}

/// The seeds of a target reach the code that it fuzzes, and the regression cases pass. A corpus
/// that a schema change made useless, or a directory that bolero does not read, fails here.
#[test]
fn the_seeds_of_each_target_reach_its_code_and_the_regression_cases_pass() {
    for target in &TARGETS {
        let corpus = inputs(target.test, "corpus");
        assert!(
            !corpus.is_empty(),
            "{} has no seed: run `just fuzz-seed`",
            target.test
        );
        let reached = corpus.iter().filter(|bytes| (target.run)(bytes)).count();
        assert!(
            reached * 3 >= corpus.len(),
            "only {reached} of {} seeds of {} reach its code",
            corpus.len(),
            target.test
        );
        for case in inputs(target.test, "crashes") {
            (target.run)(&case);
        }
    }
}

/// The book names each test that calls `bolero::check!`: each target and each property. A test
/// that the book leaves out is a test that a reader cannot find. The check reads the source of the
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
