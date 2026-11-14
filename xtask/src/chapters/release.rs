//! The chapters of the use cases of a release: the review, the merge gate, the promotion, and the
//! packs that differ between two projects.

use super::edits::{MACRO_REGION, OCI_CSI, RETAIN, TAG_2404, TAG_LTS2404};
use super::ids::{
    A100, A110, A110_PROD, A111, A111_PROD, A112_PROD, A113, A114, A114_PROD, ADDONS_DEV,
    ADDONS_PACK, ADDONS_PROD, ADDONS_PROD2, DEV, PAYMENTS, PROD,
};
use super::{CANDIDATE_YAML, DEV_ENV, GIT};
use crate::book::Runner;
use crate::tenant::Edit;

/// Run the chapters of the use cases of a release.
pub(super) fn run(r: &mut Runner) {
    review(r);
    merge_gate(r);
    merge_gate_rollback(r);
    promote(r);
    registry_move(r);
    tag_name(r);
    manifest_pack(r);
}

/// The chapter `review`: the review of a candidate against the release that dev runs.
fn review(r: &mut Runner) {
    r.chapter("review", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    // the release under review runs somewhere: the repave has a target
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A100], &[]));
    r.step(
        "review-record",
        0,
        true,
        "strata snapshot aws-stack@1.0.0 -o releases/1.0.0/dev.json",
    );
    r.write_file("aws-stack.yaml", CANDIDATE_YAML);
    r.step("review-candidate", 0, false, "cat aws-stack.yaml");
    r.step(
        "review-snapshot",
        0,
        true,
        "strata snapshot --from-candidate aws-stack.yaml -o candidate.json",
    );
    r.step(
        "review-markdown",
        1,
        false,
        "strata compare releases/1.0.0/dev.json candidate.json --format markdown",
    );
    r.step("review-repave", 1, false, r#"strata compare releases/1.0.0/dev.json candidate.json --json | jq -r '.packs[] | select(.impact.level == "repave") | "\(.name): \(.impact.text)"'"#);
    r.step(
        "review-pr-comment",
        0,
        true,
        "scripts/ci/pr-comment.sh releases/1.0.0/dev.json candidate.json --dry-run > comment.md",
    );
    r.step("review-comment-head", 0, false, "head -3 comment.md");
    r.serve_here(); // the page describes what follows: the publication of the reviewed version
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.step(
        "review-published",
        0,
        true,
        "strata snapshot aws-stack@1.1.0 -o releases/1.1.0/dev.json",
    );
    r.step(
        "review-same",
        0,
        false,
        "strata same-release candidate.json releases/1.1.0/dev.json",
    );
}

/// The chapter `merge-gate`: a merge request that pins a version, and the checks of the gate.
fn merge_gate(r: &mut Runner) {
    r.chapter("merge-gate", GIT);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    // main pins aws-stack@1.0.0, and a branch changes the record: the profile was edited in place
    r.quiet("git init -q -b main . && git remote add origin .");
    r.quiet("strata snapshot aws-stack@1.0.0 -o releases/dev.json &&\n  git add releases && git commit -qm \"dev pins aws-stack@1.0.0\"");
    r.quiet("git checkout -q -b keep-the-volumes");
    r.tenant(|t| t.edit(DEV, A100, RETAIN));
    r.quiet("strata snapshot aws-stack@1.0.0 -o releases/dev.json &&\n  git commit -qam \"keep the volumes on delete\"");
    // while that branch waited, another merge request published 1.1.0 and pinned it on main
    r.quiet("git checkout -q main");
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.quiet("strata snapshot aws-stack@1.1.0 -o releases/dev.json &&\n  git commit -qam \"dev pins aws-stack@1.1.0\"");
    // the branch settled the conflict by keeping its own record, which is what a hand merge does
    r.quiet("git checkout -q keep-the-volumes && git fetch -q origin main &&\n  { git merge -q --no-edit --no-commit origin/main || true; } &&\n  git checkout HEAD -- releases/dev.json &&\n  git commit -qm \"merge main, keep our record\"");
    r.step(
        "merge-gate-behind",
        1,
        true,
        r#"scripts/ci/merge-gate.sh "releases/*.json" --base main"#,
    );
    // the fix: pin the version that main has, with the change on top
    r.quiet("strata snapshot aws-stack@1.1.0 -o releases/dev.json && git commit -qam \"pin aws-stack@1.1.0\"");
    r.step(
        "merge-gate-clear",
        0,
        true,
        r#"scripts/ci/merge-gate.sh "releases/*.json" --base main"#,
    );
    // and the check that git cannot make: the pinned profile is edited in Palette after the record
    r.tenant(|t| t.edit(DEV, A110, RETAIN));
    r.step(
        "merge-gate-drifted",
        1,
        true,
        r#"scripts/ci/merge-gate.sh "releases/*.json" --base main"#,
    );
    // a strata that is older than the gate: the schema of its comparison has no versionMove
    r.write_file("older-strata", OLDER_STRATA);
    r.quiet("chmod +x older-strata");
    r.step(
        "merge-gate-older-strata",
        2,
        true,
        r#"STRATA_BIN=./older-strata scripts/ci/merge-gate.sh "releases/*.json" --base main"#,
    );
}

/// A script for `STRATA_BIN` that answers as strata 0.26.0 does: its schema of the comparison
/// names no `versionMove`.
const OLDER_STRATA: &str = "#!/bin/sh\necho '{\"title\": \"Diff\", \"type\": \"object\"}'\n";

/// The chapter `merge-gate-rollback`: a branch that restores the version that dev ran before.
fn merge_gate_rollback(r: &mut Runner) {
    r.chapter("merge-gate-rollback", GIT);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    // dev ran 1.0.0, and then 1.1.0
    r.quiet("git init -q -b main . && git remote add origin .");
    r.quiet("strata snapshot aws-stack@1.0.0 -o releases/dev.json &&\n  git add releases && git commit -qm \"dev pins aws-stack@1.0.0\"");
    r.quiet("strata snapshot aws-stack@1.1.0 -o releases/dev.json &&\n  git commit -qam \"dev pins aws-stack@1.1.0\"");
    // 1.1.0 repaved the nodes badly, so a branch off main restores what dev ran before
    r.quiet("git checkout -q -b roll-dev-back &&\n  git show main~1:releases/dev.json > releases/dev.json &&\n  git commit -qam \"pin what dev ran before 1.1.0\"");
    r.step(
        "merge-gate-rollback-red",
        1,
        true,
        r#"scripts/ci/merge-gate.sh "releases/*.json" --base main"#,
    );
    // the same change, with the rollback declared in the commit that makes it
    r.quiet("git commit -q --amend -m \"roll dev back to aws-stack@1.0.0\n\nRollback: releases/dev.json to aws-stack@1.0.0\n1.1.0 repaved the nodes with a CSI driver that lost the volumes\"");
    r.step(
        "merge-gate-rollback",
        0,
        true,
        r#"scripts/ci/merge-gate.sh "releases/*.json" --base main"#,
    );
}

/// The chapter `promote`: the same version in dev and in prod, with a changed value in prod.
fn promote(r: &mut Runner) {
    r.chapter("promote", &[]);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    // the release under review runs somewhere, so the check discriminates
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    // 1.1.0 reached prod by a promotion from the dev source, so its content is dev's content.
    r.profile(PROD, A110_PROD, "1.1.0", "aws-stack@1.1.0", &[]);
    // 1.1.1 was built by hand in prod, and one value differs.
    r.profile_own_copy(
        PROD,
        A111_PROD,
        "1.1.1",
        "aws-stack@1.1.1",
        RETAIN,
        "the promote chapter gives prod a changed value",
    );
    r.step(
        "promote-record",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot aws-stack@1.1.0 -o releases/1.1.0/dev.json",
    );
    r.step(
        "promote-record",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.0 -o releases/1.1.0/prod.json",
    );
    r.step(
        "promote-record",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json",
    );
    r.step(
        "promote-record",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.1 -o releases/1.1.1/prod.json",
    );
    r.step(
        "promote-same",
        0,
        false,
        "strata same-release releases/1.1.0/dev.json releases/1.1.0/prod.json",
    );
    r.step(
        "promote-different",
        1,
        false,
        "strata same-release releases/1.1.1/dev.json releases/1.1.1/prod.json",
    );
    r.step(
        "promote-compare",
        1,
        false,
        "strata compare releases/1.1.1/dev.json releases/1.1.1/prod.json",
    );
    r.step(
        "promote-consumers",
        0,
        true,
        "PALETTE_PROJECT=prod strata consumers aws-stack@1.1.1",
    );
    promote_macros(r);
}

/// The part of the chapter `promote` for a release with a macro that each project gives another
/// value.
fn promote_macros(r: &mut Runner) {
    // 1.1.4 reached prod by a promotion, and its CSI values read the macro region, which each
    // project gives another value: one release, two configurations (#164)
    r.profile(DEV, A114, "1.1.4", "aws-stack@1.1.1", MACRO_REGION);
    r.profile(PROD, A114_PROD, "1.1.4", "aws-stack@1.1.1", MACRO_REGION);
    r.tenant(|t| t.set_macro(DEV, "region", "us-east-2"));
    r.tenant(|t| t.set_macro(PROD, "region", "eu-west-1"));
    r.step(
        "promote-macro-record",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot aws-stack@1.1.4 -o releases/1.1.4/dev.json",
    );
    r.step(
        "promote-macro-record",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.4 -o releases/1.1.4/prod.json",
    );
    r.step(
        "promote-macro-not-compared",
        0,
        false,
        "strata same-release releases/1.1.4/dev.json releases/1.1.4/prod.json",
    );
    r.step(
        "promote-macro-record-digests",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot aws-stack@1.1.4 --macros -o releases/1.1.4/dev-macros.json",
    );
    r.step(
        "promote-macro-record-digests",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.4 --macros -o releases/1.1.4/prod-macros.json",
    );
    r.step(
        "promote-macro-changed",
        0,
        false,
        "strata same-release releases/1.1.4/dev-macros.json releases/1.1.4/prod-macros.json",
    );
    r.step(
        "promote-macro-changed-json",
        0,
        false,
        "strata same-release releases/1.1.4/dev-macros.json releases/1.1.4/prod-macros.json --json | jq -c '.pairs[] | {status, macrosChanged}'",
    );
}

/// The chapter `registry-move`: a pack that prod takes from another registry.
fn registry_move(r: &mut Runner) {
    r.chapter("registry-move", &[]);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile_own_copy(
        PROD,
        A111_PROD,
        "1.1.1",
        "aws-stack@1.1.1",
        OCI_CSI,
        "the registry-move chapter gives prod the OCI pack",
    );
    // prod also holds 1.1.2, which moved the pack and changed a value on top of the move. Both
    // exist before the playground hands the tenant over, so every step of the page runs against it.
    r.profile_own_copy(
        PROD,
        A112_PROD,
        "1.1.2",
        "aws-stack@1.1.1",
        &[Edit::OciCsi, Edit::Reclaim("Retain")],
        "the move, and a changed value",
    );
    r.step(
        "move-record",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json",
    );
    r.step(
        "move-record",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.1 -o releases/1.1.1/prod.json",
    );
    r.step(
        "move-exact",
        1,
        false,
        "strata same-release releases/1.1.1/dev.json releases/1.1.1/prod.json",
    );
    r.step(
        "move-compare",
        1,
        false,
        "strata compare releases/1.1.1/dev.json releases/1.1.1/prod.json",
    );
    // the page teaches the flags on the CSI line and its impact, and not the other packs
    r.excerpt(
        "move-compare-csi",
        "move-compare",
        r"^(aws-stack|root |note |~ \[csi)|  +(impact:|upstream digest)",
    );
    r.step("move-fields", 1, false, r#"strata compare releases/1.1.1/dev.json releases/1.1.1/prod.json --json | jq '.packs[] | select(.identity) | {name, a: .a | {packUid, registryUid, upstream}, b: .b | {packUid, registryUid, upstream}}'"#);
    r.step("move-allow", 0, false, "strata same-release --allow registry-move releases/1.1.1/dev.json releases/1.1.1/prod.json");
    r.step(
        "move-rerecord",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.2 -o releases/1.1.2/prod.json",
    );
    r.step("move-allow-edited", 1, false, "strata same-release --allow registry-move releases/1.1.1/dev.json releases/1.1.2/prod.json");
}

/// The chapter `tag-name`: a tag that gets another name for the same build.
fn tag_name(r: &mut Runner) {
    r.chapter("tag-name", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile(DEV, A113, "1.1.3", "aws-stack@1.1.1", TAG_2404);
    r.step(
        "tag-compare",
        1,
        false,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.3",
    );
    // the page teaches the verdict, the note and the OS line; it never reads the other packs
    r.excerpt(
        "tag-compare-os",
        "tag-compare",
        r"^(aws-stack|root |note |~ \[os)|  +impact:",
    );
    r.step(
        "tag-same",
        0,
        false,
        "strata same-release aws-stack@1.1.1 aws-stack@1.1.3",
    );
    r.step("tag-json", 1, false, r#"strata compare aws-stack@1.1.1 aws-stack@1.1.3 --json | jq '{same, sameContent, maxImpact: .summary.maxImpact, renamed: [.packs[] | select(.tagRenamed) | .name]}'"#);
    r.step(
        "tag-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.3 -o releases/1.1.3/dev.json",
    );
    r.serve_here(); // the page describes what follows: the tag rename that it reports as drift
    r.tenant(|t| t.edit(DEV, A113, TAG_LTS2404));
    r.step(
        "tag-verify",
        1,
        false,
        "strata verify releases/1.1.3/dev.json",
    );
    // the page teaches the two notes and the verdict. formats.md keeps the whole report.
    r.excerpt(
        "tag-verify-notes",
        "tag-verify",
        r"^(aws-stack|root |note |~ \[os|DRIFT)|  +impact:",
    );
}

/// The chapter `manifest-pack`: an add-on profile whose manifest differs between two projects.
fn manifest_pack(r: &mut Runner) {
    r.chapter("manifest-pack", &[]);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.tenant(|t| t.addons(DEV, ADDONS_DEV, ADDONS_PACK, "restricted", "1.0.0"));
    r.tenant(|t| {
        t.addons(
            PROD,
            ADDONS_PROD,
            "64e3000000000000000000d1",
            "restricted",
            "1.0.0",
        )
    });
    // prod also holds 1.0.1, whose manifest sets another Pod Security level. Both versions exist
    // before the playground hands the tenant over, so every step of the page runs against it.
    r.tenant(|t| {
        t.addons(
            PROD,
            ADDONS_PROD2,
            "64e3000000000000000000d2",
            "baseline",
            "1.0.1",
        )
    });
    r.step(
        "manifest-record",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot platform-addons@1.0.0 -o addons/1.0.0/dev.json",
    );
    r.step(
        "manifest-record",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot platform-addons@1.0.0 -o addons/1.0.0/prod.json",
    );
    r.step(
        "manifest-compare",
        1,
        false,
        "strata compare addons/1.0.0/dev.json addons/1.0.0/prod.json",
    );
    // the page teaches the root line, the note, and the pack line with no impact line
    r.excerpt(
        "manifest-compare-note",
        "manifest-compare",
        r"^(platform-addons|root |note |~ )|  +impact:",
    );
    r.step(
        "manifest-same",
        0,
        false,
        "strata same-release addons/1.0.0/dev.json addons/1.0.0/prod.json",
    );
    manifest_pack_versions(r);
    manifest_pack_candidate(r);
}

/// The part of the chapter `manifest-pack` for a second version in prod.
fn manifest_pack_versions(r: &mut Runner) {
    r.step(
        "manifest-rerecord",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot platform-addons@1.0.1 -o addons/1.0.1/prod.json",
    );
    r.step(
        "manifest-different",
        1,
        false,
        "strata same-release addons/1.0.0/dev.json addons/1.0.1/prod.json",
    );
    r.step(
        "manifest-full",
        1,
        false,
        "strata compare addons/1.0.0/dev.json addons/1.0.1/prod.json --full",
    );
    // the page teaches the one line that differs, which only --full prints
    r.excerpt(
        "manifest-full-diff",
        "manifest-full",
        r"^([$]|--- |[+][+][+] |[-+] +pod-security)",
    );
    // the JSON document holds the same text in `unified` of the manifest
    r.step(
        "manifest-full-json",
        1,
        false,
        "strata compare addons/1.0.0/dev.json addons/1.0.1/prod.json --full --json | jq -c '.packs[] | select(.manifests) | .manifests[] | {op, name, unified}'",
    );
}

/// The part of the chapter `manifest-pack` for a manifest pack in a candidate file.
fn manifest_pack_candidate(r: &mut Runner) {
    // A manifest pack in a candidate file: a layer of type manifest, with no registry. Its record
    // differs from the published version by the minted uid alone (#150)
    r.step(
        "manifest-candidate",
        0,
        true,
        "PALETTE_PROJECT=dev strata candidate platform-addons@1.0.0 -o addons/candidate.yaml",
    );
    r.step("manifest-candidate", 0, false, "cat addons/candidate.yaml");
    r.step(
        "manifest-candidate-record",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot --from-candidate addons/candidate.yaml -o addons/candidate.json",
    );
    r.step(
        "manifest-candidate-same",
        0,
        false,
        "strata same-release addons/candidate.json addons/1.0.0/dev.json",
    );
    r.step(
        "manifest-candidate-same",
        0,
        false,
        "strata same-release addons/candidate.json addons/1.0.0/dev.json --json | jq -c '.pairs[] | {status, mintedUidOnly}'",
    );
}
