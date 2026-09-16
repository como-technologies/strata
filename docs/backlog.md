# Backlog

Each section below is one GitHub issue. `scripts/open-backlog-issues.sh` opens
them with `gh` once the repository has a GitHub remote. The `labels:` line is
read by the script and is not part of the issue body.

## Compare a record against a running cluster

labels: feature

**Why.** In the model, a cluster is an instance that Palette reconciles to a
definition. The team needs to know when a cluster does not run the profile
version that its tier file pins. Today strata compares two profile records
only.

**What.** Accept a cluster as a ref, for example `cluster:<uid>` and
`cluster:<uid>/<profile-name>` for one attached profile. Build a snapshot from
the profile templates applied to the cluster. Then `compare` and `verify` work
against a cluster with no other change.

**How.** `GET /v1/spectroclusters/{uid}` returns
`spec.clusterProfileTemplates[]`. Each template has `packs[]` in the same
`V1PackRef` shape as a profile, including `digest`. Manifest contents come
from `GET /v1/spectroclusters/{uid}/profiles/packs/manifests`. Record
`source.type = "cluster"` and the cluster uid.

**Done when** a `compare releases/1.1.1/prod.json cluster:<uid>` report shows
`IDENTICAL` for a cluster that runs 1.1.1, and shows the drift for one that
does not.

## Read the UPSTREAM digest from the OCI registry

labels: feature

**Why.** For packs that the pipeline builds, the OCI manifest digest in the
`packs` and `packs-dev` registries is authoritative. Those repositories have
tag immutability on. Palette's pack-ref digest is not yet confirmed to track
registry content (see the investigation issue).

**What.** An option, for example `--upstream-source oci`, that resolves each
pack's `<registry>/<name>:<version>` to its manifest digest and uses that as
`UPSTREAM`. Record the choice in `hash.upstreamSource` as `oci.digest`.

**How.** Read the registry endpoint and credentials from the Palette registry
object, or from environment variables for ECR. Use the OCI distribution API
`HEAD /v2/<name>/manifests/<tag>` and the `Docker-Content-Digest` header.

**Done when** a same-version-different-digest push to the sandbox registry
shows as `UPSTREAM` with `oci.digest` as the source.

## Confirm what Palette's pack-ref digest tracks

labels: investigation

**Why.** The `UPSTREAM` flag depends on the `digest` that Palette stamps on a
profile's pack ref. Strata has not compared that field with the registry
digest on a live profile, because no tenant with a cluster profile has been
available.

**What.** In a sandbox tenant: create a profile from Public Repo packs,
`strata snapshot` it, and compare `packs[].refDigest` with
`packs[].registryDigest` in the snapshot. Then edit one value in place and
`strata verify`. Record both results in `docs/model.md`.

**Done when** `docs/model.md` states what the field tracks, and the
`upstreamSource` fallback order reflects it.

## Snapshot a candidate from repository files before it is published

labels: feature

**Why.** At pull request time the candidate profile version does not exist in
Palette yet. The comparison runs after the dev apply. A snapshot built from
the repository files would move the evidence to the pull request itself.

**What.** `strata snapshot --from-profile <profile.yaml>`: read the layers,
the pins, and the values files, look up each pack in the registry for its
digest and default values, and build a snapshot with `source.template =
"candidate"`.

**Done when** the candidate snapshot of a merged version has the same root as
the snapshot taken from Palette after the apply.

## One command for the promotion check

labels: feature

**Why.** After a release reaches prod, the team compares the test and prod
records and expects `IDENTICAL`. Between dev and test the expected pattern is
`IDENTITY UPSTREAM` on every pack with no `AUTHORED`, because the release
build rewrites the registry. Today these are two manual comparisons and one
rule that lives in a document.

**What.** `strata promote-check releases/<version>/*.json` that verifies the
chain: dev to test shows only registry moves, and test, stage, and prod are
identical. Exit `1` with a plain reason otherwise.

**Done when** the release workflow calls it once after the prod apply.

## Bulk verify with a summary document

labels: feature

**Why.** The nightly job runs `verify` once per record in a shell loop and
maps tiers to project uids by hand.

**What.** `strata verify releases/**/*.json --project-map tiers.yaml` that
verifies each record against its tier's project, prints one line per record,
writes a `strata-verify-summary/1` JSON document, and exits `1` if any record
drifted.

**Done when** the scheduled workflow is one command and its output is one
document.

## Release binaries from CI

labels: ci

**Why.** A developer and a runner must run the same binary. Today each builds
its own.

**What.** A release workflow on tag `v*` that builds static binaries for
Linux x86_64 and aarch64 and macOS, writes SHA-256 checksums, signs with
cosign, and attaches them to the GitHub release.

**Done when** `curl` of the release asset and a checksum check is the install
procedure in the README.

## Reusable workflow for pull request comments

labels: ci

**Why.** Each profile and cluster repository needs the same three steps: run
`compare --format markdown`, post it as the pull request comment, and add the
`repave` label when `summary.maxImpact` is `repave`.

**What.** A reusable GitHub workflow in the tools repository that takes the
two refs and does these steps, with the label name and the threshold as
inputs.

**Done when** the profile and cluster templates call it and no repository
carries its own copy of the logic.

## Structured errors in JSON mode

labels: feature

**Why.** AI clients and scripts read `--format json`. An error today is a
sentence on stderr and exit `2`.

**What.** With `--format json`, write errors to stderr as a one-line JSON
object: `{"error": {"kind": "api", "status": 404, "path": ..., "message":
...}}`. The `kind` values follow the `Error` variants. The exit status stays.

**Done when** each `Error` variant has a `kind` and a test asserts the shape.

## Resolve `name@latest` and sort versions

labels: feature

**Why.** Scripts often want "the newest published version of this profile".
Today the caller must know the version string.

**What.** Accept `<name>@latest`. Sort versions with a natural order that
handles Palette's forms, for example `1.30.6`, `22.04`, and `LTS__24.x`.
Record the resolved version in `source.version`.

**Done when** `strata snapshot aws-pxk@latest` records the highest version in
the scope.

## Retries and rate limits for the Palette API

labels: robustness

**Why.** The pipeline calls strata from many jobs. A single 5xx or a rate limit
must not fail a release.

**What.** Retry idempotent GETs on 5xx and on connection errors with backoff,
honor `Retry-After`, and cap the attempts. Report the retries in progress
notes.

**Done when** a `MemoryApi` test that fails twice and then succeeds passes,
and a permanent failure still exits `2` with the last error.

## Consumer names and projects

labels: feature

**Why.** `source.consumers.clusters` holds cluster uids. Before end of life
the team wants to know which clusters, in which projects.

**What.** `strata consumers <ref>` that lists each consumer cluster with its
name, project, and state, from `GET /v1/spectroclusters/{uid}`. Add a
`--format json` variant.

**Done when** the end-of-life procedure in the walkthrough is one command.

## Variables diff at the variable level

labels: ux

**Why.** A new variable shows as ten leaf lines, one per field. A reviewer
wants "variable `db_password` added, sensitive, required".

**What.** Group variable leaves by variable name in the report and in the
JSON: added, removed, and changed variables with their changed fields.

**Done when** the terminal report shows one line per added or removed
variable.

## Decide whether presets and the pack schema join the authored hash

labels: design

**Why.** Strata does not hash pack presets or the pack schema. A selected
preset shows as a values change, which is correct today. The schema describes
the values and can change under the same pack version.

**What.** Decide, with the team, whether either belongs in `AUTHORED` or in
`UPSTREAM`. Record the decision in `docs/model.md`.

**Done when** the model page states the rule and the hash scheme version is
bumped if the rule changed.

## Publish the library for the Rust Palette client

labels: library

**Why.** The plan replaces Terraform with a Rust Palette client later. Strata
is already a library with a thin CLI.

**What.** Stabilize the public API of `strata::{api, snapshot, diff, ops}`,
document the semver policy for the schema ids, and publish to the internal
registry or as a git dependency.

**Done when** another crate depends on `strata` and builds a snapshot without
the CLI.
