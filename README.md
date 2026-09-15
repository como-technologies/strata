# palette-profile-diff

Diff two Spectro Cloud Palette cluster profile versions and say *what kind* of
change happened, not just that something did. One static Rust binary; the output
is a colored terminal report, a markdown pull request comment, or a versioned,
self-describing JSON document built for machine and AI consumption.

```
palette-profile-diff my-profile@1.2.0 my-profile@1.3.0
palette-profile-diff 64f0…a1 64f0…b2 --full
palette-profile-diff compare A B --format markdown        # pull request comment
palette-profile-diff compare A B --format json            # for scripts and AI clients
palette-profile-diff snapshot my-profile@1.2.0 -o rel.json # the record step
palette-profile-diff verify rel.json                       # immutability check
palette-profile-diff schema diff                           # JSON Schema of the diff document
```

Build with `cargo build --release` (or `just build`); the binary is
`target/release/palette-profile-diff`. Set `PALETTE_API_KEY`, and
`PALETTE_PROJECT_UID` for project-scoped profiles. `PALETTE_API` overrides the
base URL for self-hosted Palette. Values missing from the environment are read
from `~/.palette.env`.

## The model

Each pack on a profile is a Merkle node with three sub-hashes, so a report can
name which one moved:

| flag       | hashed from                                                  | Palette concept                              | 12-factor reading                          |
|------------|--------------------------------------------------------------|----------------------------------------------|--------------------------------------------|
| `IDENTITY` | pack name, layer, type, tag, version, packUid, registryUid    | which pack version the layer references      | the release points at a different build    |
| `UPSTREAM` | the `digest` on the profile's pack ref                        | pack content in the registry                 | the build changed under the same version, which the lifecycle outline forbids |
| `AUTHORED` | canonical values YAML, attached manifest contents, params     | the pack's values and manifests on the profile | release-level config                       |

`node = sha256(identity + upstream + authored)`, and
`root = sha256(variablesHash, [key, node] in profile order)`. Two profiles with
the same root are the same release: exit 0 and `IDENTICAL`. Any difference
exits 1. Errors exit 2. Profile variables are hashed separately; they are the
per-deploy config interface, not part of any pack.

## Where it sits in the lifecycle model

The lifecycle outline (draft 0.1) treats a profile version as a *release*: a
published definition artifact that is never changed after publication, with a
build/test/sign/publish/record pipeline and a consumer check before end of
life. In the pipeline prototype plan this tool is one of the scripts that
`just` and CI both call:

| where                          | what                                                                     |
|--------------------------------|--------------------------------------------------------------------------|
| profile `pr`                   | `--format markdown` between the last published version and the candidate, posted as the PR comment: the named evidence at the gate |
| profile `main` and `release`   | `snapshot` after each apply, kept as the record of the release           |
| cluster `bump`                 | the diff between the tier's current and proposed profile versions, with the repave impact the reviewer needs |
| scheduled                      | `verify` over every recorded snapshot: tools enforce immutability, not text |

## Output formats

**terminal** (default) is colored when stdout is a terminal: repave impacts
red, cluster-wide yellow, in-place cyan; added green, removed red, changed
yellow; old values red, new values green; unchanged packs dimmed. Piped output
is plain. `--color=always` forces it, `--color=never` or `--no-color` disables
it, and `NO_COLOR` and `CLICOLOR_FORCE` are honored.

```
lab-profile@1.2.0 (64f0000000000000000000a1)  ->  lab-profile@1.3.0 (64f0000000000000000000b2)
root       73c75fb7dba6 -> d5c5d079f15f   CHANGED
consumers  0 -> 2 clusters

  variables CHANGED
        ~ cluster_env.defaultValue: "dev" -> "staging"
        + db_password.name: "db_password"

  [os   ] ubuntu-aws                 22.04              unchanged
~ [k8s  ] kubernetes                 1.29.4 -> 1.30.2   IDENTITY UPSTREAM AUTHORED
      upstream digest  sha256:k8s12 -> sha256:k8s13   (packRef.digest)
      values: 3 changed, 1 added, 0 removed
        ~ kubeadmconfig.apiServer.extraArgs.audit-log-maxage: "30" -> "90"
        + kubeadmconfig.apiServer.extraArgs.profiling: "false"
      impact: node repave (Kubernetes version change)
~ [cni  ] cni-cilium-oss             1.15.3             UPSTREAM
      upstream digest  sha256:cni-a -> sha256:cni-b   (packRef.digest)
      impact: pack content changed under the same tag, applied on next profile sync
~ [addon] prometheus-operator        55.8.3             AUTHORED
      ~ manifest extra-rules
      + manifest dashboards
      impact: in-place addon upgrade
+ [addon] cert-manager               1.14.0             ADDED
- [addon] external-dns               0.14.0             REMOVED
```

**markdown** is a pull request comment: a header table, a result line with
the maximum impact, one table row per pack, and a collapsible `diff` block per
changed pack. `--full` appends unified diffs of raw values and changed
manifests.

**json** is the document described below. It is never colored and never
truncated.

## The JSON document, for scripts and AI clients

`--format json` prints `palette-profile-diff/1`. It is designed to be read
cold by a program or a language model without the docs:

- `schema` and `tool` identify the document and the implementation.
- `legend` explains the three flags, every `status`, every `op`, and every
  `impact` level in plain sentences.
- `summary` answers the common questions first: packs added, removed,
  changed, unchanged; how many identity, upstream, and authored moves;
  whether variables changed; and `maxImpact`, the most severe level across
  all packs.
- `packs[]` carries, per pack, `status`, the three booleans, `impact` as a
  `level` plus the sentence the terminal prints, both sides' version, tag,
  pack uid, registry uid, digest, values hash and node hash, `values.diff[]`
  as leaves, and `manifests[]` changes with content hashes.
- Every leaf has `op`, a dotted `path`, the same path as `segments` (strings
  for keys, integers for indexes, `{"name": ...}` for items of a list whose
  members all carry a unique `name`), and `old` and `new` values.
- `a` and `b` are the two snapshot sources, with uids, versions, consumer
  counts, and fetch times.

`palette-profile-diff schema diff` and `schema snapshot` print the JSON Schema
of each document. Impact levels, in ascending severity: `none`, `sync`,
`in-place`, `cluster-wide`, `may-repave`, `repave`. The `impact` text is a
hint derived from the layer and the flags, not a Palette API answer.

## Canonicalization

Palette rewrites values on read: whitespace, key order, expanded
`{{.spectro.var.X}}` templates. Text diffs churn on that; these hashes do not,
because values and manifests are parsed as YAML and re-serialized as JSON with
sorted keys before hashing. Values that do not parse as YAML, such as an
unquoted Go template, are hashed as trimmed text and the report says so;
`--full` prints a unified diff for those and for every changed manifest.

Lists whose items all carry a unique `name` are addressed by name in leaf
paths, so an inserted item shows as one addition rather than shifted
neighbors. Other lists are positional.

## Snapshots

`snapshot` writes one JSON file per profile (`palette-profile-snapshot/1`)
with the hashes, the canonical values, the raw values and manifests, and the
consumer list at the time. Keep it next to the release record, or feed it back
to `compare` or `verify` later. `--raw-dir DIR` also keeps the three raw API
responses, and `--from-raw DIR` rebuilds a snapshot from them offline.

Endpoints used per profile:

- `GET /v1/clusterprofiles/{uid}` for metadata, consumers, and the published pack refs
- `GET /v1/clusterprofiles/{uid}/packs/manifests` for registry digests and manifest contents
- `GET /v1/clusterprofiles/{uid}/variables` for profile variables
- `GET /v1/dashboard/clusterprofiles/metadata` to resolve `name@version`

A profile with no published template falls back to its draft; the snapshot
records which under `source.template`.

## Examples

`examples/fetch-examples.sh` pulls real packs from the Public Repo registry at
several versions, builds three profiles from them, and writes snapshots and
reports under `examples/`. See `examples/README.md` for what is real and what
is synthesized.

## Tests

`cargo test` runs unit tests and end-to-end tests over raw-API-shaped
fixtures with no network. One of them checks parity with the bash reference:
identical roots and per-pack hashes on the fixtures. `just test` runs both
suites.

## The bash reference implementation

`bash/palette-profile-diff` is the first implementation, kept as the
reference for the hash scheme and the report layout. It needs curl, jq, and
python3 with PyYAML. The two agree on the fixtures and on the example data,
with three known edges where the YAML parsers differ: PyYAML reads `yes`,
`no`, `on`, `off` as booleans and the Rust parser reads them as strings;
PyYAML accepts duplicate mapping keys and the Rust parser rejects them as
unparseable; and exponent-form floats print differently. Compare snapshots
with one implementation; `tool.impl` in every snapshot says which wrote it.

## Known gaps

- The `digest` Palette stamps on a profile's pack ref has not been checked
  against a live profile to confirm it tracks registry content. The tenant
  used on 2026-09-15 had no cluster profiles to look at. The snapshot records
  which field fed `UPSTREAM` under `hash.upstreamSource`. For packs built by
  the pipeline, the OCI manifest digest in the immutable ECR repositories is
  the better source and is the planned replacement.
- Only profiles are supported as refs. A cluster's applied profile templates
  use the same pack ref shape and are the natural next input, giving
  release-vs-run drift.
- Pack presets and schema are not hashed; a selected preset shows up as a
  values change anyway.
- Scalars longer than 120 characters are truncated in the text reports; the
  JSON document and `--full` carry the whole value.
