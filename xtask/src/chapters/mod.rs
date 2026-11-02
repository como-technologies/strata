//! Each chapter builds the tenant that one page needs and runs the commands that page shows.
//!
//! A chapter reads as the page does: the tenant, then the steps, with a change to the tenant
//! where the page describes one. `serve_here` marks a state the playground can hand over.
//!
//! One file holds the chapters of one group of pages: the first pages, the everyday loop, the
//! command pages, and each group of use cases. `ids.rs` and `edits.rs` hold what the files share.

use crate::book::Runner;

mod automation;
mod commands;
mod content;
mod drift;
mod edits;
mod fields;
mod fleet;
mod ids;
mod objects;
mod preflight;
mod release;
mod repave;
mod start;
mod the_loop;

/// The git settings that the merge-gate chapters need, so a commit is the same on every machine.
const GIT: &[(&str, &str)] = &[
    ("PALETTE_PROJECT", "dev"),
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_CONFIG_SYSTEM", "/dev/null"),
    ("GIT_AUTHOR_NAME", "dev"),
    ("GIT_AUTHOR_EMAIL", "dev@example.com"),
    ("GIT_COMMITTER_NAME", "dev"),
    ("GIT_COMMITTER_EMAIL", "dev@example.com"),
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
    automation::run(r);
    preflight::run(r);
    fields::run(r);
}
