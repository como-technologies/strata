//! The chapter `loop`: the everyday loop of a team.

use super::edits::RETAIN;
use super::ids::{
    A100, A100_PROD, A110, A110_PROD, DEV, PAYMENTS, PAYMENTS_PROD, PROD, PRODUCTION,
};
use super::preflight::{accepted_answer, aws_validate, preflight_entity, preflight_routes};
use super::{CANDIDATE_YAML, GIT, payments_addons, payments_cluster};
use crate::book::Runner;
use serde_json::json;

/// The labels that put a cluster of the loop in its environment, and in the fleet of its filter.
fn label(r: &mut Runner, project: &str, uid: &str, env: &str) {
    r.tenant(|t| {
        t.cluster_field(
            project,
            uid,
            "/metadata/labels",
            json!({"env": env, "managed-by": "opentofu"}),
        )
    });
}

/// The everyday loop: one change of one team, from the pull request to the retire of the
/// version that it replaces. Each step shows the few lines that answer its question; the use
/// case of the step shows the rest.
pub(super) fn run(r: &mut Runner) {
    r.chapter("loop", GIT);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.tenant(|t| t.registries(DEV));
    r.tenant(|t| t.registries(PROD));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(PROD, A100_PROD, "1.0.0", "aws-stack@1.0.0", &[]);
    // payments of dev runs the release and three add-on profiles (#615)
    payments_addons(r);
    payments_cluster(r, A100);
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A100_PROD], &[]));
    label(r, DEV, PAYMENTS, "dev");
    label(r, PROD, PAYMENTS_PROD, "prod");
    r.tenant(|t| t.tag_filter("production", PRODUCTION, "env", "prod"));
    // Palette takes the name aws-stack@1.1.0 and the packs of the candidate, which it does not hold
    r.tenant(|t| t.profile_create_checks(DEV, ("aws-stack", "1.1.0"), None, None));
    r.tenant(|t| t.profile_packs_answer(DEV, None, None));
    // Palette accepts the move of each pin that is behind. Only aws-stack@1.0.0 in dev pins one
    r.tenant(|t| t.profile_upgrade_answer(DEV, A100, None));
    // the repository of the team: main holds the record of what each project runs
    r.quiet("git init -q -b main . && git remote add origin .");
    r.quiet("strata snapshot aws-stack@1.0.0 -o releases/1.0.0/dev.json &&\n  PALETTE_PROJECT=prod strata snapshot aws-stack@1.0.0 -o releases/1.0.0/prod.json &&\n  git add releases && git commit -qm \"dev and prod run aws-stack@1.0.0\"");
    r.serve_here(); // state 1: dev and prod run 1.0.0, and main records it
    loop_merge(r);
    loop_publish(r);
    loop_promote(r);
    loop_preflight(r);
    loop_nightly(r);
    loop_incident(r);
}

/// The steps 1 and 2 of the loop: the pull request of a developer, and the merge gate.
fn loop_merge(r: &mut Runner) {
    // 1. a developer changes the profile in a pull request
    r.quiet("git checkout -q -b aws-stack-1.1.0");
    r.write_file("aws-stack.yaml", CANDIDATE_YAML);
    r.step(
        "loop-candidate",
        0,
        true,
        "strata snapshot --from-candidate aws-stack.yaml --validate -o candidate.json",
    );
    r.quiet("git add aws-stack.yaml && git commit -qm \"aws-stack 1.1.0: Kubernetes 1.36, Ubuntu 24.04\"");
    r.step(
        "loop-review",
        0,
        true,
        "scripts/ci/pr-comment.sh releases/1.0.0/dev.json candidate.json --dry-run > comment.md",
    );
    r.step("loop-review", 0, false, "grep '^\\*\\*Result' comment.md");
    // 2. the merge gate: the records after the merge button
    r.step(
        "loop-merge",
        0,
        true,
        r#"scripts/ci/merge-gate.sh "releases/*/*.json" --base main"#,
    );
    r.quiet("git checkout -q main && git merge -q --no-ff --no-edit aws-stack-1.1.0");
}

/// The step 3 of the loop: the pipeline publishes the version, and dev moves a cluster to it.
fn loop_publish(r: &mut Runner) {
    // 3. the pipeline of the team publishes the version, and records it
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    // Palette answers for the move of payments to the version: it repaves, and it takes the packs
    r.tenant(|t| t.repave_answer(DEV, PAYMENTS, true));
    r.tenant(|t| t.packs_answer(DEV, PAYMENTS, None));
    r.serve_here(); // state 2: the pipeline published 1.1.0 in dev
    r.step(
        "loop-publish",
        0,
        true,
        "strata snapshot aws-stack@1.1.0 -o releases/1.1.0/dev.json",
    );
    r.step(
        "loop-publish",
        0,
        false,
        "strata same-release candidate.json releases/1.1.0/dev.json",
    );
    r.quiet("git add releases && git commit -qm \"record aws-stack@1.1.0 in dev\"");
    // the release that the clusters run has a newer published version now (#707), and the line
    // under it says what the move to it does (#708)
    r.step(
        "loop-behind-profile",
        1,
        false,
        "strata behind releases/1.0.0/dev.json | grep -A2 '\\[profile\\]'",
    );
    // before dev moves payments, the pipeline asks Palette about the move
    r.step(
        "loop-ask-repave",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.0 --repave | grep -E '^(aws-stack|root |palette)'",
    );
    r.step(
        "loop-ask-validate",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.0 --validate | grep -E '^(aws-stack|root |palette)'",
    );
    // the pull request that moves the cluster: its comment holds the answer, and the label follows
    // it
    r.step(
        "loop-ask-repave-comment",
        0,
        true,
        "STRATA_QUIET=1 scripts/ci/pr-comment.sh cluster:payments aws-stack@1.1.0 --repave --dry-run > comment.md",
    );
    r.step(
        "loop-ask-repave-comment",
        0,
        false,
        "grep '^\\*\\*Palette' comment.md",
    );
    // dev moves payments to the new version, and a record says what the cluster runs
    payments_cluster(r, A110);
    label(r, DEV, PAYMENTS, "dev");
    r.step(
        "loop-cluster",
        0,
        false,
        "strata compare releases/1.1.0/dev.json cluster:payments | head -2",
    );
}

/// The step 4 of the loop: the promotion to prod, on demand.
fn loop_promote(r: &mut Runner) {
    // 4. the promotion to prod, on demand
    r.profile(PROD, A110_PROD, "1.1.0", "aws-stack@1.1.0", &[]);
    r.quiet("PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.0 -o releases/1.1.0/prod.json");
    r.step(
        "loop-promote",
        0,
        false,
        "strata same-release releases/1.1.0/dev.json releases/1.1.0/prod.json",
    );
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A110_PROD], &[]));
    label(r, PROD, PAYMENTS_PROD, "prod");
    r.quiet("git add releases && git commit -qm \"promote aws-stack@1.1.0 to prod\"");
}

/// The step 5 of the loop: the check of the body of a create before the build.
fn loop_preflight(r: &mut Runner) {
    // 5. Before a build, the team writes the body of the create of a cluster that prod builds on
    // the release. The team asks Palette about it, with nothing created
    r.write_file("cluster.json", &preflight_entity("ledger", A110_PROD));
    preflight_routes(r, PROD);
    r.serve_here(); // state 3: both projects run 1.1.0, main records it, and Palette refuses a worker pool
    r.step(
        "loop-body",
        0,
        false,
        "jq -c '{name: .metadata.name, profile: .spec.profiles[0].uid, pools: [.spec.machinepoolconfig[].poolConfig | {name, size}]}' cluster.json",
    );
    r.step(
        "loop-preflight-static",
        0,
        false,
        "PALETTE_PROJECT=prod strata preflight cluster.json --cloud aws --tier static --offline --quiet | grep -E '^(OK|FAIL)'",
    );
    // the pull request gets the same report as a comment of its own
    r.step(
        "loop-preflight-comment",
        0,
        true,
        "PALETTE_PROJECT=prod strata preflight cluster.json --cloud aws --tier static --offline --quiet --out preflight > /dev/null && scripts/ci/pr-comment.sh --preflight preflight/report.md --dry-run | sed -n '1,4p'",
    );
    r.step(
        "loop-preflight-live",
        1,
        false,
        r#"PALETTE_PROJECT=prod strata preflight cluster.json --cloud aws --no-exec --strict --json --quiet | jq -r '.checks[] | select(.verdict == "failed" and .severity == "fail") | "\(.id): \((.messages // []) | join("; "))"'"#,
    );
    // the fix is two workers; Palette then finds no fault
    r.step(
        "loop-preflight-fix",
        0,
        false,
        r#"jq '(.spec.machinepoolconfig[] | select(.poolConfig.name == "worker-pool") | .poolConfig.size) = 2' cluster.json > fixed.json && mv fixed.json cluster.json && jq -c '.spec.machinepoolconfig[].poolConfig | {name, size}' cluster.json"#,
    );
    r.answer(PROD, &aws_validate(), &accepted_answer());
    r.serve_here(); // state 4: Palette finds no fault in the body with two workers
    r.step(
        "loop-preflight-clean",
        0,
        false,
        "PALETTE_PROJECT=prod strata preflight cluster.json --cloud aws --no-exec --strict --quiet | grep -E '^(OK|FAIL)'",
    );
}

/// The step 6 of the loop: the nightly jobs, which find an edit in place.
fn loop_nightly(r: &mut Runner) {
    // 6. the nightly jobs. The fleet first: the records of the clusters that Palette names prod.
    r.step(
        "loop-fleet",
        0,
        true,
        "strata snapshot filter:production --dir fleet/prod --prune",
    );
    r.quiet("git add fleet && git commit -qm \"the fleet of prod\"");
    // a person edits the published version in prod through the API, with no review
    r.tenant(|t| t.edit(PROD, A110_PROD, RETAIN));
    r.serve_here(); // state 5: somebody edited aws-stack@1.1.0 in prod in place
    r.step(
        "loop-nightly",
        1,
        true,
        "scripts/ci/verify-report.sh 'releases/**/*.json' 'fleet/**/*.json' --dry-run > issues.md",
    );
    r.step(
        "loop-who-full",
        1,
        false,
        "strata verify releases/1.1.0/prod.json --who",
    );
    r.excerpt(
        "loop-who",
        "loop-who-full",
        r"^(aws-stack|DRIFT|who:|  \d{4}-)",
    );
    // the scheduled job takes the same option: the issue of the edit names its writer
    r.step(
        "loop-nightly-who",
        1,
        true,
        "scripts/ci/verify-report.sh 'releases/**/*.json' 'fleet/**/*.json' --who --dry-run > issues-who.md",
    );
    r.step(
        "loop-nightly-who",
        0,
        false,
        "grep -E '^(who:|- 20)' issues-who.md",
    );
    r.step(
        "loop-upstream",
        1,
        true,
        "scripts/ci/upstream-report.sh 'releases/**/dev.json' --dry-run > upstream.md",
    );
    r.step(
        "loop-upstream",
        0,
        false,
        "grep '^| `.*palette ' upstream.md | cut -d'|' -f2,4",
    );
}

/// The steps 7 and 8 of the loop: an incident, and the retire of a version that nothing runs.
fn loop_incident(r: &mut Runner) {
    // 7. an incident: a pack has a security problem, and a person asks who carries it
    for project in ["dev", "prod"] {
        r.step(
            "loop-incident-full",
            0,
            false,
            &format!("PALETTE_PROJECT={project} strata list --pack csi-aws-ebs"),
        );
    }
    r.excerpt(
        "loop-incident",
        "loop-incident-full",
        r"^(\$|project|\d+ profiles?|  aws-stack|    csi)",
    );
    for project in ["dev", "prod"] {
        r.step(
            "loop-consumers",
            1,
            false,
            &format!("PALETTE_PROJECT={project} strata consumers aws-stack@1.1.0 --list-only"),
        );
    }

    // 8. the retire of the version that nothing runs now
    r.step("loop-retire", 0, false, "strata consumers aws-stack@1.0.0");
}
