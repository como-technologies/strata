//! The chapters of the fleet, the registries, and the retire of a version.

use super::DEV_ENV;
use super::edits::RETAIN;
use super::ids::{
    A100, A110, A111, A111_PROD, A1111, AWS_STANDARD, DEV, LEDGER, PAYMENTS, PAYMENTS_PROD, PROD,
    PRODUCTION, SEARCH,
};
use crate::book::Runner;
use crate::tenant::Edit;
use serde_json::json;

/// Run the chapters of the fleet, the registries, and the retire of a version.
pub(super) fn run(r: &mut Runner) {
    behind(r);
    fleet(r);
    same_names(r);
    packs(r);
    palette_latest(r);
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
    // a pin that is current against the copy of a registry whose sync stopped before its end
    r.quiet("strata snapshot 'pack:Palette Registry/csi-aws-ebs@1.65.0' -o csi-oci.json");
    r.step("behind-stale", 0, false, "strata behind csi-oci.json");
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
            uid: "6a75f441d410c3b5762fed2d",
            version: "1.63.1",
        }],
    );
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A1111], &[]));
    // The route of the cluster accepts the same move. It checks no sent tag of a profile that the
    // cluster runs, so only the route of the version refuses
    r.tenant(|t| {
        t.profile_upgrade_answer(DEV, A1111, Some(("csi-aws-ebs", &["payments"])));
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

/// The chapter `packs`: the packs of the registries, compared and recorded.
fn packs(r: &mut Runner) {
    r.chapter("packs", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.tenant(|t| t.registries(DEV));
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
    r.step(
        "pack-ambiguous",
        2,
        true,
        "strata compare pack:csi-aws-ebs@1.65.0 pack:csi-aws-ebs@1.65.0",
    );
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

/// The chapter `palette-latest`: a rebuild of the newest version of a pack.
fn palette_latest(r: &mut Runner) {
    // A rebuild of the newest version: strata orders it above its base, and Palette below
    r.chapter("palette-latest", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.tenant(|t| t.rebuild_pack_version(DEV, "cert-manager", "1.21.1"));
    r.step(
        "packs-palette-latest",
        0,
        false,
        "strata packs cert-manager --latest",
    );
    r.step(
        "packs-palette-latest-json",
        0,
        false,
        "strata packs cert-manager --latest --json | jq -c '.registries[0].versions[] | {version, latest, paletteLatest}'",
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
