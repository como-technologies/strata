# Examples

Real packs from Spectro Cloud's **Public Repo** registry at several versions, plus
cluster profiles built from them so `strata` has real releases to record,
compare, and verify without touching a tenant.

```
examples/
  fetch-examples.sh          pulls packs (GET only), builds profiles, runs the tool
  INDEX.md                   generated: what was pulled, versions, digests, reports
  packs/<name>/<version>.json   one real pack version: values, digest, packUid, schema, template, dependencies
  packs/<name>/_entity.json     GET /v1/packs/<name>/registries/<publicRepoUid>  (readme stripped)
  packs/<name>/_summaries.json  GET /v1/packs?filters=spec.name=<name>, filtered to the Public Repo
  profiles/raw/<profile>@<ver>/ profile.json, manifests.json, variables.json in the shapes the tool reads
  profiles/<profile>@<ver>.json snapshots (Merkle root, per-pack hashes, canonical values)
  clusters/raw/<cluster>/       cluster.json, manifests.json, variables.json in the shapes the tool reads
  clusters/<cluster>.json       cluster snapshots: what the cluster runs, hashed like the profile
  reports/                   compare output: text reports and one strata-diff/1 JSON document
```

## What is real and what is synthesized

- **Real:** everything under `packs/`. Values, digests, pack UIDs, tags, schema,
  and dependencies come straight from the registry through the Palette API.
- **Synthesized:** the profiles. No tenant profile was used. Each profile is the
  `V1ClusterProfile` shape wrapped around real packs, so the tool's `--from-raw`
  path reads it exactly as it would read an API response. The pack ref `digest`
  is copied from the registry pack digest. On a real profile Palette stamps that
  field itself, and whether it carries the same value is the open question noted
  in the top-level README.

The cluster `workload-a` is synthesized the same way. It runs `aws-pxk@1.1.0`
while `1.1.1` is proposed, so its snapshot has the root of the `1.1.0` record,
and the reports show one comparison that is `IDENTICAL` and one that is not.

The three profiles:

| profile          | contents                                                                 | shows                       |
|------------------|--------------------------------------------------------------------------|-----------------------------|
| `aws-pxk@1.0.0`  | OS, Kubernetes, CNI, CSI at the oldest pulled version; addons at newest   | baseline                    |
| `aws-pxk@1.1.0`  | same packs, infra layers at the newest pulled version                     | `IDENTITY` and `UPSTREAM` moves, values diffs between real defaults |
| `aws-pxk@1.1.1`  | same as 1.1.0 plus one attached manifest and one profile variable         | `AUTHORED` only, no pack change |

## What was pulled

Newest three versions of each pack in the Public Repo registry, values included:

| pack             | layer | versions                 |
|------------------|-------|--------------------------|
| `ubuntu-aws`     | os    | 20.04, 22.04, 24.04      |
| `kubernetes`     | k8s   | 1.35.3, 1.35.6, 1.36.2   |
| `cni-cilium-oss` | cni   | 1.19.6, 1.20.0, 1.20.1   |
| `csi-aws-ebs`    | csi   | 1.63.1, 1.64.0, 1.65.0   |
| `cert-manager`   | addon | 1.20.3, 1.21.0, 1.21.1   |

`metrics-server` is in the default pack list but not in the Public Repo, so it
was skipped. `INDEX.md` has pack UIDs and digests.

Two things the real data shows well. Between `aws-pxk@1.0.0` and `1.1.0` the
OS pack moves 20.04 to 24.04 with `IDENTITY UPSTREAM` but no `AUTHORED`: the
two versions' values differ only in one comment line, and canonicalization
sees through that. The Cilium pack moves 1.19.6 to 1.20.1 with 46 changed, 42
added, and 16 removed leaves in the default values, which is what a chart
upgrade looks like when you can see it.

## Regenerate

```
export PALETTE_API_KEY=...            # or SPECTROCLOUD_APIKEY, or put it in ~/.palette.env
export PALETTE_PROJECT_UID=...        # optional
cargo build --release && ./examples/fetch-examples.sh   # or: just examples-fetch
```

Knobs: `PACKS="kubernetes cni-cilium-oss"`, `VERSIONS_PER_PACK=5`, `CLOUD=azure`,
`EXAMPLES_DIR=/somewhere/else`. `OFFLINE=1` rebuilds the per-version files,
profiles, snapshots, and reports from the cached `_entity.json` and
`_summaries.json` without calling the API. The caches keep full values only
for the kept versions to stay small, so an offline rebuild can use the same or
fewer versions per pack; more needs a live fetch. The pack list endpoint caps
`limit` at 100 and the script pages by offset.

The script only issues GET requests.
