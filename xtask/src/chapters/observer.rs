//! The chapter of the observer: a service in a cluster that checks the records of the team against
//! Palette on a timer.
//!
//! One tenant holds four things: a cluster with two profiles, and a published version that a
//! person edits after its record. It also holds a cluster that differs from its release, and a
//! registry with newer versions. The steps run `strata-observer once`, which the runner puts on the `PATH`. The
//! playground script `scripts/observer-playground.sh` serves the page of the same tenant.

use super::edits::RETAIN;
use super::ids::{A100, A110, ADDONS_DEV, ADDONS_PACK, DEV, LEDGER, PAYMENTS};
use crate::book::Runner;

/// The Git revision that the records of the chapter name.
const REVISION: &str = "3f9c2ab";

/// The environment of the chapter: the project, and the revision that the pipeline of the team gives
/// to the script of the records.
const ENV: &[(&str, &str)] = &[("PALETTE_PROJECT", "dev"), ("GIT_SHA", REVISION)];

/// The command line of a run of `once` for the cluster `uid`, with the folders of the pod in the
/// current folder.
fn once(uid: &str, state: &str) -> String {
    format!(
        "strata-observer once --cluster-uid {uid} --records records --work work --state {state}"
    )
}

/// Run the chapter of the observer.
pub(super) fn run(r: &mut Runner) {
    r.chapter("observer", ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.tenant(|t| t.addons(DEV, ADDONS_DEV, ADDONS_PACK, "restricted", "1.0.0"));
    // payments runs two profiles. ledger runs the release, and a person changed it on the cluster
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A100, ADDONS_DEV], &[]));
    r.tenant(|t| t.cluster(DEV, LEDGER, "ledger", &[A110], &[]));
    r.tenant(|t| t.on_cluster_reclaim(DEV, LEDGER, "Retain"));
    records(r);
    r.serve_here(); // state 1: the records are taken, and nobody has edited a version
    // a person edits aws-stack@1.0.0 through the API, after its record. payments has not taken it
    r.tenant(|t| t.edit(DEV, A100, RETAIN));
    answers(r);
}

/// The records of the pipeline of the team: one for each profile that its clusters run. The records
/// script writes them as Secrets, and a folder stands for the Secrets in the pod.
fn records(r: &mut Runner) {
    r.quiet("mkdir -p releases work state state-ledger");
    for (profile, file) in [
        ("aws-stack@1.0.0", "aws-stack_1.0.0"),
        ("aws-stack@1.1.0", "aws-stack_1.1.0"),
        ("platform-addons@1.0.0", "platform-addons_1.0.0"),
    ] {
        r.quiet(&format!(
            "strata snapshot {profile} -o releases/{file}.json"
        ));
    }
    r.step(
        "observer-records",
        0,
        false,
        "scripts/ci/observer-records.sh --revision \"$GIT_SHA\" releases/*.json \\\n  | grep -E '^(kind|  name|  [a-z]+[-_.0-9a-z]*[.]json(.gz)?):' | cut -c1-60",
    );
    // the pod reads the same files from its volume. This folder stands for the four Secrets
    r.quiet(&format!(
        "mkdir records && for f in releases/*.json; do gzip -c \"$f\" > \"records/$(basename \"$f\").gz\"; done && echo '{{\"baseline\":\"approved\",\"revision\":\"{REVISION}\"}}' > records/meta.json"
    ));
    r.step(
        "observer-first-seen",
        0,
        false,
        &format!(
            "scripts/ci/observer-records.sh --first-seen {PAYMENTS} | grep ' meta.json:' | cut -d' ' -f4 | base64 -d; echo"
        ),
    );
}

/// A step that runs `once` for the cluster `uid` and keeps the part of the summary that `filter`
/// selects. The exit code is 1: a finding exists.
fn summary(r: &mut Runner, name: &str, uid: &str, state: &str, filter: &str) {
    r.step(
        name,
        1,
        false,
        &format!("{} 2>/dev/null | jq {filter}", once(uid, state)),
    );
}

/// The answers of the observer after the edit: the finding of `payments`, the newer versions, the
/// limits, the log, the text of the edit, and the finding of `ledger`.
fn answers(r: &mut Runner) {
    summary(
        r,
        "observer-once",
        PAYMENTS,
        "state",
        "'{page, counts, findings}'",
    );
    summary(
        r,
        "observer-newer",
        PAYMENTS,
        "state",
        "-c '.newer[] | {name, version, newest, impact}'",
    );
    summary(
        r,
        "observer-limits",
        PAYMENTS,
        "state",
        "-c '.limits, (.profiles[] | {name, version, status, published: .published.status, clusterCopy: .clusterCopy.status})'",
    );
    r.step(
        "observer-log",
        1,
        false,
        &format!(
            "{} 2>&1 >/dev/null | sed -E 's/^time=[^ ]+ //; s/ duration=[0-9.]+m?s//' | grep 'msg=cycle'",
            once(PAYMENTS, "state")
        ),
    );
    // the Values tab of the page shows this text of the edit, which the cycle reads with --full
    r.step(
        "observer-text-all",
        1,
        false,
        "strata verify releases/aws-stack_1.0.0.json --full",
    );
    r.excerpt(
        "observer-text",
        "observer-text-all",
        r"^([$]|--- |[+][+][+] |[-+] +reclaimPolicy)",
    );
    // ledger runs aws-stack@1.1.0, and its own copy differs from the record
    summary(
        r,
        "observer-ledger",
        LEDGER,
        "state-ledger",
        "-c '.findings[] | {kind, profile, packs}'",
    );
}
