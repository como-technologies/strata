//! The chapters of the content that the root covers or omits: macros, images, invalid packs,
//! missing digests, and masked values.

use super::DEV_ENV;
use super::edits::{
    CERT_MANAGER_TAG_DELETED, IMAGE_1, IMAGE_2, MACRO_REGION, PRODUCT_BUILD, PRODUCT_DIGEST_A,
    PRODUCT_DIGEST_B, PRODUCT_MIRROR, PRODUCT_TAG, SECRET_VALUE,
};
use super::ids::{
    A111, A114, A115, A118, A119, A1110, A1110_PROD, A1113, A1114, A1115, A1116, A1117, ADDONS_DEV,
    ADDONS_PACK, CHART_PACK, CHARTS_PROBE, DEV, DEV_GROUP, PAYMENTS, PROD,
};
use crate::book::Runner;
use crate::tenant::Edit;

/// Run the chapters of the content that the root covers or omits.
pub(super) fn run(r: &mut Runner) {
    macros(r);
    os_image(r);
    image_tag(r);
    invalid_pack(r);
    no_digest(r);
    masked_value(r);
}

/// The chapter `macros`: a project macro that the values of a pack read.
fn macros(r: &mut Runner) {
    // A project macro that the values of a pack read. The root covers its name, and a record
    // with --macros holds the digest of its value. The add-on profile that the cluster group pins
    // reads a second macro in its manifest.
    r.chapter("macros", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A114, "1.1.4", "aws-stack@1.1.1", MACRO_REGION);
    r.tenant(|t| {
        t.addons(
            DEV,
            ADDONS_DEV,
            ADDONS_PACK,
            "\"{{.spectro.macro.podSecurity}}\"",
            "1.0.0",
        )
    });
    r.tenant(|t| t.set_macro(DEV, "region", "us-east-2"));
    r.tenant(|t| t.set_macro(DEV, "podSecurity", "baseline"));
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A114], &[]));
    r.tenant(|t| t.cluster_group(DEV, DEV_GROUP, "dev-group", &[ADDONS_DEV], &[]));
    r.step(
        "macros-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.4 -o releases/1.1.4/dev.json",
    );
    r.step(
        "macros-record",
        0,
        true,
        "strata snapshot cluster:payments --macros -o runs/payments.json",
    );
    r.step(
        "macros-digests",
        0,
        false,
        "jq .source.cluster.macroValues runs/payments.json",
    );
    r.step(
        "macros-record-version",
        0,
        true,
        "strata snapshot aws-stack@1.1.4 --macros -o releases/1.1.4/dev-macros.json",
    );
    r.step(
        "macros-digests-version",
        0,
        false,
        "jq .source.macroValues releases/1.1.4/dev-macros.json",
    );
    r.step(
        "macros-record-group",
        0,
        true,
        "strata snapshot clustergroup:dev-group --macros -o runs/dev-group.json",
    );
    r.step(
        "macros-digests-group",
        0,
        false,
        "jq .source.cluster.macroValues runs/dev-group.json",
    );
    r.serve_here(); // state 1: the records are taken, and each macro has its first value
    macros_verify(r);
}

/// The part of the chapter `macros` where each macro gets another value.
fn macros_verify(r: &mut Runner) {
    r.tenant(|t| t.set_macro(DEV, "region", "eu-west-1"));
    r.tenant(|t| t.set_macro(DEV, "podSecurity", "restricted"));
    r.step(
        "macros-verify-release-full",
        0,
        false,
        "strata verify releases/1.1.4/dev.json",
    );
    r.excerpt(
        "macros-verify-release",
        "macros-verify-release-full",
        r"^(aws-stack|root |note |OK)",
    );
    r.step(
        "macros-verify-cluster-full",
        1,
        false,
        "strata verify runs/payments.json",
    );
    r.excerpt(
        "macros-verify-cluster",
        "macros-verify-cluster-full",
        r"^(aws-stack|root |note |DRIFT)",
    );
    r.step(
        "macros-verify-version-full",
        1,
        false,
        "strata verify releases/1.1.4/dev-macros.json",
    );
    r.excerpt(
        "macros-verify-version",
        "macros-verify-version-full",
        r"^(aws-stack|root |note |DRIFT)",
    );
    r.step(
        "macros-verify-group-full",
        1,
        false,
        "strata verify runs/dev-group.json",
    );
    r.excerpt(
        "macros-verify-group",
        "macros-verify-group-full",
        r"^(platform-addons|root |note |DRIFT)",
    );
}

/// The chapter `os-image`: an OS pack that names its machine image.
fn os_image(r: &mut Runner) {
    // An OS pack that names its machine image in the annotation imageId. The root does not
    // cover the annotations, and compare, verify, and same-release name a new image (#189)
    r.chapter("os-image", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A118, "1.1.8", "aws-stack@1.1.1", IMAGE_1);
    r.step(
        "os-image-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.8 -o releases/1.1.8/dev.json",
    );
    r.step(
        "os-image-annotations",
        0,
        false,
        r#"jq '.packs[] | select(.layer == "os") | .annotations' releases/1.1.8/dev.json"#,
    );
    r.serve_here(); // state 1: the record is taken, and the OS pack names its first image
    r.tenant(|t| t.edit(DEV, A118, IMAGE_2));
    r.step(
        "os-image-verify-full",
        1,
        false,
        "strata verify releases/1.1.8/dev.json",
    );
    r.excerpt(
        "os-image-verify",
        "os-image-verify-full",
        r"^(aws-stack|root |note |DRIFT)",
    );
    r.step(
        "os-image-compare-json",
        1,
        false,
        "strata compare releases/1.1.8/dev.json aws-stack@1.1.8 --json | jq -c '{same, imageIdsChanged}'",
    );
    // same-release reads the imageId of each pack too: one root, two images (#229)
    r.step(
        "os-image-same-release",
        1,
        false,
        "strata same-release releases/1.1.8/dev.json aws-stack@1.1.8",
    );
}

/// The chapter `image-tag`: a vendor rebuilds an image under one tag, and three ways to name it show
/// the move.
fn image_tag(r: &mut Runner) {
    // The vendor gives a tag another image. Palette is not told, so the record verifies. A later
    // version of the pack names a build tag, a tag of another registry, or a digest (#630).
    r.chapter("image-tag", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A1113, "1.1.13", "aws-stack@1.1.1", PRODUCT_TAG);
    r.profile(DEV, A1114, "1.1.14", "aws-stack@1.1.1", PRODUCT_BUILD);
    r.profile(DEV, A1115, "1.1.15", "aws-stack@1.1.1", PRODUCT_MIRROR);
    r.profile(DEV, A1116, "1.1.16", "aws-stack@1.1.1", PRODUCT_DIGEST_A);
    r.profile(DEV, A1117, "1.1.17", "aws-stack@1.1.1", PRODUCT_DIGEST_B);
    r.step(
        "image-tag-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.13 -o releases/1.1.13/dev.json",
    );
    r.step(
        "image-tag-images",
        0,
        false,
        r#"jq -r '.packs[].images[]? | select(contains("product"))' releases/1.1.13/dev.json"#,
    );
    // the vendor gives the tag another image: Palette, the pack, and the record are the same
    r.step(
        "image-tag-verify-full",
        0,
        false,
        "strata verify releases/1.1.13/dev.json",
    );
    r.excerpt(
        "image-tag-verify",
        "image-tag-verify-full",
        r"^(aws-stack|root |OK)",
    );
    r.step(
        "image-tag-compare-same",
        0,
        false,
        "strata compare releases/1.1.13/dev.json aws-stack@1.1.13",
    );
    r.step(
        "image-tag-compare-build-full",
        1,
        false,
        "strata compare releases/1.1.13/dev.json aws-stack@1.1.14",
    );
    r.excerpt(
        "image-tag-compare-build",
        "image-tag-compare-build-full",
        r"^(root |~ \[|      impact|      [~+-] image)",
    );
    r.step(
        "image-tag-compare-mirror-full",
        1,
        false,
        "strata compare releases/1.1.13/dev.json aws-stack@1.1.15",
    );
    r.excerpt(
        "image-tag-compare-mirror",
        "image-tag-compare-mirror-full",
        r"^(root |~ \[|      impact|      [~+-] image)",
    );
    r.step(
        "image-tag-record-digest",
        0,
        true,
        "strata snapshot aws-stack@1.1.16 -o releases/1.1.16/dev.json",
    );
    r.step(
        "image-tag-compare-digest-full",
        1,
        false,
        "strata compare releases/1.1.16/dev.json aws-stack@1.1.17",
    );
    r.excerpt(
        "image-tag-compare-digest",
        "image-tag-compare-digest-full",
        r"^(root |~ \[|      impact|      [~+-] image)",
    );
}

/// The chapter `invalid-pack`: a registry that deletes the tag of a pack in use.
fn invalid_pack(r: &mut Runner) {
    // a registry deletes the tag of a pack that a published version uses. Palette marks the
    // pack ref invalid, the root stays, and snapshot, verify, and compare say so (#108).
    r.chapter("invalid-pack", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A119, "1.1.9", "aws-stack@1.1.1", &[]);
    r.step(
        "invalid-pack-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.9 -o releases/1.1.9/dev.json",
    );
    r.serve_here(); // state 1: the record is taken, and each pack is valid
    r.tenant(|t| t.edit(DEV, A119, CERT_MANAGER_TAG_DELETED));
    r.step(
        "invalid-pack-verify-full",
        0,
        false,
        "strata verify releases/1.1.9/dev.json",
    );
    r.excerpt(
        "invalid-pack-verify",
        "invalid-pack-verify-full",
        r"^(aws-stack|root |  \[addon|OK|INVALID)",
    );
    r.step(
        "invalid-pack-snapshot",
        0,
        true,
        "strata snapshot aws-stack@1.1.9 -o releases/1.1.9/dev-now.json",
    );
    r.step(
        "invalid-pack-fields",
        0,
        false,
        r#"jq -c '.packs[] | select(.invalid) | {key, invalid, invalidReason}' releases/1.1.9/dev-now.json"#,
    );
    r.step(
        "invalid-pack-compare-json",
        0,
        false,
        "strata compare releases/1.1.9/dev.json releases/1.1.9/dev-now.json --json | jq -c '{same, packsInvalid: .summary.packsInvalid}'",
    );
}

/// The chapter `no-digest`: a pack with no upstream digest.
fn no_digest(r: &mut Runner) {
    // a pack with no upstream digest, as a registry that Palette does not sync gives it
    r.chapter("no-digest", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile(
        DEV,
        A115,
        "1.1.5",
        "aws-stack@1.1.1",
        &[Edit::NoDigest("csi-aws-ebs")],
    );
    r.step(
        "nodigest-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.5 -o releases/1.1.5/dev.json",
    );
    r.step(
        "nodigest-source",
        0,
        false,
        r#"jq -r '.packs[] | "\(.name) \(.hash.upstreamSource)"' releases/1.1.5/dev.json"#,
    );
    r.step(
        "nodigest-verify-full",
        1,
        false,
        "strata verify releases/1.1.5/dev.json",
    );
    r.excerpt(
        "nodigest-verify",
        "nodigest-verify-full",
        r"^(aws-stack|root |note |  \[csi|      upstream unknown|UNVERIFIED)",
    );
    r.step(
        "nodigest-allow-full",
        0,
        false,
        "strata verify releases/1.1.5/dev.json --allow-no-digest",
    );
    r.excerpt("nodigest-allow", "nodigest-allow-full", r"^(note |OK)");
    r.step(
        "nodigest-same-release",
        1,
        false,
        "strata same-release aws-stack@1.1.1 aws-stack@1.1.5",
    );
    no_digest_chart(r);
}

/// The part of the chapter `no-digest` for a chart of a private Helm registry.
fn no_digest_chart(r: &mut Runner) {
    // a candidate layer from a private Helm registry: Palette lists no chart of it, and makes
    // the pack when it creates the profile (#154)
    r.tenant(|t| t.registries(DEV));
    r.tenant(|t| t.private_charts(DEV));
    r.write_file(
        "charts-probe.yaml",
        "name: charts-probe\nversion: 1.0.0\ntype: add-on\ncloudType: all\nlayers:\n  - name: podinfo\n    version: 6.7.1\n    registry: charts-auth\n    values: |\n      replicaCount: 2\n",
    );
    r.step("nodigest-chart-file", 0, false, "cat charts-probe.yaml");
    // no registry has the chart in the listing: the error names the registry that Palette does
    // not sync (#123)
    r.step("nodigest-chart-unsynced", 2, true, "strata packs podinfo");
    r.step(
        "nodigest-chart-record",
        0,
        true,
        "strata snapshot --from-candidate charts-probe.yaml -o charts/candidate.json",
    );
    r.step(
        "nodigest-chart-behind",
        2,
        false,
        "strata behind charts/candidate.json",
    );
    r.step(
        "nodigest-chart-pack",
        0,
        false,
        "jq -c '.packs[] | {name, type, layer, version, packUid, upstreamSource: .hash.upstreamSource}' charts/candidate.json",
    );
    r.tenant(|t| t.chart_profile(DEV, CHARTS_PROBE, CHART_PACK, "6.7.1"));
    r.step(
        "nodigest-chart-published",
        0,
        true,
        "strata snapshot charts-probe@1.0.0 -o charts/published.json",
    );
    r.step(
        "nodigest-chart-pack",
        0,
        false,
        "jq -c '.packs[] | {name, type, layer, version, packUid, upstreamSource: .hash.upstreamSource}' charts/published.json",
    );
    r.step(
        "nodigest-chart-packs",
        0,
        false,
        "strata packs podinfo --registry charts-auth",
    );
    r.step(
        "nodigest-chart-same",
        1,
        false,
        "strata same-release charts/candidate.json charts/published.json",
    );
    r.step(
        "nodigest-chart-allow",
        0,
        false,
        "strata same-release charts/candidate.json charts/published.json --allow no-digest",
    );
    r.step(
        "nodigest-chart-allow",
        0,
        false,
        "strata same-release charts/candidate.json charts/published.json --allow no-digest --json | jq -c '.pairs[] | {status, mintedUidOnly}'",
    );
}

/// The chapter `masked-value`: a pack value that Palette serves as a mask.
fn masked_value(r: &mut Runner) {
    // A pack value whose schema format is password. Palette serves the mask in place of the
    // secret, so an equal root does not show an equal secret (#163)
    r.chapter("masked-value", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A1110, "1.1.10", "aws-stack@1.1.1", SECRET_VALUE);
    r.profile(PROD, A1110_PROD, "1.1.10", "aws-stack@1.1.1", SECRET_VALUE);
    r.step(
        "masked-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.10 -o releases/1.1.10/dev.json",
    );
    r.step(
        "masked-values",
        0,
        false,
        r#"jq -c '.packs[] | select(.values | contains("********")) | {key}' releases/1.1.10/dev.json"#,
    );
    r.quiet("PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.10 -o releases/1.1.10/prod.json");
    r.serve_here(); // state 1: the two records are taken, and each one holds the mask
    r.step(
        "masked-same-release",
        0,
        false,
        "strata same-release releases/1.1.10/dev.json releases/1.1.10/prod.json",
    );
    r.step(
        "masked-compare-full",
        0,
        false,
        "strata compare releases/1.1.10/dev.json releases/1.1.10/prod.json",
    );
    r.excerpt(
        "masked-compare",
        "masked-compare-full",
        r"^(aws-stack|root |note )",
    );
    r.step(
        "masked-verify-full",
        0,
        false,
        "strata verify releases/1.1.10/dev.json",
    );
    r.excerpt(
        "masked-verify",
        "masked-verify-full",
        r"^(aws-stack|root |note |  \[addon\] cert|      masked|OK)",
    );
    r.step(
        "masked-same-release-json",
        0,
        false,
        "strata same-release releases/1.1.10/dev.json releases/1.1.10/prod.json --json | jq -c '.pairs[] | {status, masked}'",
    );
}
