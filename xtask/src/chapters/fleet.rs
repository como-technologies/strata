//! The chapters of the fleet, the registries, and the retire of a version.

use super::edits::{OCI_CSI, RETAIN};
use super::ids::{
    A100, A110, A110_PROD, A111, A111_PROD, A120_RC, A130_PROD, A200_TENANT, A1111, A1118, A1120,
    AWS_STANDARD, DEV, LEDGER, PAYMENTS, PAYMENTS_PROD, PROD, PRODUCTION, SEARCH,
};
use super::{DEV_ENV, payments_addons, payments_cluster};
use crate::book::Runner;
use crate::tenant::{Edit, UpgradeRefusal};
use serde_json::json;

/// Run the chapters of the fleet, the registries, and the retire of a version.
pub(super) fn run(r: &mut Runner) {
    behind(r);
    behind_profiles(r);
    fleet(r);
    same_names(r);
    projects(r);
    packs(r);
    palette_latest(r);
    behind_pre_release(r);
    retire(r);
}

/// The chapter `behind`: the pins that are behind the registry, and a version that arrives between
/// two runs.
fn behind(r: &mut Runner) {
    // The pins that are behind the registry, and a version that arrives between two runs
    r.chapter("behind", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.hide_pack_version(DEV, "csi-aws-ebs", "1.65.0"));
    r.serve_here(); // state 1: the registry has not published csi-aws-ebs 1.65.0 yet
    r.quiet("strata snapshot aws-stack@1.0.0 -o releases/1.0.0/dev.json");
    r.step(
        "behind-last-week",
        0,
        false,
        "strata behind releases/1.0.0/dev.json --no-impact --json > behind.json; jq -c '.pins[] | select(.name == \"csi-aws-ebs\") | {version, newer: [.newer[].version]}' behind.json",
    );
    r.tenant(|t| t.registries(DEV));
    // the newest kubernetes supports amd64 alone, as loki 3.6.7 does in the Public Repo
    r.tenant(|t| t.pack_architectures(DEV, "kubernetes", "1.36.2", "amd64"));
    // Palette accepts each move of the two versions: no pack declares a limit
    r.tenant(|t| {
        t.profile_upgrade_answer(DEV, A100, None);
        t.profile_upgrade_answer(DEV, A111, None);
    });
    r.serve_here(); // state 2: the registry publishes csi-aws-ebs 1.65.0
    r.quiet("strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json");
    r.step("behind", 1, false, "strata behind 'releases/*/dev.json'");
    behind_rows(r);
    // a pin that is current against the copy of a registry whose sync stopped before its end
    r.quiet("strata snapshot 'pack:Palette Registry/csi-aws-ebs@1.65.0' -o csi-oci.json");
    r.step("behind-stale", 0, false, "strata behind csi-oci.json");
    // one pack: the pins of the Kubernetes pack, and the question to Palette about its move
    r.step(
        "behind-pin",
        1,
        false,
        "strata behind 'releases/*/dev.json' --pin k8s/kubernetes",
    );
    // a key that no record pins from a registry is a usage error, before any request
    r.step(
        "behind-pin-none",
        2,
        true,
        "strata behind 'releases/*/dev.json' --pin addon/nothing",
    );
    r.step(
        "behind-since",
        1,
        false,
        "strata behind 'releases/*/dev.json' --since behind.json --no-impact",
    );
    r.excerpt(
        "behind-since-lines",
        "behind-since",
        r"^(\$|\d+ pins|~ \[csi|      arrived)",
    );
    r.step(
        "behind-json",
        1,
        false,
        "strata behind releases/1.0.0/dev.json --json | jq -c '.pins[] | select(.behind) | {name, version, newest: .newer[0].version, bump: .bump.impact}'",
    );
    r.step(
        "behind-report",
        1,
        false,
        "scripts/ci/upstream-report.sh 'releases/*/dev.json' --dry-run",
    );
    behind_refused(r);
    behind_skip(r);
}

/// The steps of the chapter `behind` that show the line `next patch` and the option
/// `--behind-only` (#622).
fn behind_rows(r: &mut Runner) {
    // the command of the line `next patch` gives what the move to the patch does
    r.step(
        "behind-next-patch",
        1,
        false,
        "strata compare 'pack:Public Repo/kubernetes@1.35.3' 'pack:Public Repo/kubernetes@1.35.6'",
    );
    // the rows that are behind, or that strata could not check, under the heading of all the pins
    r.step(
        "behind-only",
        1,
        false,
        "strata behind 'releases/*/dev.json' --behind-only",
    );
}

/// The part of the chapter `behind` where Palette refuses a move on a cluster that runs the
/// profile.
fn behind_refused(r: &mut Runner) {
    // A move that Palette refuses on a cluster that runs the profile. The only pin that is behind
    // is csi-aws-ebs 1.63.1, two minor versions below 1.65.0. The stand-in refuses each request of
    // the route, so the profile has no other pin that is behind
    r.profile(
        DEV,
        A1111,
        "1.1.11",
        "aws-stack@1.1.1",
        &[Edit::Upgrade {
            pack: "csi-aws-ebs",
            version: "1.63.1",
        }],
    );
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A1111], &[]));
    // The route of the cluster accepts the same move. It checks no sent tag of a profile that the
    // cluster runs, so only the route of the version refuses
    r.tenant(|t| {
        t.profile_upgrade_answer(
            DEV,
            A1111,
            Some(UpgradeRefusal {
                pack: "csi-aws-ebs",
                to: "1.65.0",
                clusters: &["payments"],
            }),
        );
        t.packs_answer(DEV, PAYMENTS, None);
    });
    r.serve_here(); // state 3: payments runs aws-stack@1.1.11, and Palette refuses the move of its CSI pack
    r.quiet("strata snapshot aws-stack@1.1.11 -o refused/dev.json");
    r.step("behind-refused", 1, false, "strata behind refused/dev.json");
    r.step(
        "behind-refused-json",
        1,
        false,
        "strata behind refused/dev.json --json | jq -c '.pins[] | select(.behind) | .bump.paletteValidation'",
    );
    r.step(
        "behind-refused-no-impact",
        1,
        false,
        "strata behind refused/dev.json --no-impact",
    );
    r.quiet("strata snapshot cluster:payments -o refused/payments.json");
    // The record of the cluster that runs the profile pins the same pack, and gets the same bump
    r.step(
        "behind-cluster",
        1,
        false,
        "strata behind refused/payments.json",
    );
}

/// The part of the chapter `behind` where a move of Kubernetes skips a minor version (#623).
fn behind_skip(r: &mut Runner) {
    // aws-stack@1.1.20 pins kubernetes 1.34.9, two minor versions below 1.36.2, and the cluster
    // search runs it. The route of the version and the route of the cluster accept the packs of
    // the move. The update of the cluster refuses it with K8sUpgradeNotSupported
    r.profile(
        DEV,
        A1120,
        "1.1.20",
        "aws-stack@1.1.1",
        &[Edit::Upgrade {
            pack: "kubernetes",
            version: "1.34.9",
        }],
    );
    r.tenant(|t| {
        t.cluster(DEV, SEARCH, "search", &[A1120], &[]);
        t.profile_upgrade_answer(DEV, A1120, None);
        t.packs_answer(DEV, SEARCH, None);
    });
    r.serve_here(); // state 4: search runs kubernetes 1.34.9, and Palette accepts the packs of each move
    r.quiet("strata snapshot aws-stack@1.1.20 -o skip/dev.json");
    r.step(
        "behind-skip",
        1,
        false,
        "strata behind skip/dev.json --pin k8s/kubernetes",
    );
    r.step(
        "behind-skip-json",
        1,
        false,
        "strata behind skip/dev.json --pin k8s/kubernetes --json | jq -c '.pins[] | select(.behind) | .bump.updateChecks'",
    );
    // compare --validate of a move of the cluster to aws-stack@1.1.1, which pins kubernetes 1.36.2
    r.step(
        "validate-skip",
        1,
        false,
        "strata compare cluster:search aws-stack@1.1.1 --validate",
    );
    r.excerpt(
        "validate-skip-lines",
        "validate-skip",
        r"^(aws-stack|root |palette|note|[ ~] \[k8s)",
    );
}

/// The chapter `behind-profiles`: the newer published versions of a profile that a record holds.
fn behind_profiles(r: &mut Runner) {
    // the pipeline of the team published aws-stack 1.1.0 and the pre-release 1.2.0-rc.1 in dev,
    // after the record of 1.0.0. prod and the tenant hold later versions of the same name
    r.chapter("behind-profiles", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A120_RC, "1.2.0-rc.1", "aws-stack@1.1.1", &[]);
    r.profile(PROD, A130_PROD, "1.3.0", "aws-stack@1.1.1", &[]);
    r.tenant_profile(A200_TENANT, "2.0.0", "aws-stack@1.1.1", &[]);
    // payments runs the release and three add-on profiles, as the clusters of a field test do
    // (#622)
    payments_addons(r);
    payments_cluster(r, A100);
    // Palette accepts the move of each pin of 1.0.0 that is behind: the run with the impact asks it
    r.tenant(|t| t.profile_upgrade_answer(DEV, A100, None));
    r.quiet("strata snapshot aws-stack@1.0.0 -o releases/1.0.0/dev.json");
    r.step(
        "behind-profile",
        1,
        false,
        "strata behind releases/1.0.0/dev.json --no-impact",
    );
    r.excerpt(
        "behind-profile-lines",
        "behind-profile",
        r"^(\$|\d+ pins|. \[profile\]|      1\.1\.0 created)",
    );
    r.step(
        "behind-profile-pre-release",
        1,
        false,
        "strata behind releases/1.0.0/dev.json --no-impact --pre-release",
    );
    r.excerpt(
        "behind-profile-pre-release-lines",
        "behind-profile-pre-release",
        r"^(\$|. \[profile\])",
    );
    r.step(
        "behind-profile-json",
        1,
        false,
        "strata behind releases/1.0.0/dev.json --no-impact --json | jq -c '.profiles[] | {name, version, scope, newer: [.newer[].version], behind}'",
    );
    behind_profile_move(r);
    // the record of a cluster holds each profile that the cluster runs
    r.quiet("strata snapshot cluster:payments -o payments.json");
    r.step(
        "behind-profile-cluster",
        1,
        false,
        "strata behind payments.json --no-impact",
    );
    r.excerpt(
        "behind-profile-cluster-lines",
        "behind-profile-cluster",
        r"^(\$|\d+ pins|. \[profile\])",
    );
    // the rows that are behind, each with the profile of the cluster that holds its pack. The
    // JSON also keeps the title of the record
    r.step(
        "behind-only-cluster",
        1,
        false,
        "strata behind payments.json --no-impact --behind-only",
    );
    r.step(
        "behind-owner-json",
        1,
        false,
        "strata behind payments.json --no-impact --json | jq -c '.pins[0].pinnedBy[0]'",
    );
    behind_carriers(r);
}

/// The steps of the chapter `behind-profiles` that name the newer profile version that carries a
/// newer pack (#709).
fn behind_carriers(r: &mut Runner) {
    r.serve_here(); // state 1: each newer pack version is in aws-stack@1.1.0, or above it
    // the registry publishes csi-aws-ebs 1.66.0 and cert-manager 1.22.0. No profile version pins
    // them yet, so 1.1.0 carries 1.65.0 of csi-aws-ebs, and no version carries a newer cert-manager
    r.tenant(|t| {
        t.copy_pack_version(DEV, "csi-aws-ebs", "1.65.0", "1.66.0");
        t.copy_pack_version(DEV, "cert-manager", "1.21.1", "1.22.0");
    });
    r.step(
        "behind-carrier",
        1,
        false,
        "strata behind releases/1.0.0/dev.json --no-impact",
    );
    r.excerpt(
        "behind-carrier-lines",
        "behind-carrier",
        r"^(\$|\d+ pins|~ \[|      (aws-stack@\S+ carries|no newer version))",
    );
    r.step(
        "behind-carrier-json",
        1,
        false,
        "strata behind releases/1.0.0/dev.json --no-impact --json | jq -c '.pins[] | select(.behind) | {name, version, newest: .newer[0].version, carriers}'",
    );
}

/// The steps of the chapter `behind-profiles` with the impact: `behind` compares the version with
/// the newest one, as `compare` of the two versions does (#708).
fn behind_profile_move(r: &mut Runner) {
    r.step(
        "behind-profile-move",
        1,
        false,
        "strata behind releases/1.0.0/dev.json",
    );
    r.excerpt(
        "behind-profile-move-lines",
        "behind-profile-move",
        r"^(\$|\d+ pins|. \[profile\]|      1\.1\.0 created|      bump to 1\.1\.0:)",
    );
    r.step(
        "behind-profile-move-json",
        1,
        false,
        "strata behind releases/1.0.0/dev.json --json | jq -c '.profiles[].bump.packs[]'",
    );
    r.step(
        "behind-profile-move-compare",
        1,
        false,
        "strata compare aws-stack@1.0.0 aws-stack@1.1.0 --format markdown | grep '^\\*\\*Result'",
    );
}

/// The chapter `fleet`: the clusters of a label in two projects, and a record of each.
fn fleet(r: &mut Runner) {
    // The fleet from Palette: the clusters of a label, in two projects, and a record of each
    r.chapter("fleet", &[]);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile(PROD, A111_PROD, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.tenant(|t| t.cluster(DEV, SEARCH, "search", &[A111], &[]));
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A111_PROD], &[]));
    r.tenant(|t| t.cluster(PROD, LEDGER, "ledger", &[A111_PROD], &[]));
    for (project, uid, env) in [
        (DEV, PAYMENTS, "dev"),
        (DEV, SEARCH, "dev"),
        (PROD, PAYMENTS_PROD, "prod"),
        (PROD, LEDGER, "prod"),
    ] {
        r.tenant(|t| {
            t.cluster_field(
                project,
                uid,
                "/metadata/labels",
                json!({"env": env, "managed-by": "opentofu"}),
            )
        });
    }
    r.tenant(|t| t.tag_filter("production", PRODUCTION, "env", "prod"));
    r.serve_here(); // state 1: two clusters of each project, and the production ones labeled so
    r.step("fleet-tag", 0, false, "strata list clusters --tag env=prod");
    r.step(
        "fleet-filters",
        0,
        false,
        "strata list filters --project ''",
    );
    r.step(
        "fleet-snapshot",
        0,
        true,
        "strata snapshot filter:production --dir fleet/prod --prune",
    );
    r.step(
        "fleet-set-json",
        0,
        false,
        "strata snapshot filter:production --dir fleet/prod --json --quiet | jq -c '.schema, .summary, (.records[] | {name: .cluster.name, path, root, error})'",
    );
    r.step(
        "fleet-json",
        0,
        false,
        "strata list clusters --filter production --json | jq -c '.resolved, (.clusters[] | {name, projectName, labels})'",
    );
    // ledger leaves production: the automation that labels it drops the label
    r.tenant(|t| {
        t.cluster_field(
            PROD,
            LEDGER,
            "/metadata/labels",
            json!({"env": "retired", "managed-by": "opentofu"}),
        )
    });
    r.serve_here(); // state 2: ledger is no longer labeled env=prod
    r.step(
        "fleet-snapshot-again",
        0,
        true,
        "strata snapshot filter:production --dir fleet/prod --prune",
    );
    r.step(
        "fleet-verify",
        0,
        false,
        "strata verify 'fleet/prod/*.json' --summary",
    );
}

/// The chapter `same-names`: two projects that hold a cluster and a profile with one name.
fn same_names(r: &mut Runner) {
    r.chapter("same-names", &[]);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile_own_copy(
        PROD,
        A111_PROD,
        "1.1.1",
        "aws-stack@1.1.1",
        RETAIN,
        "the same-names chapter gives prod a changed value",
    );
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A111_PROD], &[]));
    r.step(
        "names-tenant",
        0,
        false,
        "strata list clusters --project ''",
    );
    r.step(
        "names-ambiguous",
        2,
        true,
        "strata snapshot cluster:payments -o payments.json",
    );
    r.step(
        "names-cluster",
        0,
        false,
        "strata list --cluster payments --project ''",
    );
    r.step(
        "names-profile",
        2,
        true,
        "strata snapshot aws-stack@1.1.1 -o aws-stack.json",
    );
    r.step(
        "names-by-project",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot cluster:payments -o dev-payments.json",
    );
    r.step(
        "names-by-project",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot cluster:payments -o prod-payments.json",
    );
    r.step(
        "names-verify",
        0,
        false,
        "strata verify dev-payments.json prod-payments.json",
    );
    r.step(
        "names-compare",
        1,
        false,
        "strata compare dev-payments.json prod-payments.json",
    );
}

/// The chapter `projects`: one tenant with a project for each environment. One run reads the
/// two projects, for each question of the table of the introduction (#486).
fn projects(r: &mut Runner) {
    r.chapter("projects", &[]);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.tenant(|t| t.registries(DEV));
    r.tenant(|t| t.registries(PROD));
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    // 1.1.0 reached prod by a promotion, and somebody built 1.1.1 by hand in prod
    r.profile(PROD, A110_PROD, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile_own_copy(
        PROD,
        A111_PROD,
        "1.1.1",
        "aws-stack@1.1.1",
        RETAIN,
        "the projects chapter gives prod a changed value",
    );
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A110_PROD], &[]));
    let sides = "--project-name-a dev --project-name-b prod";
    r.step(
        "projects-same",
        0,
        false,
        &format!("strata same-release aws-stack@1.1.0 aws-stack@1.1.0 {sides}"),
    );
    r.step(
        "projects-different",
        1,
        false,
        &format!("strata same-release aws-stack@1.1.1 aws-stack@1.1.1 {sides}"),
    );
    r.step(
        "projects-compare",
        1,
        false,
        &format!("strata compare aws-stack@1.1.1 aws-stack@1.1.1 {sides}"),
    );
    projects_records(r);
}

/// The part of the chapter `projects` that records each project, and asks about the records.
fn projects_records(r: &mut Runner) {
    for project in ["dev", "prod"] {
        r.step(
            "projects-record",
            0,
            true,
            &format!(
                "strata snapshot aws-stack@1.1.0 --project-name {project} -o releases/1.1.0/{project}.json"
            ),
        );
    }
    r.step(
        "projects-verify",
        0,
        false,
        "strata verify 'releases/*/*.json'",
    );
    r.step(
        "projects-cluster",
        0,
        false,
        "strata compare releases/1.1.0/prod.json cluster:payments --project-name prod",
    );
    r.step(
        "projects-behind",
        1,
        false,
        "strata behind 'releases/*/*.json' --no-impact",
    );
    r.step(
        "projects-consumers",
        1,
        true,
        "strata consumers aws-stack@1.1.0 --project-name prod",
    );
    r.step(
        "projects-list",
        0,
        false,
        "strata list clusters --project-name prod",
    );
}

/// The chapter `packs`: the packs of the registries, compared and recorded.
fn packs(r: &mut Runner) {
    r.chapter("packs", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.tenant(|t| t.registries(DEV));
    // two versions pin csi-aws-ebs 1.65.0, each from one of the two registries that have it
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile(DEV, A1118, "1.1.18", "aws-stack@1.1.1", OCI_CSI);
    r.step(
        "pack-compare",
        1,
        false,
        "strata compare 'pack:Public Repo/kubernetes@1.35.3' 'pack:Public Repo/kubernetes@1.36.2'",
    );
    r.excerpt(
        "pack-compare-images",
        "pack-compare",
        r"^(pack |root |images |~ \[|      [~+-] (image|chart) )",
    );
    r.step(
        "pack-os",
        1,
        false,
        "strata compare 'pack:Public Repo/ubuntu-aws@20.04' 'pack:Public Repo/ubuntu-aws@24.04'",
    );
    two_registries(r);
    r.step("pack-registries", 1, false, "strata compare 'pack:Public Repo/csi-aws-ebs@1.65.0' 'pack:Palette Registry/csi-aws-ebs@1.65.0'");
    r.step(
        "pack-record",
        0,
        true,
        "strata snapshot 'pack:Public Repo/kubernetes@latest' -o kubernetes.json",
    );
    r.step("pack-verify", 0, false, "strata verify kubernetes.json");
    r.step(
        "pack-verify-json",
        0,
        false,
        "strata verify kubernetes.json --json | jq '.verify'",
    );
    r.step(
        "packs-list",
        0,
        false,
        "strata packs csi-aws-ebs --registry 'Public Repo'",
    );
    r.step(
        "packs-latest",
        0,
        false,
        "strata packs csi-aws-ebs --latest",
    );
    r.step(
        "pack-behind",
        1,
        false,
        "strata compare 'pack:Public Repo/kubernetes@1.35.3' 'pack:Public Repo/kubernetes@latest'",
    );
    // the claim is that @latest resolves, and that the report names the version it resolved to
    r.excerpt("pack-behind-head", "pack-behind", r"^(pack |root )");
    r.step(
        "packs-cloud",
        0,
        false,
        "strata packs kubernetes --registry 'Public Repo' --cloud aws --latest",
    );
    r.step("packs-json", 0, false, r#"strata packs csi-aws-ebs --registry 'Public Repo' --latest --json | jq '.registries[0].versions[0]'"#);
}

/// The steps of the chapter `packs` for a pack version in two registries: the refusal of a ref
/// that names no registry, and the list and the record that name the registry of each pin.
fn two_registries(r: &mut Runner) {
    r.step(
        "pack-ambiguous",
        2,
        true,
        "strata compare pack:csi-aws-ebs@1.65.0 pack:csi-aws-ebs@1.65.0",
    );
    r.step(
        "pack-which-registry",
        0,
        false,
        "strata list --pack csi-aws-ebs",
    );
    r.quiet("strata snapshot aws-stack@1.1.18 -o aws-stack-1.1.18.json");
    r.step(
        "pack-record-registry",
        0,
        false,
        r#"jq '.packs[] | select(.name == "csi-aws-ebs") | {key, version, registryUid, registryName}' aws-stack-1.1.18.json"#,
    );
}

/// The chapter `palette-latest`: a rebuild of the newest version of a pack.
fn palette_latest(r: &mut Runner) {
    // A rebuild of the newest version: strata and Palette both order it below its base
    r.chapter("palette-latest", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.tenant(|t| t.copy_pack_version(DEV, "cert-manager", "1.21.1", "1.21.1-rev1"));
    r.step(
        "packs-palette-latest",
        0,
        false,
        "strata packs cert-manager | head -4",
    );
    r.step(
        "packs-palette-latest-json",
        0,
        false,
        "strata packs cert-manager --json | jq -c '.registries[0].versions[:2][] | {version, latest, paletteLatest}'",
    );
    r.serve_here(); // state 1: cert-manager has a rebuild below its newest version
    // A pre-release above the newest version: Palette marks it, and strata does not
    r.tenant(|t| t.copy_pack_version(DEV, "cert-manager", "1.21.1", "1.22.0-rc.1"));
    r.step(
        "packs-pre-release",
        0,
        false,
        "strata packs cert-manager --latest",
    );
    r.step(
        "packs-pre-release-json",
        0,
        false,
        "strata packs cert-manager --latest --json | jq -c '.registries[0].versions[] | {version, latest, paletteLatest}'",
    );
    // the note of the record names the pre-release that `@latest` skips (#625)
    r.step(
        "packs-pre-release-record",
        0,
        true,
        "strata snapshot 'pack:Public Repo/cert-manager@latest' -o cert-manager.json",
    );
}

/// The chapter `behind-pre-release`: a pipeline publishes a pre-release of the next version.
fn behind_pre_release(r: &mut Runner) {
    // The pre-release is below its version, so it is no newer version of a pin that is final
    r.chapter("behind-pre-release", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.copy_pack_version(DEV, "csi-aws-ebs", "1.65.0", "1.66.0-rc.1"));
    r.quiet("strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json");
    r.step(
        "behind-pre-release",
        0,
        false,
        "strata behind releases/1.1.1/dev.json --no-impact | head -1",
    );
    r.step(
        "behind-pre-release-shown",
        1,
        false,
        "strata behind releases/1.1.1/dev.json --no-impact --pre-release",
    );
    r.excerpt(
        "behind-pre-release-lines",
        "behind-pre-release-shown",
        r"^(\$|\d+ pins|~ \[csi)",
    );
}

/// The chapter `retire`: the consumers of a version that a team retires.
fn retire(r: &mut Runner) {
    r.chapter("retire", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A110], &[]));
    r.tenant(|t| t.cluster(DEV, SEARCH, "search", &[A110], &[]));
    r.step(
        "retire-in-use",
        1,
        false,
        "strata consumers aws-stack@1.1.0",
    );
    r.step(
        "consumers-list-only",
        1,
        false,
        "strata consumers aws-stack@1.1.0 --list-only",
    );
    r.step("retire-free", 0, false, "strata consumers aws-stack@1.0.0");
    r.step(
        "retire-gate",
        0,
        false,
        r#"strata consumers aws-stack@1.0.0 && echo "no cluster uses aws-stack@1.0.0""#,
    );
    r.step("retire-json", 1, false, r#"strata consumers aws-stack@1.1.0 --json | jq -r '.consumers[] | "\(.name) \(.state) \(.version)"'"#);
    // no cluster runs 1.1.1, and a cluster template links it: Palette refuses the delete
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster_template(DEV, AWS_STANDARD, "aws-standard", &[A111]));
    r.step(
        "retire-template",
        1,
        false,
        "strata consumers aws-stack@1.1.1",
    );
}
