//! The chapters of the command pages.

use super::DEV_ENV;
use super::edits::{LATER_PROFILE_TYPE, RETAIN};
use super::ids::{
    A100, A110, A111, A111_PROD, A111_TENANT, A112, A130, A1112, AWS_STANDARD, DEV, DEVELOP_118,
    DEVELOP_1110, DEVELOP_BUILD, DEVELOP_SHORT, LEGACY, PAYMENTS, PAYMENTS_PROD, PROD, SEARCH,
};
use crate::book::Runner;
use crate::common_palette::{access_denied_message, project_denied_message};
use crate::tenant::{CLUSTER_LISTING, Edit, TAG_FILTER_LISTING, TENANT, profile_exists_message};
use serde_json::json;
use strata::api::PROJECT_UID_HEADER;
use strata::api::codes::{Denial, PROFILE_EXISTS_CODE};
use strata::closed_set::ClosedSet;
use strata::model::cluster_state::ClusterState;

/// Run the chapters of the command pages.
pub(super) fn run(r: &mut Runner) {
    list(r);
    list_denied(r);
    unknown_value(r);
    scope(r);
    snapshot(r);
    latest_develop(r);
    candidate(r);
    compare(r);
    schema(r);
    verify_objects(r);
}

/// The chapter `list`: the listings of profiles, clusters, packs, and consumers.
fn list(r: &mut Runner) {
    r.chapter("list", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    // Palette lists the registries in each scope, and a pack row names its registry from them
    for scope in [DEV, PROD, TENANT] {
        r.tenant(|t| t.registries(scope));
    }
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile(PROD, A111_PROD, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A110], &[]));
    r.tenant(|t| t.cluster(DEV, SEARCH, "search", &[A111], &[]));
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A111_PROD], &[]));
    r.step("list", 0, false, "strata list");
    r.step("list-projects", 0, false, "strata list projects");
    r.step("list-prod", 0, false, "strata list --project-name prod");
    r.step("list-tenant", 0, false, "strata list clusters --project ''");
    r.step("list-applied", 0, false, "strata list clusters --applied");
    r.step("list-cluster", 0, false, "strata list --cluster payments");
    r.step(
        "list-cluster-tenant",
        0,
        false,
        "strata list --cluster payments --project ''",
    );
    r.step("list-pack", 0, false, "strata list --pack cilium");
    r.step(
        "list-pack-all",
        0,
        false,
        r#"for p in dev prod; do strata list --pack cilium --project-name "$p"; done"#,
    );
    r.step(
        "list-pack-tenant",
        0,
        false,
        "strata list --pack cilium --project ''",
    );
    r.step("list-packs", 0, false, "strata list --packs");
    r.step(
        "list-pack-consumers",
        1,
        false,
        "for v in 1.0.0 1.1.0 1.1.1; do strata consumers aws-stack@$v; done",
    );
    // The empty uid is the point of the step: a reader who exported one needs it. The book must
    // print the command that works for them, not the one that works only here
    r.step(
        "list-pack-consumers",
        1,
        false,
        "PALETTE_PROJECT_UID= PALETTE_PROJECT=prod strata consumers aws-stack@1.1.1",
    );
    r.step("list-pack-json", 0, false, r#"strata list --pack cilium --json | jq -r '.profiles[] | .ref as $ref | .packs[] | select(.name | contains("cilium")) | "\($ref) \(.name) \(.version)"'"#);
    list_edge_cases(r);
}

/// The chapter `list-denied`: a key whose roles do not give each read. Its role in the project
/// gives the profiles and not the clusters. No role of its user gives the saved tag filters.
fn list_denied(r: &mut Runner) {
    r.chapter("list-denied", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    // the answer of Palette to a role in the project without cluster.list
    let project = project_denied_message("dev");
    r.tenant(|t| t.deny(DEV, CLUSTER_LISTING, Denial::Project, &project));
    r.step("list-denied", 2, false, "strata list");
    // the answer of Palette to a user with no role that gives filter.list
    let filters = access_denied_message("filter.list", "tenant");
    r.tenant(|t| t.deny(TENANT, TAG_FILTER_LISTING, Denial::Route, &filters));
    r.step("list-denied-filters", 2, true, "strata list filters");
    // the error document gives the words of Palette in its message, and the body as Palette sent
    // it (#632); the page of the errors shows it
    r.step(
        "list-denied-filters-json",
        2,
        true,
        "strata list filters --json --quiet",
    );
}

/// The chapter `unknown-value`: a read that gives a value which this release of strata does not
/// know. The profile has a profile type that a later Palette release could add.
fn unknown_value(r: &mut Runner) {
    r.chapter("unknown-value", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A1112, "1.1.12", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.edit(DEV, A1112, LATER_PROFILE_TYPE));
    r.step(
        "unknown-value-note",
        0,
        true,
        "strata list profiles --packs > /dev/null",
    );
    r.step(
        "unknown-value-quiet",
        0,
        true,
        "strata list profiles --packs --quiet > /dev/null",
    );
    // the two runs above wrote two lines to the log of the home of the runner
    r.step("unknown-value-report", 0, false, "strata report");
    r.step(
        "unknown-value-report-markdown",
        0,
        false,
        "strata report --format markdown",
    );
    r.step(
        "unknown-value-report-json",
        0,
        false,
        "strata report --json | jq -c '.unknown[]'",
    );
    // the link is one long line: the page shows its start
    r.step(
        "unknown-value-report-link",
        0,
        false,
        "strata report --link | cut -c 1-150",
    );
    r.step(
        "unknown-value-report-sent",
        0,
        false,
        "strata report --sent > /dev/null && strata report",
    );
    r.step("unknown-value-report-all", 0, false, "strata report --all");
    // the runner gives each step STRATA_REPORT_URL, the stand-in for the endpoint of the report
    r.step(
        "unknown-value-send",
        0,
        true,
        "STRATA_REPORT=1 strata list profiles --packs > /dev/null",
    );
    // the send marked the line that it sent, so the report holds nothing new
    r.step("unknown-value-send-marked", 0, false, "strata report");
    // a person edits the log: the value of each line gets a space
    let log = "$HOME/.local/state/strata/strata-unknown.jsonl";
    r.quiet(&format!(
        "sed 's/later-type/later type/' {log} > {log}.edited && mv {log}.edited {log}"
    ));
    r.step("unknown-value-report-invalid", 2, true, "strata report");
}

/// The part of the chapter `list` for a draft, an imported cluster, and a cluster template.
fn list_edge_cases(r: &mut Runner) {
    // a version that nobody published: its packs are the packs of its draft
    r.unpublished(DEV, A130, "1.3.0", "aws-stack@1.1.1", &[]);
    r.step("list-draft", 0, false, "strata list profiles --packs");
    r.step(
        "list-draft-json",
        0,
        false,
        r#"strata list profiles --packs --json | jq -r '.profiles[] | "\(.ref) published=\(.published)"'"#,
    );
    // an imported cluster in read-only mode: Palette lists it with an import type and the cloud
    // generic, and applies no add-on profile to it
    r.tenant(|t| t.cluster(DEV, LEGACY, "legacy", &[], &[]));
    r.tenant(|t| t.cluster_field(DEV, LEGACY, "/spec/clusterType", json!("AlloyMonitor")));
    r.tenant(|t| t.cluster_field(DEV, LEGACY, "/spec/cloudType", json!("generic")));
    r.step("list-imported", 0, false, "strata list clusters --applied");
    r.step(
        "list-imported-json",
        0,
        false,
        r#"strata list clusters --json | jq -r '.clusters[] | "\(.name) \(.clusterType)"'"#,
    );
    // a cluster template links a version that no cluster runs: the version has a consumer
    r.tenant(|t| t.cluster_template(DEV, AWS_STANDARD, "aws-standard", &[A100]));
    r.step(
        "list-consumers-full",
        0,
        false,
        "strata list profiles --packs",
    );
    r.excerpt("list-consumers", "list-consumers-full", r"^  aws-stack@");
    r.step(
        "list-consumers-json",
        0,
        false,
        r#"strata list profiles --packs --json | jq -c '.profiles[] | {ref, consumers, consumersByKind}'"#,
    );
    r.step(
        "list-consumers-same",
        1,
        false,
        "strata consumers aws-stack@1.0.0 --list-only",
    );
}

/// The chapter `scope`: a profile of the tenant that has the name and the version of a profile of a
/// project.
fn scope(r: &mut Runner) {
    // a profile of the tenant with the name and the version of a profile of the project
    r.chapter("scope", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant_profile(A111_TENANT, "1.1.1", "aws-stack@1.1.1", &[]);
    r.step("scope-list", 0, false, "strata list profiles");
    r.step(
        "scope-list-json",
        0,
        false,
        r#"strata list profiles --json | jq -r '.profiles[] | "\(.ref) \(.uid) \(.scope)"'"#,
    );
    r.step(
        "scope-ambiguous",
        2,
        true,
        "strata snapshot aws-stack@1.1.1 -o aws-stack.json",
    );
    r.step(
        "scope-by-uid",
        0,
        true,
        &format!("strata snapshot {A111_TENANT} -o tenant-aws-stack.json"),
    );
    r.step("scope-packs", 0, false, "strata list profiles --packs");
    r.step(
        "scope-tenant",
        0,
        false,
        "strata list profiles --project ''",
    );
}

/// The chapter `snapshot`: the record of a profile, a cluster, and the raw files.
fn snapshot(r: &mut Runner) {
    r.chapter("snapshot", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.step(
        "snapshot-profile",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json",
    );
    r.step("snapshot-record", 0, false, r#"jq '{root, source: (.source | {ref, uid, projectUid, name, version, template, fetchedAt}), packs: [.packs[] | {key, version, node: .hash.node}]}' releases/1.1.1/dev.json"#);
    // a ref with no version reads as a uid. When it is a profile name, the error gives its versions
    // and the ref with latest (#632); the refs page shows it
    r.step("refs-name-no-version", 2, true, "strata snapshot aws-stack");
    // What a ref that looks like a file gets when no such file is there; the refs page shows it.
    r.step(
        "refs-missing-file",
        2,
        true,
        "strata verify releases/1.1.1/missing.json",
    );
    r.step(
        "snapshot-latest",
        0,
        true,
        "strata snapshot aws-stack@latest -o latest.json",
    );
    r.step(
        "snapshot-latest-version",
        0,
        false,
        r#"jq -r '.source | "\(.name)@\(.version)"' latest.json"#,
    );
    r.step(
        "snapshot-cluster",
        0,
        true,
        "strata snapshot cluster:payments -o runs/payments.json",
    );
    r.step(
        "snapshot-cluster-source",
        0,
        false,
        "jq .source.cluster runs/payments.json",
    );
    r.step(
        "snapshot-raw",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 --raw-dir raw/aws-stack-1.1.1 -o online.json",
    );
    r.step("snapshot-raw", 0, false, "ls raw/aws-stack-1.1.1");
    r.step(
        "snapshot-raw",
        0,
        true,
        "strata snapshot --from-raw raw/aws-stack-1.1.1 -o offline.json",
    );
    r.step(
        "snapshot-raw",
        0,
        false,
        "jq -r .root online.json offline.json",
    );
}

/// The chapter `latest-develop`: a develop build beside two releases, as a field test found it
/// (#616). Palette reads `1.1.8.develop` as `1.1.8`. Strata puts it below `1.1.8`, and `@latest`
/// skips it. `1.2.develop` holds no three numbers. Strata reads it as a pre-release of `1.2.0`,
/// above `1.1.10`, and `@latest` skips it too (#714).
fn latest_develop(r: &mut Runner) {
    r.chapter("latest-develop", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, DEVELOP_118, "1.1.8", "aws-stack@1.1.1", &[]);
    r.profile(DEV, DEVELOP_BUILD, "1.1.8.develop", "aws-stack@1.1.1", &[]);
    r.profile(DEV, DEVELOP_1110, "1.1.10", "aws-stack@1.1.1", &[]);
    r.profile(DEV, DEVELOP_SHORT, "1.2.develop", "aws-stack@1.1.1", &[]);
    r.step("latest-develop-list", 0, false, "strata list profiles");
    r.step(
        "latest-develop",
        0,
        true,
        "strata snapshot aws-stack@latest -o latest.json",
    );
    r.step(
        "latest-develop-version",
        0,
        false,
        r#"jq -r '.source | "\(.name)@\(.version)"' latest.json"#,
    );
}

/// The chapter `candidate`: the file that holds a profile as a candidate, and its record.
fn candidate(r: &mut Runner) {
    r.chapter("candidate", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.step(
        "candidate-write",
        0,
        true,
        "strata candidate aws-stack@1.1.1 -o aws-stack.yaml --files-dir aws-stack",
    );
    r.step("candidate-yaml", 0, false, "cat aws-stack.yaml");
    r.step("candidate-files", 0, false, "find aws-stack -type f | sort");
    r.step(
        "candidate-snapshot",
        0,
        true,
        "strata snapshot --from-candidate aws-stack.yaml -o candidate.json",
    );
    r.step(
        "candidate-published",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 -o published.json",
    );
    r.step(
        "candidate-same",
        0,
        false,
        "strata same-release candidate.json published.json",
    );
    candidate_export(r);
    candidate_validate_names(r);
    candidate_validate_packs(r);
    candidate_validate_disabled(r);
    candidate_validate_snapshot(r);
    candidate_presets(r);
    candidate_layers(r);
}

/// The part of the chapter `candidate` that reads the export of Palette as a candidate.
fn candidate_export(r: &mut Runner) {
    // The export of Palette is a candidate too. The export route gives the file to a request that
    // accepts a file, and its record has the root of the version (#156)
    r.tenant(|t| t.export(DEV, A111));
    r.quiet(&format!(
        "curl -sS -H 'Accept: application/octet-stream' -H '{PROJECT_UID_HEADER}: {DEV}' \
         \"$PALETTE_API/v1/clusterprofiles/{A111}/export?format=json\" -o aws-stack-export.json"
    ));
    r.step(
        "candidate-export",
        0,
        false,
        "jq -c '.spec.template.packs[] | {name, type, layer, version, tag, registry: .registry.metadata.name}' aws-stack-export.json",
    );
    r.step(
        "candidate-export-record",
        0,
        true,
        "strata snapshot --from-candidate aws-stack-export.json -o from-export.json",
    );
    r.step(
        "candidate-export-same",
        0,
        false,
        "strata same-release from-export.json published.json",
    );
}

/// The part of the chapter `candidate` where the routes of Palette refuse a name and a version.
fn candidate_validate_names(r: &mut Runner) {
    // Palette's own checks before a create (#273). The candidate comes from aws-stack@1.1.1, which
    // the project holds, so the name route and the import validate route refuse it
    r.tenant(|t| t.profile_packs_answer(DEV, None, None));
    let message = profile_exists_message("aws-stack", "1.1.1");
    let exists = (409, PROFILE_EXISTS_CODE, message.as_str());
    r.tenant(|t| t.profile_create_checks(DEV, ("aws-stack", "1.1.1"), Some(exists), Some(exists)));
    r.step(
        "candidate-validate-exists",
        2,
        true,
        "strata snapshot --from-candidate aws-stack.yaml --validate -o validated.json",
    );
    // a version that Palette does not parse: both routes refuse it, and strata prints it once
    r.quiet("sed 's/^version: 1.1.1$/version: 1.1.1.1/' aws-stack.yaml > bad-version.yaml");
    let unparsed = (
        500,
        "VersionParseFailure",
        "Unable to parse the version '1.1.1.1'",
    );
    r.tenant(|t| {
        t.profile_create_checks(
            DEV,
            ("aws-stack", "1.1.1.1"),
            Some(unparsed),
            Some(unparsed),
        )
    });
    r.step(
        "candidate-validate-version",
        2,
        true,
        "strata snapshot --from-candidate bad-version.yaml --validate -o bad-version.json",
    );
}

/// The part of the chapter `candidate` where the check of the packs refuses a new version, and then
/// accepts.
fn candidate_validate_packs(r: &mut Runner) {
    // a new version: Palette takes the name, and the check of the packs refuses, then accepts
    r.quiet(
        "sed 's/^version: 1.1.1$/version: 1.2.0/' aws-stack.yaml > next.yaml && mv next.yaml aws-stack.yaml",
    );
    r.tenant(|t| t.profile_create_checks(DEV, ("aws-stack", "1.2.0"), None, None));
    r.tenant(|t| {
        t.profile_packs_answer(DEV, None, Some(("cni-cilium-oss", "kubernetes", "1.37.0")))
    });
    r.step(
        "candidate-validate-refuses",
        2,
        true,
        "strata snapshot --from-candidate aws-stack.yaml --validate -o validated.json",
    );
    r.step(
        "candidate-validate-refuses",
        0,
        false,
        "test -e validated.json || echo no record",
    );
    r.tenant(|t| t.profile_packs_answer(DEV, None, None));
    r.step(
        "candidate-validate-accepts",
        0,
        true,
        "strata snapshot --from-candidate aws-stack.yaml --validate -o validated.json",
    );
    r.step(
        "candidate-validate-accepts",
        0,
        false,
        "jq -c .source.paletteValidation validated.json",
    );
}

/// The part of the chapter `candidate` where a disabled pack makes the import check refuse.
fn candidate_validate_disabled(r: &mut Runner) {
    // Spectro Cloud marks a version that a profile must not hold. Palette serves it, and the import
    // validate route refuses a candidate that holds it (#405)
    r.tenant(|t| t.mark_pack_disabled(DEV, "cert-manager", "1.21.1"));
    let disabled = (
        500,
        "DisabledPacksForbidden",
        "Disabled packs 'cert-manager' are forbidden",
    );
    r.tenant(|t| t.profile_create_checks(DEV, ("aws-stack", "1.2.0"), None, Some(disabled)));
    r.step(
        "candidate-validate-disabled",
        2,
        true,
        "strata snapshot --from-candidate aws-stack.yaml --validate -o validated.json",
    );
    r.tenant(|t| t.registries(DEV));
    r.tenant(|t| t.profile_create_checks(DEV, ("aws-stack", "1.2.0"), None, None));
}

/// The part of the chapter `candidate` that checks a version which Palette holds.
fn candidate_validate_snapshot(r: &mut Runner) {
    // a version that Palette holds, as an update of itself
    r.tenant(|t| t.profile_packs_answer(DEV, Some(A111), None));
    r.step(
        "snapshot-validate-version",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 --validate -o checked.json",
    );
    // with no -o, the record goes to standard output, and the answer follows it
    r.step(
        "snapshot-validate-stdout",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 --validate",
    );
    r.excerpt(
        "snapshot-validate-stdout-lines",
        "snapshot-validate-stdout",
        r"^(strata|\{|\})",
    );
    r.step(
        "snapshot-validate-usage",
        2,
        true,
        "strata snapshot --from-raw raw/aws-stack-1.1.1 --validate",
    );
}

/// The part of the chapter `candidate` for the options of a pack template.
fn candidate_presets(r: &mut Runner) {
    // a template on cert-manager 1.21.1 with one input parameter and one option, as the listing
    // gives it: the option has no presets
    r.tenant(|t| {
        t.pack_template(
            DEV,
            "cert-manager",
            "1.21.1",
            &json!({"parameters": {"inputParameters": [{
                "name": "access", "displayName": "Access", "type": "string",
                "targetKey": "pack.access",
                "options": {"public": {"label": "Direct", "dependencies": []}},
            }]}}),
        )
    });
    r.write_file("certs.yaml", PRESET_CANDIDATE_YAML);
    r.step("candidate-preset-yaml", 0, false, "cat certs.yaml");
    r.step(
        "candidate-preset-warning",
        0,
        true,
        "strata snapshot --from-candidate certs.yaml -o certs.json",
    );
    r.step(
        "candidate-preset-options",
        0,
        false,
        "jq -c '.packs[0].presetOptions' certs.json",
    );
}

/// The part of the chapter `candidate` where strata refuses a candidate before any request.
fn candidate_layers(r: &mut Runner) {
    // Palette refuses an os layer in an add-on profile only at the create, so strata refuses it
    // before any request
    r.quiet("sed 's/^type: cluster$/type: add-on/' aws-stack.yaml > add-on.yaml");
    r.step(
        "candidate-refused",
        2,
        true,
        "strata snapshot --from-candidate add-on.yaml -o add-on.json",
    );
    // Palette takes the layer of a pack from its registry, so strata refuses another layer
    r.quiet("sed 's/^  layer: addon$/  layer: cni/' aws-stack.yaml > wrong-layer.yaml");
    r.step(
        "candidate-layer-refused",
        2,
        true,
        "strata snapshot --from-candidate wrong-layer.yaml -o wrong-layer.json",
    );
}

/// The chapter `compare`: the comparison of two versions, with the formats of its report.
fn compare(r: &mut Runner) {
    r.chapter("compare", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile(DEV, A112, "1.1.2", "aws-stack@1.1.1", RETAIN);
    r.step(
        "compare-identical",
        0,
        false,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.1",
    );
    r.step(
        "compare-repave",
        1,
        false,
        "strata compare aws-stack@1.0.0 aws-stack@1.1.0",
    );
    r.step(
        "compare-images-count",
        1,
        false,
        "strata compare aws-stack@1.0.0 aws-stack@1.1.0 --json | jq -c '(.summary | {imagesChanged, chartsChanged}), [.packs[] | select(.images or .charts) | .name]'",
    );
    r.step(
        "compare-values",
        1,
        false,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.2",
    );
    r.step(
        "compare-full",
        1,
        false,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.2 --full",
    );
    // the JSON document holds the text of --full in `unified`: the page shows the first lines
    r.step(
        "compare-full-json",
        1,
        false,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.2 --full --json | jq -c '.packs[] | select(.values.unified) | {name, unified: (.values.unified | split(\"\\n\")[:4])}'",
    );
    r.step(
        "compare-markdown",
        1,
        false,
        "strata compare aws-stack@1.0.0 aws-stack@1.1.0 --format markdown",
    );
    r.step(
        "compare-json",
        1,
        false,
        "strata compare aws-stack@1.0.0 aws-stack@1.1.0 --json | jq .summary",
    );
}

/// The chapter `schema`: the JSON schemas that strata prints.
fn schema(r: &mut Runner) {
    r.chapter("schema", &[]);
    r.step(
        "schema-diff",
        0,
        false,
        r#"strata schema diff | jq -r '.required | join(", ")'"#,
    );
    r.step(
        "schema-variable",
        0,
        false,
        r#"strata schema snapshot | jq -c '.properties.variables.items.properties | keys'"#,
    );
    r.step(
        "schema-segment",
        0,
        false,
        r#"strata schema diff | jq -c '."$defs".Leaf.properties.segments.items.anyOf | map(.type)'"#,
    );
    r.step(
        "schema-tiers",
        0,
        false,
        r#"strata schema snapshot | jq -c '."$defs".ClusterSource.properties.tiers.properties | keys'"#,
    );
    r.step(
        "schema-read-errors",
        0,
        false,
        r#"strata schema snapshot | jq -c '."$defs".ClusterRead.oneOf | map(.const)'"#,
    );
    r.step(
        "schema-time",
        0,
        false,
        r#"strata schema snapshot | jq -r '."$defs".Source.properties.fetchedAt.format'"#,
    );
}

/// What `verify` says when the object of a record changed its state, went, or took another name.
fn verify_objects(r: &mut Runner) {
    r.chapter("verify-objects", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.quiet("strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json");
    r.quiet("strata snapshot cluster:payments -o runs/payments.json");
    // the cluster goes: it runs the same copy, and the verdict says the state
    r.tenant(|t| {
        t.cluster_field(
            DEV,
            PAYMENTS,
            "/status/state",
            json!(ClusterState::Deleting.as_str()),
        )
    });
    r.step(
        "verify-state-full",
        0,
        false,
        "strata verify runs/payments.json",
    );
    r.excerpt("verify-state", "verify-state-full", r"^OK:");
    // Palette deletes the cluster, and keeps its document
    r.tenant(|t| t.delete_cluster(DEV, PAYMENTS, "2026-09-17T11:05:00Z"));
    r.step(
        "verify-deleted-full",
        1,
        false,
        "strata verify runs/payments.json",
    );
    r.excerpt("verify-deleted", "verify-deleted-full", r"^DRIFT:");
    // a rename keeps the uid and the version of the profile
    r.tenant(|t| t.edit(DEV, A111, &[Edit::Name("aws-stack-legacy")]));
    r.step(
        "verify-renamed-full",
        1,
        false,
        "strata verify releases/1.1.1/dev.json",
    );
    r.excerpt("verify-renamed", "verify-renamed-full", r"^DRIFT:");
    r.step(
        "verify-objects-bulk",
        1,
        false,
        "strata verify releases/1.1.1/dev.json runs/payments.json",
    );
}

/// An add-on candidate whose values select the option `public` of the template of cert-manager.
const PRESET_CANDIDATE_YAML: &str = "\
name: certs
version: 1.0.0
type: add-on
registry: Public Repo
layers:
  - name: cert-manager
    version: 1.21.1
    values: |
      pack:
        namespace: cert-manager
        access: public
";
