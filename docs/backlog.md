# Backlog

Each `##` section is one GitHub issue; its `###` headings become `##` in the posted body. `scripts/open-backlog-issues.sh` (or `just backlog-issues`) creates missing issues and updates the body of existing ones by title, so this file is the source of truth. The `labels:` line is read by the script and is not part of the body. Paragraphs are single lines: GitHub renders markdown, hard wraps only add noise.

Strata is a tool, not a pipeline. Nothing here assumes a cloud provider, a registry product, a publishing tool, a CI system, a repository layout, or a set of environments. Where an example needs names, it uses the `examples/` data and calls them examples.

## Compare a record against a running cluster

labels: feature

### Background

A cluster is an instance that Palette reconciles to a profile version. The team needs to know when a cluster does not run the version its record says, which is the release-versus-run question. Today `strata compare` accepts profile uids, `name@version`, and snapshot files only (`src/reference.rs`), so the walkthrough's cluster step can only compare two profile records.

A cluster carries its applied profiles in the same shape strata already parses: `GET /v1/spectroclusters/{uid}` returns `spec.clusterProfileTemplates[]`, each with `packs[]` as `V1PackRef` including `digest`, `values`, `tag`, `version`, `packUid`, `registryUid`. Manifest contents come from `GET /v1/spectroclusters/{uid}/profiles/packs/manifests` (`V1SpectroClusterProfilesPacksManifests.profiles[].spec.packs[]`). Palette's own "profile update available" notification is computed from these same digests.

### Plan

1. `src/reference.rs`: add `Ref::Cluster { uid, profile: Option<String> }` parsed from `cluster:<uid>` and `cluster:<uid>/<profile-name>`.
2. `src/api.rs`: add `Palette::fetch_cluster(uid)` returning the two documents, and a `RawCluster` type next to `RawProfile`.
3. `src/snapshot.rs`: build a snapshot from one applied template; set `source.type = "cluster"`, record the cluster uid and the profile uid the template came from. With no profile name given and several templates attached, merge their packs (keys stay `layer/name`) and record every profile uid.
4. `src/ops.rs`: route `Ref::Cluster` through `snapshot_for`; `verify` gains nothing new, a cluster snapshot is just a snapshot.
5. Tests: `tests/verify.rs` with `MemoryApi` serving a cluster document built from the `b` fixture; assert `compare` of profile `b` against the cluster is `IDENTICAL`, and against profile `a` shows the drift.
6. `docs/walkthrough.md` day 5: replace the "not available yet" note with the command.

### Acceptance criteria

- [x] `strata compare <record>.json cluster:<uid>` prints `IDENTICAL` and exits 0 for a cluster that runs the recorded version
- [x] The same command against a cluster on an older version shows the changed layers and exits 1
- [x] `cluster:<uid>/<profile>` selects one attached profile; an unknown profile name is an error with exit 2
- [x] `source.cluster` carries the cluster uid, its name, and the applied profiles; `source.type` keeps the profile type
- [x] `strata schema snapshot` includes the new fields
- [x] Walkthrough day 5 shows the real command and output

## Read the UPSTREAM digest from the pack's registry

labels: feature

### Background

`UPSTREAM` today hashes the `digest` Palette stamps on the pack ref, with the registry summary digest as fallback (`Pack::from_api` in `src/snapshot.rs`, recorded in `hash.upstreamSource`). Both values come from Palette. For a pack in an OCI registry the authoritative fact is the manifest digest the registry itself reports for `<name>:<version>`. Reading it makes `UPSTREAM` independent of what Palette's field tracks (see the investigation issue) and turns "same version, different content" into a detected violation of the immutability rule, whichever registry holds the pack.

Palette knows several registry kinds: pack registries (`/v1/registries/pack`), OCI registries with `basic` or `ecr` authentication (`/v1/registries/oci`), and Helm registries. Strata must not assume any one of them, any registry product, or any cloud provider. The registry kind and endpoint come from the registry object the pack ref's `registryUid` points at. Credentials never come from Palette.

### Plan

1. `src/config.rs`: an `UpstreamSource` enum (`palette`, `registry`) selected by `--upstream-source`, default `palette`.
2. `src/registry.rs`: a `Registry` trait with `content_digest(name, version) -> Result<String>`, one implementation per registry kind that exposes a content digest, and a `MemoryRegistry` for tests. OCI first: `HEAD /v2/<name>/manifests/<tag>` and the `Docker-Content-Digest` header, with basic auth or a bearer token from the standard token flow. A registry kind with no implementation is an error that names the kind, never a fallback.
3. Endpoint and kind from `GET /v1/registries/metadata` and the registry object. Credentials from a Docker-style `config.json` keyed by registry host (`--registry-auth <file>`, default the Docker config path), so any login flow or credential helper works.
4. `src/snapshot.rs`: when `registry` is selected, resolve each pack's `<host>/<name>:<version>` and set `hash.upstreamRaw` to the digest and `hash.upstreamSource` to `registry.digest`.
5. Tests: `MemoryRegistry` returning two digests for the same tag across two snapshots; assert `UPSTREAM` only. A registry kind without an implementation is an error.
6. `docs/model.md`: document the source order and the new value of `upstreamSource`.

### Acceptance criteria

- [ ] `strata snapshot <ref> --upstream-source registry` records `hash.upstreamSource = "registry.digest"` for every pack
- [ ] Pushing the same version with different content to any OCI registry shows `UPSTREAM` with no `IDENTITY` or `AUTHORED`
- [ ] A registry that cannot be reached, or a registry kind strata cannot read, is an error with exit 2, never a silent fallback
- [ ] Nothing in the implementation names a registry product or a cloud provider
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

- [x] `docs/model.md` states what `refDigest` tracks, with the sandbox evidence
- [x] `docs/model.md` states what Palette does on an in-place edit of a published version
- [x] `hash.upstreamSource` fallback order matches the finding
- [x] The "Known limits" entry in the README is removed or rewritten

## Snapshot a candidate from local files before it is published

labels: feature

### Background

At review time the candidate profile version does not exist in Palette yet, so today the comparison runs after the version is published and the report arrives after the fact. Everything strata needs is known before publication: the layers with their pack name, version, and registry, and the values and manifests for each layer. A file that strata defines can carry that, whatever tool later publishes the version: Terraform, the API, or the UI. Building the snapshot from that file moves the evidence onto the review itself.

### Plan

1. `docs/model.md`: define the candidate file. `name`, `version`, `type`, `cloudType`, `variables`, and `layers[]` with `layer`, `name`, `version`, `registry` (name or uid), `values` as inline text or a file path, and `manifests[]`. It carries only what the snapshot already records.
2. `src/candidate.rs`: parse the file; look each pack up with `GET /v1/packs/{name}/registries/{registryUid}` for `packUid`, and `GET /v1/packs?filters=spec.name=<name>` for the registry digest and default values.
3. `src/snapshot.rs`: build a `Snapshot` with `source.template = "candidate"`, no uid, `fetchedAt` as usual.
4. `src/main.rs`: `strata snapshot --from-candidate <file>`.
5. Tests: a fixture under `tests/fixtures/candidate/` and a `MemoryApi` serving the pack lookups; assert the root equals the root built from the equivalent raw API responses.
6. `docs/walkthrough.md`: move the comparison to review time in day 2.

### Acceptance criteria

- [x] `strata snapshot --from-candidate profile.yaml -o candidate.json` needs pack lookups only; no profile has to exist
- [x] Once the version is published, the candidate root equals the root of the snapshot taken from Palette
- [x] Missing values files and unknown pack versions are errors with exit 2
- [x] `source.template` is `candidate`
- [x] The file format is documented and assumes no repository layout

## Check that several records are the same release

labels: feature

### Background

A team that publishes one version into several Palette projects, or several tenants, wants to know that each record is the same release. Today that is one `compare` per pair and a reading of each report. One difference can be intended: when a team promotes packs between registries and re-pins the profile, the same version shows `IDENTITY UPSTREAM` on each pack with no `AUTHORED`, because only `registryUid`, `packUid`, and the digest moved. `docs/walkthrough.md` day 4 describes both rules as prose.

### Plan

1. `src/ops.rs`: `same_release(records: &[Snapshot], policy: Policy) -> ChainReport` that compares consecutive records in the order given. `Policy::Exact` requires equal roots. `Policy::RegistryMove` also accepts a pair where every changed pack has `identity && upstream && !authored`, only `registryUid`, `packUid`, and the digest differ, and variables are unchanged.
2. `src/main.rs`: `strata same-release [--allow registry-move] <record>...`; the order comes from the command line, never from file names.
3. `src/render/`: a terminal line per pair and a `strata-chain/1` JSON document with a `legend`.
4. Tests: fixtures with a record whose registry uid and digests differ, and a record with one edited value.

### Acceptance criteria

- [x] `strata same-release a.json b.json c.json` exits 0 when every root is equal
- [x] With `--allow registry-move`, a pair that differs only by registry passes; an `AUTHORED` change exits 1 and names the pack and the leaf
- [x] A root difference exits 1 and names the two records
- [x] `--format json` writes `strata-chain/1`
- [x] Walkthrough day 4 uses the command

## Bulk verify with a summary document

labels: feature

### Background

The scheduled immutability check in `docs/walkthrough.md` is a shell loop over record files that sets the project for each record by hand and greps for `DRIFT`. One command with one document makes the job a single step and gives any scheduler something to keep. A record does not yet say which project it was taken in, so the loop has to know.

### Plan

1. `src/snapshot.rs`: record the project scope a snapshot was taken in as `source.projectUid`, empty for tenant scope, so a record carries what `verify` needs.
2. `src/main.rs`: `strata verify` accepts several paths and globs; `--project` overrides the recorded scope for every record.
3. `src/ops.rs`: `verify_many` returning per-record results; one API client per project scope.
4. `src/render/`: one terminal line per record, and a `strata-verify-summary/1` JSON document with counts and the drifted records.
5. Exit 1 if any record drifted, 2 if any record could not be checked.

### Acceptance criteria

- [x] `strata verify 'releases/**/*.json'` checks every record in the project it was recorded from
- [x] Output is one line per record and, with `--format json`, one summary document
- [x] Exit codes: 0 all unchanged, 1 any drift, 2 any error
- [x] The scheduled check in the walkthrough is one command

## Release binaries from CI

labels: ci

### Background

A developer and a CI runner must run the same binary. Today `ci.yml` uploads a Linux x86_64 build as a workflow artifact and every laptop builds its own. Static binaries with checksums and signatures make `curl` plus a checksum the install procedure, and let anyone verify the binary they run.

### Plan

1. `.github/workflows/release.yml` on tag `v*`: build `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `aarch64-apple-darwin`, `x86_64-apple-darwin`.
2. Write `SHA256SUMS`, sign with cosign keyless, and attach binaries, sums, and signatures to the GitHub release.
3. `justfile`: a `release` recipe that tags from `Cargo.toml` and refuses when the tag exists.
4. `README.md`: an install section with the download and checksum commands.

### Acceptance criteria

- [x] Pushing `v0.4.0` produces a release with four binaries, `SHA256SUMS`, and signatures
- [x] `cosign verify-blob` passes on each binary
- [x] The README install procedure works on a clean Linux machine
- [x] `ci.yml` and `release.yml` share the build step

## Post the comparison as a pull request comment

labels: ci

### Background

Any repository that reviews profile changes wants the same three steps when a pull request opens: run `strata compare --format markdown`, post the result as one comment that updates on each push, and add a label when `summary.maxImpact` reaches a threshold. The markdown works on any code host. The posting step needs a host API, so it belongs in a script that uses the host's CLI, and a repository calls the script from whatever runs its CI.

### Plan

1. `scripts/pr-comment.sh <A> <B> [--label repave] [--threshold repave]`: runs the comparison in both formats, posts or updates one comment (marker `<!-- strata -->`), and sets the label from `summary.maxImpact`. GitHub through `gh` first; the host-specific part is one function, so another host is one more function.
2. A reusable GitHub Actions workflow in this repository, `.github/workflows/comment.yml`, that downloads the release binary and calls the script; inputs `a`, `b`, `label`, `threshold`. Other CI systems call the script directly.
3. `README.md`: how to call the script and the workflow.

### Acceptance criteria

- [x] A pull request shows exactly one strata comment, updated on each push
- [x] The label is added when `maxImpact` reaches the threshold and removed when it does not
- [x] The script runs locally and produces the same comment
- [x] A repository needs no copy of the logic, only a call

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

- [x] `strata compare --json a.json missing.json` writes one JSON line to stderr with `kind` and `message`
- [x] Every `Error` variant has a `kind` and a test asserts the mapping is total
- [x] Exit codes are unchanged
- [x] `docs/output.md` documents the object

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

- [x] `strata snapshot aws-pxk@latest` records the highest version in the scope
- [x] `22.04` sorts above `20.04`, `1.30.6` above `1.29.10`, `LTS__24.x` above `LTS__22.04`
- [x] A name with no versions is `ProfileNotFound`

## Retries and rate limits for the Palette API

labels: robustness

### Background

CI jobs call strata in bursts, and Palette rate-limits. A single 5xx or a 429 must not fail a job. `HttpApi::get_json` in `src/api.rs` makes one attempt.

### Plan

1. `src/api.rs`: a `RetryPolicy` (attempts, base delay, max delay) on `HttpApi`; retry on 5xx, 429, and transport errors with exponential backoff; honor `Retry-After`.
2. Progress notes for each retry so a job log shows what happened.
3. `MemoryApi::failing_n_times(path, n, status, body)` for tests.
4. Configuration through `Config` and `STRATA_RETRIES`.

### Acceptance criteria

- [x] A `MemoryApi` that fails twice with 503 and then succeeds yields a successful snapshot
- [x] A permanent 500 exits 2 with the last error after the configured attempts
- [x] `Retry-After: 2` is honored
- [x] 4xx other than 429 is not retried

## Fail fast when Retry-After exceeds the wait ceiling

labels: robustness

### Background

Strata retries transient Palette API failures with a doubling wait and honors a `Retry-After` header as sent (#11, `Retry` in `src/api.rs`). A server that knows when capacity returns is the best source for the wait, so capping the value would only spend attempts against a server that already said no. But an unbounded wait has one bad case: a hard throttle or a misconfigured proxy sends `Retry-After: 3600`, and strata sleeps an hour inside a CI job, silently under `--quiet`, until the job's own timeout kills it. That is worse than either failing or capping.

The header means "come back at this time". When that time is further away than a job can reasonably wait, the right reading is to stop now and say so, so the scheduler can re-run later, not to wait less and retry into the same throttle.

### Plan

1. `src/api.rs`: `RetryPolicy` gains `max_retry_after: Duration`, default five minutes. `RetryPolicy::delay` is unchanged.
2. `src/api.rs`: in the retry loop, when the error carries a `Retry-After` above `max_retry_after`, return at once instead of sleeping.
3. `src/error.rs`: a `RetryAfterTooLong { path, status, retry_after, limit }` variant with kind `retry-after-too-long`, so the message and the JSON object name the seconds the server asked for and the ceiling. A script can tell "come back later" from a plain API failure.
4. `src/config.rs`: `STRATA_MAX_RETRY_AFTER` in seconds, read like `STRATA_RETRIES`; an invalid value is `invalid-setting`.
5. Tests in `tests/verify.rs`: a 429 with `Retry-After: 900` fails after one call with nothing slept; `Retry-After: 2` still waits two seconds; the ceiling from `Config` reaches the policy.
6. `README.md` and `docs/output.md`: state the default ceiling next to `STRATA_RETRIES` and add the new error kind.

### Acceptance criteria

- [ ] A `Retry-After` at or below the ceiling is honored as sent; above it, the command exits 2 at once with no sleep
- [ ] The error message names the seconds the server asked for and the ceiling
- [ ] `--format json` writes kind `retry-after-too-long` with `retryAfter` and `limit`
- [ ] `STRATA_MAX_RETRY_AFTER=900` raises the ceiling to fifteen minutes
- [ ] The README states the default ceiling

## Consumer names and projects

labels: feature

### Background

`source.consumers.clusters` holds cluster uids from the profile's `inUseClusterUids`. Before end of life the team wants names, projects, and states, so the consumer check in the walkthrough is one readable command instead of a uid list.

### Plan

1. `src/api.rs`: `Palette::cluster_summary(uid)` from `GET /v1/spectroclusters/{uid}` (name, project uid, `status.state`).
2. `src/main.rs`: `strata consumers <ref>` with terminal and JSON output (`strata-consumers/1`).
3. `docs/walkthrough.md`: replace the `jq .source.consumers` step.

### Acceptance criteria

- [x] `strata consumers aws-pxk@1.0.0` lists each consumer with name, project, and state
- [x] Exit 0 when there are no consumers, 1 when there are, so a delete step can branch on it
- [x] `--format json` writes `strata-consumers/1`

## Variables diff at the variable level

labels: ux

### Background

A new profile variable shows as ten leaf lines in the report, one per field (`db_password.name`, `db_password.required`, ...). A reviewer wants one line: variable `db_password` added, sensitive, required. The leaf list is right for machines and wrong for people.

### Plan

1. `src/diff.rs`: `VariablesDiff` gains `added[]`, `removed[]`, `changed[]` grouped by variable name with the changed fields; the leaf `diff[]` stays for compatibility.
2. `src/render/terminal.rs` and `markdown.rs`: one line per added or removed variable, one line per changed field of a changed variable.
3. Tests over the existing fixtures (`db_password` added, `cluster_env` default changed).

### Acceptance criteria

- [x] The terminal report shows `+ variable db_password (required, sensitive)` instead of ten leaf lines
- [x] Changed variables show only the changed fields
- [x] The JSON document keeps `variables.diff[]` and adds the grouped form
- [x] `strata schema diff` includes the new fields

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

## Publish the library crate

labels: library

### Background

Strata is already a library with a thin CLI (`src/lib.rs`, `src/ops.rs`). Another Rust program, for example a Palette client, a controller, or a review bot, can build snapshots and diffs without the binary once the public API is stable.

### Plan

1. Review the public surface of `strata::{api, config, snapshot, diff, ops, render}`; mark internals `pub(crate)`.
2. Write the semver policy: crate version for the API, schema ids for the documents.
3. `cargo doc` clean with `missing_docs` as an error; examples in the module docs.
4. Publish to a crate registry or document the git dependency.

### Acceptance criteria

- [ ] `cargo doc --no-deps` builds with no warnings
- [ ] Another crate depends on `strata` and builds a snapshot from a `MemoryApi` without the CLI
- [ ] `docs/model.md` states the semver policy
