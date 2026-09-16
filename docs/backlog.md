# Backlog

Each `##` section is one GitHub issue; its `###` headings become `##` in the posted body. `scripts/open-backlog-issues.sh` (or `just backlog-issues`) creates missing issues and updates the body of existing ones by title, so this file is the source of truth. The `labels:` line is read by the script and is not part of the body. Paragraphs are single lines: GitHub renders markdown, hard wraps only add noise.

## Compare a record against a running cluster

labels: feature

### Background

In the model a cluster is an **instance** that Palette reconciles to a **definition**. The team needs to know when a cluster does not run the profile version its tier file pins, which is the release-versus-run question. Today `strata compare` accepts profile uids, `name@version`, and snapshot files only (`src/reference.rs`), so the walkthrough's cluster step can only compare two profile records.

A cluster carries its applied profiles in the same shape strata already parses: `GET /v1/spectroclusters/{uid}` returns `spec.clusterProfileTemplates[]`, each with `packs[]` as `V1PackRef` including `digest`, `values`, `tag`, `version`, `packUid`, `registryUid`. Manifest contents come from `GET /v1/spectroclusters/{uid}/profiles/packs/manifests` (`V1SpectroClusterProfilesPacksManifests.profiles[].spec.packs[]`). Palette's own "profile update available" notification is computed from these same digests.

### Plan

1. `src/reference.rs`: add `Ref::Cluster { uid, profile: Option<String> }` parsed from `cluster:<uid>` and `cluster:<uid>/<profile-name>`.
2. `src/api.rs`: add `Palette::fetch_cluster(uid)` returning the two documents, and a `RawCluster` type next to `RawProfile`.
3. `src/snapshot.rs`: build a snapshot from one applied template; set `source.type = "cluster"`, record the cluster uid and the profile uid the template came from. With no profile name given and several templates attached, merge their packs (keys stay `layer/name`) and record every profile uid.
4. `src/ops.rs`: route `Ref::Cluster` through `snapshot_for`; `verify` gains nothing new, a cluster snapshot is just a snapshot.
5. Tests: `tests/verify.rs` with `MemoryApi` serving a cluster document built from the `b` fixture; assert `compare` of profile `b` against the cluster is `IDENTICAL`, and against profile `a` shows the drift.
6. `docs/walkthrough.md` day 5: replace the "not available yet" note with the command.

### Acceptance criteria

- [ ] `strata compare releases/1.1.1/prod.json cluster:<uid>` prints `IDENTICAL` and exits 0 for a cluster running 1.1.1
- [ ] The same command against a cluster on 1.1.0 shows the changed layers and exits 1
- [ ] `cluster:<uid>/<profile>` selects one attached profile; an unknown profile name is an error with exit 2
- [ ] `source.type` is `cluster` and the JSON document carries the cluster uid
- [ ] `strata schema snapshot` includes the new fields
- [ ] Walkthrough day 5 shows the real command and output

## Read the UPSTREAM digest from the OCI registry

labels: feature

### Background

`UPSTREAM` today hashes the `digest` Palette stamps on the pack ref, with the registry summary digest as fallback (`Pack::from_api` in `src/snapshot.rs`, recorded in `hash.upstreamSource`). For packs the pipeline builds, the authoritative fact is the OCI manifest digest in the `packs` and `packs-dev` ECR repositories, which have tag immutability on. Reading that digest directly makes `UPSTREAM` independent of what Palette's field tracks (see the investigation issue) and turns "same version, different digest" into a detected violation of the immutability rule.

### Plan

1. `src/config.rs`: an `UpstreamSource` enum (`palette`, `oci`) selected by `--upstream-source`, default `palette`.
2. `src/oci.rs`: a small `OciRegistry` trait with `manifest_digest(reference) -> Result<String>`, an HTTP implementation using `HEAD /v2/<name>/manifests/<tag>` and the `Docker-Content-Digest` header, and a `MemoryRegistry` for tests. Registry endpoint and credentials come from the Palette registry object (`GET /v1/registries/oci/basic/{uid}`) or from `STRATA_OCI_*` variables for ECR.
3. `src/snapshot.rs`: when `oci` is selected, resolve each pack's `<registry>/<name>:<version>` and set `hash.upstreamRaw` to the manifest digest and `hash.upstreamSource` to `oci.digest`.
4. Tests: `MemoryRegistry` returning two digests for the same tag across two snapshots; assert `UPSTREAM` only.
5. `docs/model.md`: document the source order and the new value of `upstreamSource`.

### Acceptance criteria

- [ ] `strata snapshot <ref> --upstream-source oci` records `hash.upstreamSource = "oci.digest"` for every pack
- [ ] Pushing the same version with different content to the sandbox registry shows `UPSTREAM` with no `IDENTITY` or `AUTHORED`
- [ ] A registry that cannot be reached is an error with exit 2, never a silent fallback
- [ ] Tests cover both sources and the failure

## Confirm what Palette's pack-ref digest tracks

labels: investigation

### Background

The `UPSTREAM` flag relies on the `digest` field of a profile's pack ref (`V1PackRef.digest`). The SDK comment says changing it triggers a pack upgrade, which suggests it tracks registry content, but strata has never compared it with the registry digest on a live profile because no tenant with a cluster profile has been available. Every snapshot records which field fed the hash in `hash.upstreamSource`, so the comparison is one fetch once a profile exists.

### Plan

1. In a sandbox tenant, create a profile from Public Repo packs (the five in `examples/`), publish it, and run `strata snapshot <name>@<version> -o before.json`.
2. Compare `packs[].refDigest` with `packs[].registryDigest` in the snapshot; record whether they are equal per pack and per pack type.
3. Edit one value of the published version in the Palette UI, save, and run `strata verify before.json`; record whether Palette allowed the edit in place or created a new version.
4. Bump a pack version on the profile and repeat step 2 to see whether `refDigest` follows the pack or the profile.
5. Write the results into `docs/model.md` under "The three hashes per layer" and adjust the fallback order in `Pack::from_api` if the field does not track registry content.

### Acceptance criteria

- [ ] `docs/model.md` states what `refDigest` tracks, with the sandbox evidence
- [ ] `docs/model.md` states what Palette does on an in-place edit of a published version
- [ ] `hash.upstreamSource` fallback order matches the finding
- [ ] The "Known limits" entry in the README is removed or rewritten

## Snapshot a candidate from repository files before it is published

labels: feature

### Background

At pull request time the candidate profile version does not exist in Palette yet; the pipeline runs `terraform plan` and the comparison happens after the dev apply. A profile repository holds everything needed to build the snapshot anyway: `profile/profile.yaml` (name, type, version, layers with exact pins and registry) and `profile/values/` (one values file per layer). Building the snapshot from those files moves the evidence onto the pull request itself.

### Plan

1. `src/candidate.rs`: parse `profile.yaml` and the referenced values files; look each pack up with `GET /v1/packs/{name}/registries/{registryUid}` for `packUid`, and `GET /v1/packs?filters=spec.name=<name>` for the registry digest and default values.
2. `src/snapshot.rs`: build a `Snapshot` with `source.template = "candidate"`, no uid, `fetchedAt` as usual.
3. `src/main.rs`: `strata snapshot --from-profile profile/profile.yaml`.
4. Tests: a fixture profile repository under `tests/fixtures/profile-repo/` and a `MemoryApi` serving the pack lookups; assert the root equals the root built from the equivalent raw API responses.
5. `docs/walkthrough.md`: move the comparison from "after the dev apply" to the pull request in day 2.

### Acceptance criteria

- [ ] `strata snapshot --from-profile profile/profile.yaml -o candidate.json` works with only registry access
- [ ] For a merged version, the candidate root equals the root of the snapshot taken from Palette after the apply
- [ ] Missing values files and unknown pack versions are errors with exit 2
- [ ] `source.template` is `candidate`

## One command for the promotion check

labels: feature

### Background

After a release reaches prod, the team compares the test and prod records and expects `IDENTICAL`. Between dev and test the expected pattern is different: the release build rewrites pins from the `packs-dev` registry to `packs`, so every pack shows `IDENTITY UPSTREAM` and none shows `AUTHORED`. Both rules live in `docs/walkthrough.md` day 4 as prose and two manual comparisons.

### Plan

1. `src/ops.rs`: `promotion_check(records: &[Snapshot]) -> PromotionReport` that orders records by tier (dev, test, stage, prod), checks dev to test for "registry move only" (every changed pack has `identity && upstream && !authored`, variables unchanged), and checks test, stage, prod pairwise for equal roots.
2. `src/main.rs`: `strata promote-check releases/<version>/*.json`, tier taken from the file name.
3. `src/render/`: a terminal line per pair and a `strata-promotion/1` JSON document with a `legend`.
4. Tests: fixtures with a dev record whose registry uid and digests differ from test, and a prod record with one edited value.

### Acceptance criteria

- [ ] `strata promote-check releases/1.1.1/*.json` exits 0 when dev to test is a registry move only and test, stage, prod are identical
- [ ] An `AUTHORED` change between dev and test exits 1 and names the pack and the leaf
- [ ] A root difference between test and prod exits 1 and names the tiers
- [ ] `--format json` writes `strata-promotion/1`
- [ ] Walkthrough day 4 uses the command

## Bulk verify with a summary document

labels: feature

### Background

The nightly immutability check in `docs/walkthrough.md` is a shell loop over `releases/*/*.json` that maps tier to project uid by hand and greps for `DRIFT`. One command with one document makes the job a single step and gives the scheduled workflow something to upload.

### Plan

1. `src/main.rs`: `strata verify` accepts several paths and globs.
2. `src/config.rs`: `--project-map tiers.yaml` mapping tier name to `PALETTE_PROJECT_UID`; the tier is the record's file name.
3. `src/ops.rs`: `verify_many` returning per-record results; the API client is built once per project.
4. `src/render/`: one terminal line per record, and a `strata-verify-summary/1` JSON document with counts and the drifted records.
5. Exit 1 if any record drifted, 2 if any record could not be checked.

### Acceptance criteria

- [ ] `strata verify 'releases/**/*.json' --project-map tiers.yaml` checks every record against its tier's project
- [ ] Output is one line per record and, with `--format json`, one summary document
- [ ] Exit codes: 0 all unchanged, 1 any drift, 2 any error
- [ ] The scheduled workflow in the walkthrough is one command

## Release binaries from CI

labels: ci

### Background

A developer and a CI runner must run the same binary. Today `ci.yml` uploads a Linux x86_64 build as a workflow artifact and every laptop builds its own. Static binaries with checksums and signatures make `curl` plus a checksum the install procedure, and match the plan's rule that every published artifact is signed.

### Plan

1. `.github/workflows/release.yml` on tag `v*`: build `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `aarch64-apple-darwin`, `x86_64-apple-darwin`.
2. Write `SHA256SUMS`, sign with cosign keyless, and attach binaries, sums, and signatures to the GitHub release.
3. `justfile`: a `release` recipe that tags from `Cargo.toml` and refuses when the tag exists.
4. `README.md`: an install section with the download and checksum commands.

### Acceptance criteria

- [ ] Pushing `v0.3.0` produces a release with four binaries, `SHA256SUMS`, and signatures
- [ ] `cosign verify-blob` passes on each binary
- [ ] The README install procedure works on a clean Linux machine
- [ ] `ci.yml` and `release.yml` share the build step

## Reusable workflow for pull request comments

labels: ci

### Background

Every profile and cluster repository needs the same three steps when a bump pull request opens: run `strata compare --format markdown`, post the result as the pull request comment, and add the `repave` label when `summary.maxImpact` is `repave`. The plan puts shared pipeline logic in the tools repository as reusable workflows, and forbids logic that lives only in YAML.

### Plan

1. `scripts/pr-comment.sh <A> <B> [--label-threshold repave]`: runs the comparison in both formats, posts or updates one comment (marker `<!-- strata -->`), sets the label from `summary.maxImpact`.
2. A reusable workflow `strata-comment.yml` in the tools repository that downloads the release binary and calls the script; inputs `a`, `b`, `label`, `threshold`.
3. Profile and cluster templates call it from their `pr.yml` and `bump.yml`.

### Acceptance criteria

- [ ] A bump pull request shows exactly one strata comment, updated on each push
- [ ] The `repave` label is added when `maxImpact` is `repave` and removed when it is not
- [ ] The script runs locally with `gh` and produces the same comment
- [ ] No profile or cluster repository carries its own copy of the logic

## Structured errors in JSON mode

labels: feature

### Background

Scripts and AI clients read `--format json` from stdout. An error today is a sentence on stderr and exit 2, which a program has to parse. The library already has one variant per failure in `src/error.rs`, so a machine-readable form is a serialization away.

### Plan

1. `src/error.rs`: `Error::kind() -> &'static str` (`missing-credential`, `api`, `transport`, `not-json`, `profile-not-found`, `not-snapshot`, ...), and `Error::to_json()` with `kind`, `message`, and the variant's fields (`status`, `path`).
2. `src/main.rs`: when the selected format is JSON, write `{"error": {...}}` as one line on stderr; keep the sentence otherwise.
3. Tests in `tests/cli.rs` asserting the shape for a missing credential and a non-snapshot file.
4. `docs/output.md`: document the error object.

### Acceptance criteria

- [ ] `strata compare --json a.json missing.json` writes one JSON line to stderr with `kind` and `message`
- [ ] Every `Error` variant has a `kind` and a test asserts the mapping is total
- [ ] Exit codes are unchanged
- [ ] `docs/output.md` documents the object

## Resolve `name@latest` and sort versions

labels: feature

### Background

Scripts often want the newest published version of a profile without knowing its version string. `Ref::NameVersion` requires an exact version and `Palette::resolve` does an exact match on the dashboard listing. Palette versions come in several forms (`1.30.6`, `22.04`, `LTS__24.x`), so ordering needs a natural sort, not semver.

### Plan

1. `src/version.rs`: a natural version comparator (numeric runs compared as numbers, the rest as text) with tests over Palette's forms.
2. `src/api.rs`: `Palette::resolve_latest(name)` picking the highest version among the listing entries with that name.
3. `src/reference.rs`: treat `version == "latest"` as a request to resolve; record the resolved version in `source.version` and keep `latest` in `source.ref`.
4. `docs/walkthrough.md`: use `aws-pxk@latest` in the nightly example where it fits.

### Acceptance criteria

- [ ] `strata snapshot aws-pxk@latest` records the highest version in the scope
- [ ] `22.04` sorts above `20.04`, `1.30.6` above `1.29.10`, `LTS__24.x` above `LTS__22.04`
- [ ] A name with no versions is `ProfileNotFound`

## Retries and rate limits for the Palette API

labels: robustness

### Background

The pipeline calls strata from many jobs, often in bursts after a fan-out. A single 5xx or a rate limit must not fail a release. `HttpApi::get_json` in `src/api.rs` makes one attempt.

### Plan

1. `src/api.rs`: a `RetryPolicy` (attempts, base delay, max delay) on `HttpApi`; retry on 5xx, 429, and transport errors with exponential backoff; honor `Retry-After`.
2. Progress notes for each retry so a job log shows what happened.
3. `MemoryApi::failing_n_times(path, n, status, body)` for tests.
4. Configuration through `Config` and `STRATA_RETRIES`.

### Acceptance criteria

- [ ] A `MemoryApi` that fails twice with 503 and then succeeds yields a successful snapshot
- [ ] A permanent 500 exits 2 with the last error after the configured attempts
- [ ] `Retry-After: 2` is honored
- [ ] 4xx other than 429 is not retried

## Consumer names and projects

labels: feature

### Background

`source.consumers.clusters` holds cluster uids from the profile's `inUseClusterUids`. Before end of life the team wants names, projects, and states, so the consumer check in the walkthrough is one readable command instead of a uid list.

### Plan

1. `src/api.rs`: `Palette::cluster_summary(uid)` from `GET /v1/spectroclusters/{uid}` (name, project uid, `status.state`).
2. `src/main.rs`: `strata consumers <ref>` with terminal and JSON output (`strata-consumers/1`).
3. `docs/walkthrough.md`: replace the `jq .source.consumers` step.

### Acceptance criteria

- [ ] `strata consumers aws-pxk@1.0.0` lists each consumer with name, project, and state
- [ ] Exit 0 when there are no consumers, 1 when there are, so a delete step can branch on it
- [ ] `--format json` writes `strata-consumers/1`

## Variables diff at the variable level

labels: ux

### Background

A new profile variable shows as ten leaf lines in the report, one per field (`db_password.name`, `db_password.required`, ...). A reviewer wants one line: variable `db_password` added, sensitive, required. The leaf list is right for machines and wrong for people.

### Plan

1. `src/diff.rs`: `VariablesDiff` gains `added[]`, `removed[]`, `changed[]` grouped by variable name with the changed fields; the leaf `diff[]` stays for compatibility.
2. `src/render/terminal.rs` and `markdown.rs`: one line per added or removed variable, one line per changed field of a changed variable.
3. Tests over the existing fixtures (`db_password` added, `cluster_env` default changed).

### Acceptance criteria

- [ ] The terminal report shows `+ variable db_password (required, sensitive)` instead of ten leaf lines
- [ ] Changed variables show only the changed fields
- [ ] The JSON document keeps `variables.diff[]` and adds the grouped form
- [ ] `strata schema diff` includes the new fields

## Decide whether presets and the pack schema join the authored hash

labels: design

### Background

Strata does not hash pack presets or the pack schema. A selected preset shows as a values change, which is correct today. The schema describes the values and can change under the same pack version, which would be an upstream change strata does not see. `docs/model.md` lists this as a known gap.

### Plan

1. Collect real examples: two versions of one pack with different schema or presets from `examples/packs/`.
2. Decide with the team whether schema belongs in `UPSTREAM` (it comes from the registry) and whether presets belong anywhere.
3. If the hash scheme changes, bump the snapshot schema id to `strata-snapshot/2` and add a migration note to `docs/model.md`.

### Acceptance criteria

- [ ] `docs/model.md` states the rule for presets and schema
- [ ] If the scheme changed, old snapshots still load and `compare` reports the schema mismatch clearly
- [ ] The "Known limits" entry is removed

## Publish the library for the Rust Palette client

labels: library

### Background

The plan replaces Terraform with a Rust Palette client later. Strata is already a library with a thin CLI (`src/lib.rs`, `src/ops.rs`), so that client can build snapshots and diffs without the binary once the public API is stable.

### Plan

1. Review the public surface of `strata::{api, config, snapshot, diff, ops, render}`; mark internals `pub(crate)`.
2. Write the semver policy: crate version for the API, schema ids for the documents.
3. `cargo doc` clean with `missing_docs` as an error; examples in the module docs.
4. Publish to the internal registry or document the git dependency.

### Acceptance criteria

- [ ] `cargo doc --no-deps` builds with no warnings
- [ ] Another crate depends on `strata` and builds a snapshot from a `MemoryApi` without the CLI
- [ ] `docs/model.md` states the semver policy
