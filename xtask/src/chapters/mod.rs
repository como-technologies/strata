//! Each chapter builds the tenant that one page needs and runs the commands that page shows.
//!
//! A chapter reads as the page does: the tenant, then the steps, with a change to the tenant
//! where the page describes one. `serve_here` marks a state the playground can hand over.
//!
//! One file holds the chapters of one group of pages: the first pages, the everyday loop, the
//! command pages, each group of use cases, and the pages of How it works. `ids.rs` and `edits.rs`
//! hold what the files share. This file holds the settings, the candidate, and the cluster
//! `payments` of dev with its add-on profiles, which several files use.

use crate::book::Runner;
use ids::{
    ADDONS_DEV, ADDONS_PACK, DEV, OBSERVABILITY, OBSERVABILITY_PACK, PAYMENTS, PAYMENTS_APPS,
    PAYMENTS_APPS_PACK,
};

mod automation;
mod commands;
mod content;
mod drift;
mod edits;
mod fields;
mod fleet;
mod ids;
mod objects;
mod observer;
mod pipeline;
mod preflight;
mod release;
mod repave;
mod start;
mod the_loop;

/// The project and the dates of the commits that the merge-gate chapters need, so a commit is the
/// same on every machine. The runner gives each step the other git settings: see [`isolate_git`].
///
/// [`isolate_git`]: crate::git::isolate_git
const GIT: &[(&str, &str)] = &[
    ("PALETTE_PROJECT", "dev"),
    ("GIT_AUTHOR_DATE", "2026-09-10T10:00:00+00:00"),
    ("GIT_COMMITTER_DATE", "2026-09-10T10:00:00+00:00"),
];

const DEV_ENV: &[(&str, &str)] = &[("PALETTE_PROJECT", "dev")];

const CANDIDATE_YAML: &str = "\
name: aws-stack
version: 1.1.0
cloudType: aws
registry: Public Repo
layers:
  - name: ubuntu-aws
    version: \"24.04\"
  - name: kubernetes
    version: 1.36.2
  - name: cni-cilium-oss
    version: 1.20.1
  - name: csi-aws-ebs
    version: 1.65.0
  - name: cert-manager
    version: 1.21.1
";

/// The manifest of the pack `log-shipper` of `observability@1.0.0`.
const LOG_SHIPPER: &str = "\
apiVersion: v1
kind: ConfigMap
metadata:
  name: log-shipper
  namespace: observability
data:
  endpoint: logs.example.com:443";

/// The manifest of the pack `payments-quota` of `payments-apps@1.0.0`.
const PAYMENTS_QUOTA: &str = "\
apiVersion: v1
kind: ResourceQuota
metadata:
  name: payments-quota
  namespace: payments
spec:
  hard:
    pods: \"50\"";

/// The add-on profiles that the cluster `payments` of dev runs beside its release, as the field
/// test found many clusters (#615): `platform-addons`, `observability`, and `payments-apps`.
const PAYMENTS_ADDONS: [&str; 3] = [ADDONS_DEV, OBSERVABILITY, PAYMENTS_APPS];

/// Publish the add-on profiles of [`PAYMENTS_ADDONS`] in dev, each at 1.0.0. Palette refuses one
/// pack name in two profiles of a cluster, so each profile has a pack of its own name.
fn payments_addons(r: &mut Runner) {
    r.tenant(|t| t.addons(DEV, ADDONS_DEV, ADDONS_PACK, "restricted", "1.0.0"));
    r.tenant(|t| {
        let profile = (OBSERVABILITY, "observability", "1.0.0");
        t.manifest_addon(
            DEV,
            profile,
            ("log-shipper", OBSERVABILITY_PACK),
            LOG_SHIPPER,
        )
    });
    r.tenant(|t| {
        let profile = (PAYMENTS_APPS, "payments-apps", "1.0.0");
        t.manifest_addon(
            DEV,
            profile,
            ("payments-quota", PAYMENTS_APPS_PACK),
            PAYMENTS_QUOTA,
        )
    });
}

/// Create or update the cluster `payments` of dev: it runs the profile version `release`, then
/// each add-on profile of [`PAYMENTS_ADDONS`].
fn payments_cluster(r: &mut Runner, release: &str) {
    let [platform, observability, apps] = PAYMENTS_ADDONS;
    let profiles = [release, platform, observability, apps];
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &profiles, &[]));
}

/// Run every chapter against `r`.
pub fn run(r: &mut Runner) {
    start::run(r);
    the_loop::run(r);
    commands::run(r);
    release::run(r);
    drift::run(r);
    content::run(r);
    repave::run(r);
    objects::run(r);
    fleet::run(r);
    pipeline::run(r);
    observer::run(r);
    automation::run(r);
    preflight::run(r);
    fields::run(r);
}
