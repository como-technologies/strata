# strata

A Spectro Cloud Palette cluster profile is a stack of layers: OS, Kubernetes,
CNI, CSI, and add-ons, each a pack with a version, a digest, and its values.
Strata treats every published version of that stack as an immutable release
and makes it a thing a team can handle like any other build artifact:

- **record** a published version as a snapshot: a Merkle root over the
  layers, three hashes per layer for what it points at, what it contains, and
  how it is configured, plus the canonical contents and the clusters using it
- **compare** two versions and say *what kind* of change happened in each
  layer and what it does to running clusters, not just that something changed
- **verify** that a published version still matches its record, so
  immutability is enforced by a tool and not by a sentence in a document

Think of it as build, release, run: a pack is a build, a profile version is
a release, and a cluster is a run. Strata is the record, the review, and the
audit of the release. One static Rust binary; the output is a colored
terminal report, a markdown pull request comment, or a versioned,
self-describing JSON document built for machine and AI consumption.

**Start here: [docs/walkthrough.md](docs/walkthrough.md)** shows a platform
team using the tool through one release cycle: record a release, review a
bump, promote through tiers, prove immutability every night, and check
consumers before end of life. Every command in it runs offline against
`examples/`.

```
strata my-profile@1.2.0 my-profile@1.3.0
strata 64f0…a1 64f0…b2 --full
strata compare A B --format markdown        # pull request comment
strata compare A B --format json            # for scripts and AI clients
strata snapshot my-profile@1.2.0 -o rel.json # the record step
strata verify rel.json                       # immutability check
strata schema diff                           # JSON Schema of the diff document
```

Build with `cargo build --release` (or `just build`); the binary is
`target/release/strata`. Set `PALETTE_API_KEY`, and
`PALETTE_PROJECT_UID` for project-scoped profiles. `PALETTE_API` overrides the
base URL for self-hosted Palette. Values missing from the environment are read
from `~/.palette.env`.

## The model

Each pack on a profile is a Merkle node with three sub-hashes, so a report can
name which one moved:

| flag       | hashed from                                                  | Palette concept                              | what it means for the release              |
|------------|--------------------------------------------------------------|----------------------------------------------|--------------------------------------------|
| `IDENTITY` | pack name, layer, type, tag, version, packUid, registryUid    | which pack version the layer references      | the release points at a different build    |
| `UPSTREAM` | the `digest` on the profile's pack ref                        | pack content in the registry                 | the build changed under the same version, which a published build must never do |
| `AUTHORED` | canonical values YAML, attached manifest contents, params     | the pack's values and manifests on the profile | release-level config                       |

`node = sha256(identity + upstream + authored)`, and
`root = sha256(variablesHash, [key, node] in profile order)`. Two profiles with
the same root are the same release: exit 0 and `IDENTICAL`. Any difference
exits 1. Errors exit 2. Profile variables are hashed separately; they are the
per-deploy config interface, not part of any pack.

## The way of working it supports

The team is adopting one way to describe everything Palette manages. There
are two kinds of thing. A *definition* has a name and a version; it is
published and never changed afterward. An *instance* is deployed, changes
over time, and is reconciled to a definition. Packs and profiles are
definitions; clusters are instances. Configuration that changes per
deployment stays out of the build. A published artifact is immutable, and a
tool enforces that, not a sentence in a document. Every level has an owner, a
pipeline of build, test, sign, publish, and record, approval gates with named
approvers and named evidence, and an end of life that checks for consumers
before anything is deleted.

The delivery pipeline that goes with it: one repository per pack, one per
profile, one per cluster. Four tiers, dev, test, stage, and prod, each a
Palette project and a deployment environment with its own reviewers. A merge
to a profile repository publishes a new profile version to dev and never
edits an existing one; a tag promotes that same version through the other
tiers behind approval gates. When a pack is published, every profile that
uses it gets a pull request that bumps the pin; when a profile version is
published, every cluster that uses it gets one that bumps the tier.
Everything is a script that a developer runs locally and CI runs the same
way.

Strata is one of those scripts. It supplies what that model asks for and
Palette does not provide on its own:

| moment                                   | what strata does                                                          |
|------------------------------------------|---------------------------------------------------------------------------|
| a profile bump is under review           | `compare --format markdown` between the last published version and the new one, posted on the pull request as the evidence the approver reads |
| a profile version has just been published | `snapshot`, kept in the profile repository as the record of the release  |
| a cluster tier is about to move          | `compare` between the tier's current and proposed versions, with the repave impact the reviewer needs |
| on a schedule                            | `verify` over every recorded snapshot: the immutability rule, enforced     |
| before end of life                       | `snapshot` and read `source.consumers`; delete only at zero               |

The procedures, with commands and what each output means, are in
[docs/walkthrough.md](docs/walkthrough.md).

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

`--format json` prints `strata-diff/1`. It is designed to be read
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

`strata schema diff` and `schema snapshot` print the JSON Schema
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

`snapshot` writes one JSON file per profile (`strata-snapshot/1`)
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

`cargo test` (or `just test`) runs unit tests and end-to-end tests over
raw-API-shaped fixtures with no network: snapshot shape, canonicalization,
every kind of change in the report, the JSON document, markdown, the color
policy, and exit codes.

## Known gaps

- The `digest` Palette stamps on a profile's pack ref has not been checked
  against a live profile to confirm it tracks registry content; no tenant
  with a cluster profile has been available. The snapshot records which field
  fed `UPSTREAM` under `hash.upstreamSource`. For packs built by
  the pipeline, the OCI manifest digest in the immutable ECR repositories is
  the better source and is the planned replacement.
- Only profiles are supported as refs. A cluster's applied profile templates
  use the same pack ref shape and are the natural next input, giving
  release-vs-run drift.
- Pack presets and schema are not hashed; a selected preset shows up as a
  values change anyway.
- Scalars longer than 120 characters are truncated in the text reports; the
  JSON document and `--full` carry the whole value.
