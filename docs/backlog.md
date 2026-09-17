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

## Confirm pack-ref digests for Helm and OCI packs

labels: investigation

### Background

`UPSTREAM` hashes the `digest` Palette stamps on each pack ref (`Pack::from_api` in `src/snapshot.rs`, recorded in `hash.upstreamSource`). The live example settled what that field tracks for `spectro` packs from a pack registry: it equals the digest the registry reports for the pack version (#3, `docs/model.md`). So strata does not need to read the digest from the registry for those packs, which was this issue's original plan.

Palette also pins packs from Helm registries and OCI registries (`/v1/registries/helm`, `/v1/registries/oci`), and the live run used neither. If Palette stamps those pack refs with something other than the registry's content digest, `UPSTREAM` would miss a change under the same version for them. Checking takes one profile, with no cluster.

### Plan

1. `examples/live/terraform`: an add-on profile version, attached to nothing, with one pack from a Helm registry and one from an OCI registry that the tenant can see.
2. `examples/live/verify.sh`: snapshot it and add its packs to the digest table in `summary.md`, with each pack's registry kind.
3. `docs/model.md`: state the result per registry kind under "What the pack-ref digest tracks".
4. Only for a kind whose digests differ: read `UPSTREAM` from that registry instead, behind `--upstream-source registry`. OCI through `HEAD /v2/<name>/manifests/<tag>` and `Docker-Content-Digest`, credentials from a Docker-style `config.json`, and an unreadable registry an error with exit 2, never a fallback. Nothing may name a registry product or a cloud provider.

### Acceptance criteria

- [x] The live example's summary shows the pack-ref and registry digests of a Helm pack and an OCI pack
- [x] `docs/model.md` states what the pack-ref digest tracks for each registry kind
- [x] Registry reading exists only for the kinds whose digests differ, with tests; or, if none differ, this issue closes with the evidence

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

- [x] A `Retry-After` at or below the ceiling is honored as sent; above it, the command exits 2 at once with no sleep
- [x] The error message names the seconds the server asked for and the ceiling
- [x] `--format json` writes kind `retry-after-too-long` with `retryAfter` and `limit`
- [x] `STRATA_MAX_RETRY_AFTER=900` raises the ceiling to fifteen minutes
- [x] The README states the default ceiling

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

- [x] `docs/model.md` states the rule for presets and schema
- [x] If the scheme changed, old snapshots still load and `compare` reports the schema mismatch clearly
- [x] The "Known limits" entry is removed

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

- [x] `cargo doc --no-deps` builds with no warnings
- [x] Another crate depends on `strata` and builds a snapshot from a `MemoryApi` without the CLI
- [x] `docs/model.md` states the semver policy

## Accept names for projects and clusters

labels: ux

### Background

Strata scopes requests with `PALETTE_PROJECT_UID` or `--project <uid>`, and names a cluster only by uid in `cluster:<uid>`. People know names. During the live verification the env file set `PALETTE_PROJECT=strata-test`, which strata ignores, so every command ran at tenant scope and found nothing until the uid was added by hand. The cluster uid had to come from a page URL or an API listing.

### Plan

1. `src/config.rs`: read `PALETTE_PROJECT` as a project name when `PALETTE_PROJECT_UID` is unset, and add `--project-name` next to `--project`. Resolve the name once, at tenant scope, through `GET /v1/dashboard/projects/metadata`; an unknown name is an error that lists the visible projects.
2. `src/reference.rs` and `src/api.rs`: `cluster:<x>` fetches `<x>` as a uid first; on a `ResourceNotFound`, it searches the project with `POST /v1/dashboard/spectroclusters/search` for a cluster named `<x>`. No match, or several, is an error that lists the candidates. A snapshot records the resolved uid.
3. Tests with `MemoryApi` for a name, an unknown name, and a uid that is also a name.
4. `README.md`, `docs/walkthrough.md`, `examples/live`: use names where a person types them.

### Acceptance criteria

- [x] `PALETTE_PROJECT=<name>` scopes every command to that project; an unknown name is an error with exit 2 that lists the visible projects
- [x] `--project-name` does the same for `verify`
- [x] `strata compare record.json cluster:<name>/<profile>` works; an unknown or ambiguous cluster name is an error with exit 2
- [x] Records carry uids, never names, so a rename does not break `verify`

## Decide whether a tag-only difference is an IDENTITY change

labels: design

### Background

`IDENTITY` hashes a pack's name, layer, type, tag, version, packUid, and registryUid. The live run found that Palette stores the tag a profile was authored with: a profile built with Terraform pins `24.04` for `ubuntu-aws`, while the registry names that pack version `LTS__24.x`, `LTS__24.04`, and `LTS__24.4.x`. Two profiles that pin the same pack uid through different tag names therefore differ in `IDENTITY`. The rules table maps `IDENTITY` on the OS or Kubernetes layer to `repave`, so the report would call it a node repave although both run the same pack version. A floating tag such as `1.x` is different: it changes which version an upgrade picks, so a change to or from one may belong in `IDENTITY`.

An in-place edit can also rename the tag of a published version and keep its pack uid. If an exact tag leaves `IDENTITY`, that edit changes no hash, and the scheduled `verify` stops seeing it. The provider's `immutable-clusterprofiles` flag refuses such an edit from Terraform, but Palette accepts it through the API, so the question stands with the flag on.

### Plan

1. In the live example, publish a version that differs only in the OS pack's tag name for the same pack uid, and record what Palette does to a cluster that moves to it: whether it reconciles or repaves. Also rename that tag on a published version in place through the API, and run `verify` on the record taken before the edit.
2. Decide with the team: keep the tag in `IDENTITY` and give a tag-only change its own impact level, or drop an exact tag from `IDENTITY` when the pack uid is unchanged.
3. `src/snapshot.rs` and `src/diff.rs`: implement the decision; if roots change, bump the snapshot schema id as `docs/model.md` requires.
4. Tests over two fixtures that differ only in a tag name, and one that changes a floating tag.

### Acceptance criteria

- [ ] `docs/model.md` states whether a tag-only change moves `IDENTITY` and what impact it reports, with the live evidence
- [ ] Comparing two profiles that pin the same pack uid under different tag names reports the decided result, with a test
- [ ] If roots change, the snapshot schema id changes and older records are refused by name
- [ ] `docs/model.md` states whether an in-place tag rename of a published version is `DRIFT`, and `verify` reports what it states, with a test

## Record the values a cluster gives its profile variables

labels: feature

### Background

A cluster snapshot records each applied profile's variable definitions and leaves out the value the cluster gave each variable, because some values are secrets (`isSensitive`). That keeps a cluster's root equal to the profile's, which the release-versus-run check needs. It also hides run-time configuration: two clusters on the same release with different variable values compare `IDENTICAL`, and nothing shows the difference.

### Plan

1. `src/snapshot.rs`: `source.cluster.variables[]` with the name, the profile uid, and the value of each non-sensitive variable. A sensitive variable is listed with `sensitive: true` and no value, and no hash of it either. None of it enters the root.
2. `src/diff.rs`: when both sides are clusters, an additive `clusterVariables` section in `strata-diff/1` with the changed values.
3. `src/render/`: the terminal and markdown reports list changed values under the variables line.
4. `src/ops.rs`: `verify` on a cluster record names the values changed since the record was taken, apart from the root verdict, as a cluster's counterpart to an in-place edit of its profile.
5. Tests over cluster fixtures with different values; the fixture secret must not appear in any document.

### Acceptance criteria

- [x] A cluster snapshot records non-sensitive variable values outside the root; sensitive values appear in no document
- [x] Comparing two cluster snapshots with different values shows the changes and still reports equal roots when the release is the same
- [x] `strata schema snapshot` and `strata schema diff` include the new fields
- [x] `verify` on a cluster record names a variable value changed since the record was taken, and `docs/output.md` states whether that alone is `DRIFT`

## Write a candidate file from a published profile

labels: feature

### Background

Teams that adopt candidate files start from profiles that are already published. The live example turns a record into a candidate file with a jq filter, `examples/live/candidate-from-snapshot.jq`, and `same-release` then shows that the file reproduces the published root. That belongs in strata: the filter tracks the snapshot shape by hand, and it writes values inline where a repository wants them in files.

### Plan

1. `src/candidate.rs`: `Candidate::from_snapshot`, with explicit tags and registry uids.
2. `src/main.rs`: `strata candidate <ref> [-o profile.yaml] [--files-dir DIR]`. With `--files-dir`, each layer's values and each manifest go to files the candidate references.
3. Tests: a candidate written from a fixture record resolves, through registry fixtures, to the record's root; after an in-place edit of the fixture version, `same-release` between the candidate's snapshot and the edited version fails and names the edited leaf.
4. `examples/live/verify.sh`: use the command and delete the jq filter. After the in-place edit, check that the candidate no longer reproduces the version.

### Acceptance criteria

- [x] `strata candidate <record>.json -o profile.yaml` writes a file that `snapshot --from-candidate` turns into the record's root
- [x] `--files-dir` writes values and manifests to files and references them
- [x] The live example uses the command
- [x] After an in-place edit of the published version, `same-release` between the candidate's snapshot and the version fails and names the edited leaf

## Keep the project scope in raw directories

labels: robustness

### Background

`snapshot --raw-dir` saves the API responses a snapshot was built from, and `snapshot --from-raw` rebuilds it offline. The project the responses came from is not saved. A rebuilt record has an empty `source.projectUid`, so `verify` later checks it at tenant scope or in whatever project the environment names, which is wrong for a project's profile or cluster.

### Plan

1. `src/api.rs`: `RawProfile` and `RawCluster` write `scope.json` with the project uid next to their documents, and read it back when present.
2. `src/ops.rs`: `snapshot_from_dir` applies the saved scope with `Snapshot::with_project`.
3. Tests: a round trip through a raw directory keeps `source.projectUid`; a raw directory without `scope.json` still loads, at tenant scope; a record rebuilt from a raw directory taken before an in-place edit of a project's profile reports `DRIFT` with no project set in the environment.

### Acceptance criteria

- [x] `snapshot <ref> --raw-dir d` followed by `snapshot --from-raw d` records the same `source.projectUid`
- [x] Raw directories written before the change still load
- [x] A record rebuilt from a raw directory taken before an in-place edit reports `DRIFT` with no project in the environment

## Live-verify a cluster that runs several profiles

labels: investigation

### Background

`cluster:<uid>` covers every profile applied to a cluster, merged in order, and `cluster:<uid>/<profile>` selects one. The live example's cluster runs a single profile, so the merge and the selection are verified only against fixtures assembled from the SDK models. A cluster with an infra profile and an add-on profile is the common shape.

### Plan

1. `examples/live/terraform`: an add-on profile with one small pack, attached to the cluster next to the infra profile.
2. `examples/live/verify.sh`: check that `cluster:<uid>` lists both profiles in order, that `cluster:<uid>/<name>` selects each, and that each profile's record compares `IDENTICAL` against its selection. Then, in `examples/live/in-place.sh` (#27), edit the add-on profile in place: its record reports `DRIFT` and the infra profile's record does not. When the edit reaches the cluster is #27's question. With `immutable-clusterprofiles` on, bump the add-on profile's version instead: the cluster's selection still matches the old version's record, and `consumers` lists the cluster under the old version only.
3. `examples/live/README.md`: the new checks and a recorded run.

### Acceptance criteria

- [ ] The new checks pass on a real cluster that runs two profiles
- [ ] An in-place edit of the add-on profile makes its record `DRIFT` and leaves the infra profile's record unchanged
- [ ] With `immutable-clusterprofiles` on, a version bump of the add-on profile leaves the cluster's selection equal to the old version's record
- [ ] The recorded run in `examples/live/README.md` includes them

## Live-verify the registry-move policy

labels: investigation

### Background

`same-release --allow registry-move` accepts a pair whose packs moved to another registry with the same tag, version, type, values, manifests, and params. The rule comes from a fixture that rewrites registry uids, pack uids, and digests. Whether a real copy of a pack in a second registry keeps the same tag names, pack type, and default values is unverified. If a real move changes one of them, a genuine promotion fails the check.

### Plan

1. In a tenant with two registries that hold the same pack version, publish one profile pinned to each. Make the second as a version bump with the provider's `immutable-clusterprofiles` flag on, so the pair is the preserved version and its clone with the packs re-pinned, the way a team promotes a move.
2. Run `strata same-release --allow registry-move` on their records and note every field that differs. Then edit one profile's values in place and run it again.
3. `docs/model.md`: state what a real registry move changes; adjust the policy in `src/ops.rs` if it differs from the fixture's, with a test.

### Acceptance criteria

- [ ] `docs/model.md` states which fields differ in a real registry move, with the evidence
- [ ] The policy passes a real registry move and fails the same pair after an in-place edit of one side's values

## Report drift from a scheduled verify

labels: ci

### Background

The live run showed that Palette accepts an in-place edit of a published version through the API: same uid, same version, no new version. Only `strata verify` noticed. The walkthrough runs `verify` on a schedule, but each team has to wire the schedule and the reporting itself. `scripts/pr-comment.sh` and the reusable comment workflow do that for pull requests; nothing does it for drift.

### Plan

1. `scripts/verify-report.sh <pattern>...`: run `strata verify --format json` over the records; open one issue per drifted record, with the report and a marker, update it on later runs, and close it when the record verifies again. A record that cannot be checked is reported too. Host calls sit in functions, as in `pr-comment.sh`.
2. A reusable workflow, `.github/workflows/verify.yml`, that installs the release binary and calls the script; the caller sets the schedule. Other CI systems call the script directly.
3. Tests with the fake `gh` from `tests/scripts.rs`.
4. `examples/live`: run the script around the in-place edit, so a real drifted record opens the issue and undoing the edit closes it. Do it with the provider's `immutable-clusterprofiles` flag off, through Terraform, and on, through the API, which Palette still accepts: the flag guards one client, and this report guards the tenant. A record whose version a legacy version bump overwrote is reported too (#27). Cluster records follow what #27 finds about an update that waits for a cluster.
5. `README.md` and `docs/walkthrough.md`: the call.

### Acceptance criteria

- [ ] A drifted record opens exactly one issue with the report, and a later clean run closes it
- [ ] A record that cannot be checked is reported, never silently skipped
- [ ] The script runs locally and through the reusable workflow
- [ ] In the live example, the in-place edit opens one issue with `immutable-clusterprofiles` off and with it on, and undoing the edit closes it

## List profiles, clusters, and projects

labels: feature

### Background

Every command takes a ref, and strata cannot show which refs exist. `<name>@<version>` resolves through `GET /v1/dashboard/clusterprofiles/metadata`, and a miss prints only `no cluster profile named 'cilium' with version '1.20.1' in this scope`. In that case `cilium` was a pack: the Palette UI shows `Cilium 1.20.1` as a layer of `strata-live-profile` 1.0.0, and the API names the pack `cni-cilium-oss`. The profile name, the cluster uid, and the project uid took three `curl` calls to find.

A pack is not a ref: a snapshot always records a whole profile, so the useful question is which profile versions carry a pack. The listings depend on the scope. With a `ProjectUid` header, the profile listing holds that project's profiles and the project listing holds only that project. Without the header, the profile listing holds tenant profiles only, the cluster listing holds the clusters of every project, and the project listing holds every project the key can see. #18 lets a person type names; this issue shows what there is to name.

### Plan

1. `src/api.rs`: `Palette::profile_listing()`, `Palette::cluster_listing()` from `GET /v1/dashboard/spectroclusters/meta`, and `Palette::project_listing()` from `GET /v1/dashboard/projects/metadata`, returning the documents as `profile_document` does. `resolve` and `resolve_latest` read through `profile_listing()`.
2. `src/ops.rs`: `list` returning the document `strata-list/1`: the scope's `projectUid` and `projectName`; `profiles[]` with `ref` (`<name>@<version>`), `name`, `version`, `uid`, and `cloudType`; `clusters[]` with `ref` (`cluster:<uid>`), `name`, `uid`, `state`, and `cloudType`; `projects[]` with `name` and `uid`. Profiles sort by name and then natural version order; clusters and projects sort by name. With `--packs`, each profile document is read for `type`, the consumer count, and `packs[]` with `layer`, `name`, `tag`, and `version`; a profile that cannot be read keeps its entry and carries the `error`. Projects are always read at tenant scope.
3. `src/main.rs`: `strata list [profiles] [clusters] [projects]`, profiles and clusters when no kind is named; `--project <uid>` as `verify` has it; `--packs`; `--pack <text>` keeps the profiles with a pack whose name contains the text, ignoring case, implies `--packs`, and lists profiles only unless kinds are named. Exit 0, or 2 on an error.
4. `src/render/`: terminal and markdown output grouped by kind, with the ref in the first column so it can be pasted; `strata schema list`.
5. `src/error.rs`: the `profile-not-found` message says that `strata list` shows the profiles in the scope and `strata list --pack <name>` the profiles that carry a pack of that name.
6. Tests: `MemoryApi` serving the three listings and profile documents, for sort order, `--pack`, a profile that cannot be read, and tenant scope for projects; CLI tests for `schema list` and the not-found message.
7. `README.md`, `docs/output.md`, `docs/walkthrough.md`, `docs/model.md`: the command, the document, the scope rules, and `strata-list/1` in the list of schema ids.

### Acceptance criteria

- [x] `strata list` prints each profile version as a `<name>@<version>` ref and each cluster as a `cluster:<uid>` ref, and `snapshot` takes each ref unchanged
- [x] `strata list --pack cilium` names every profile version with a Cilium layer, with the pack's name and version
- [x] `strata list projects` lists every project the key can see, with or without `PALETTE_PROJECT_UID`
- [x] `--project <uid>` lists another project with no change to the environment
- [x] `--format json` writes `strata-list/1`, and `strata schema list` prints its JSON Schema
- [x] A `<name>@<version>` that matches nothing points at `strata list`

## Live-verify clusters on a changed profile version, with and without immutable-clusterprofiles

labels: investigation

### Background

`cluster:<uid>` records what a cluster runs from the cluster's own copy of its profiles: `spec.clusterProfileTemplates` in `GET /v1/spectroclusters/<uid>`, and `GET /v1/spectroclusters/<uid>/profiles/packs/manifests`. The live run edited a published version in place, and `verify` on its record reported `DRIFT` (`docs/model.md`). That version ran on no cluster, and the edit is an optional manual step in `examples/live/README.md`.

How a version that clusters run can change depends on the Terraform provider's `immutable-clusterprofiles` feature_preview flag, which exists from provider 0.29.5. The live example pins 0.29.7 and leaves the flag off.

- Flag off, the legacy behavior: a content change without a version bump is a `PUT` to the published version, as `edit_next_in_place` does. A version bump on the same resource is a `PUT` too: the profile keeps its uid and takes the new version number, so the old version is gone, and the clusters that ran it point at a uid that now names another version.
- Flag on: the provider refuses a content change without a version bump at plan time. A version bump clones the profile (`POST /v1/clusterprofiles/<uid>/clone`), overwrites the clone with the configuration, and keeps the old version when `skip_destroy` is set. Palette does not enforce any of this: the API and the console still accept an in-place edit.

Nothing shows what these changes do to clusters. Take cluster A created from `P@1.0.0`, then an in-place edit of 1.0.0, then cluster B created from the edited 1.0.0. Both clusters name the same profile uid and version. Palette lists the changes that wait for a cluster separately, in `GET /v1/spectroclusters/<uid>/profileUpdates`. If A's copy keeps the old content until the update is applied, `compare cluster:A cluster:B` shows the edit, which is right, although nothing in the report says that both sides name the same version. If A's copy takes the edit at once, strata reports content that A does not run yet, and A and B compare `IDENTICAL` while they differ.

`verify` compares roots only. After a legacy version bump that changes no content, a record of 1.0.0 verifies unchanged although 1.0.0 no longer exists.

### Plan

1. `examples/live/terraform`: an `immutable_profiles` variable that sets the flag through `feature_preview`, with `skip_destroy` and `create_before_destroy` on the profiles when it is set; `just live-down` still removes every version. A second cluster, B, created only when `second_cluster` is set. `edit_addon_in_place` and `addon_version`, which edit and bump the add-on profile the clusters run (#23): its one layer is a ConfigMap, where an edit changes nothing a running cluster depends on, unlike the release's storage class, which Kubernetes does not let change.
2. `examples/live/in-place.sh`: run the sequence once with the flag off and once with it on, keeping each stage under `runs/<UTC time>/in-place/<off|on>/`. With the flag on, check that `tf.sh plan` refuses the in-place edit, then make the edit through the API.
   1. Record cluster A, the add-on version, and A's `profileUpdates`.
   2. Edit 1.0.0 in place and create cluster B. Record A again without applying the update, with its `profileUpdates`, and record B.
   3. Apply the waiting update to A and record A a third time.
   4. Undo the edit and bump the add-on version. Record both clusters, the old version, and the new one.

   At each stage, run `compare cluster:A cluster:B`, `verify` on the first records of A and of the add-on version, `consumers` of the add-on version, and `compare` of the add-on record against `cluster:A`. After both runs, `same-release` between the bumped version's records from the two runs shows whether the clone carried anything over from the version it copied. Remove B at the end.
3. `docs/model.md`, under "Editing a published version in place": for each flag setting, what an in-place edit and a version bump do to a cluster that runs the version, before and after the update is applied, and to a cluster created afterward, and whether a clone carries content over, with the evidence.
4. `src/ops.rs`: `verify` reports a record whose uid now names another version, even when the roots match. `src/diff.rs` and `src/render/`: when both sides name the same profile uid and version and their roots differ, the report says the version was edited in place. Tests over fixtures for both.
5. Only if the evidence needs it: `cluster:<uid>` reads what the cluster applied, not what waits for it, and a cluster snapshot records a waiting update outside the root.
6. `in-place.sh` asserts the recorded answers, so a later run fails when Palette or the provider changes them. `examples/live/README.md`: the script replaces the optional in-place step, with a recorded run for each flag setting.

### Acceptance criteria

- [ ] `docs/model.md` states, for each setting of `immutable-clusterprofiles`, what an in-place edit and a version bump do to a cluster that runs the version, before and after the update is applied, and to a cluster created afterward, with the live evidence
- [ ] `cluster:<uid>` records the content a cluster runs, and `compare cluster:A cluster:B` shows a change exactly when the two clusters run different content
- [ ] A comparison of two sides with the same profile uid and version and different roots says the version was edited in place
- [ ] `verify` exits 1 for a record whose uid now names another version, even when the roots match
- [ ] `examples/live/in-place.sh` runs with the flag off and on against a version a cluster runs, and `examples/live/README.md` records both runs
