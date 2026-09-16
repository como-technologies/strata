# The model: hashes, canonical form, and the snapshot

## Three hashes per layer

A profile is a list of packs in layer order. Strata makes each pack a Merkle
node with three hashes. A report can then say which hash moved.

| flag       | hashed from                                                | Palette concept                                | meaning for the release                    |
|------------|------------------------------------------------------------|------------------------------------------------|--------------------------------------------|
| `IDENTITY` | pack name, layer, type, tag, version, packUid, registryUid  | which pack version the layer references        | the release points at a different build    |
| `UPSTREAM` | the `digest` on the pack ref of the profile                 | pack content in the registry                   | the build changed under the same version. A published build must never do this. |
| `AUTHORED` | canonical values YAML, attached manifest contents, params   | the values and manifests of the pack on the profile | release-level configuration           |

`node = sha256(identity + upstream + authored)`

`root = sha256(variablesHash, [key, node] in profile order)`

Two profiles with the same root are the same release. Profile variables have
their own hash. They are the per-deployment configuration interface. They are
not part of a pack.

### What the pack-ref digest tracks

The live example (`examples/live/`, run `20260916T153806Z`) settled it for the packs
it used. On a published profile, the `digest` Palette stamps on each pack ref
equals the digest the registry reports for that pack version. That held for
all four packs, `spectro` type from the Public Repo:

| pack | version | pack-ref digest and registry digest |
|---|---|---|
| ubuntu-aws | 24.04 | `sha256:2df05daa4eb4...` |
| kubernetes | 1.36.2 | `sha256:c4f7f20f40c7...` |
| cni-cilium-oss | 1.20.1 | `sha256:917d9de9a696...` |
| csi-aws-ebs | 1.65.0 | `sha256:bd09e8b4187b...` |

The same digests appear for those versions in `examples/INDEX.md`, fetched
from the registry a day earlier. So `UPSTREAM` moves with the pack version's
content, and the fallback order, pack-ref digest first and registry digest
second, needs no change. Helm and OCI packs were not part of the run.

### Editing a published version in place

The same run edited version 1.0.1 of its profile in place through the API,
as the Terraform provider does, changing one value. Palette accepted the edit.
The profile kept its uid and its version, stayed published, and no new version
appeared. `strata verify` on the record taken before the edit exited `1` with
`DRIFT` and named the changed leaf. A published version is immutable only
because a check says so; the scheduled `verify` is that check.

## Canonical form

Palette rewrites values when it returns them. Whitespace, key order, and
template expansion change. A text diff moves on each of these. The hashes do
not move. Strata parses values and manifests as YAML and writes them as JSON
with sorted keys before it hashes them.

Values that are not valid YAML get a different treatment. An unquoted Go
template is one example. Strata hashes the trimmed text, and the report says
so. `--full` prints a unified diff for these values and for each changed
manifest.

## Leaf paths

A leaf is a scalar or an empty container. Strata writes a path for each leaf.
Keys are joined with a dot. A list item is `[i]`. When each item of a list is
an object with a unique `name`, the item is `[name=<value>]`. An inserted item
then shows as one addition, not as moved neighbors.

## Impact levels

The report gives an impact for each changed pack. The impact comes from the
layer and the flags. It is a hint. It is not an answer from the Palette API.

| level          | condition                                         | meaning                                                   |
|----------------|---------------------------------------------------|-----------------------------------------------------------|
| `repave`       | OS or Kubernetes layer with `IDENTITY`            | Palette replaces the nodes                                |
| `may-repave`   | OS or Kubernetes layer with `AUTHORED`            | node or control plane configuration changed; Palette can repave |
| `cluster-wide` | CNI or CSI layer with `IDENTITY` or `AUTHORED`    | each node is affected; a repave is possible               |
| `in-place`     | add-on layer with `IDENTITY` or `AUTHORED`        | the add-on is upgraded or reconfigured in place           |
| `sync`         | only `UPSTREAM`                                   | applied on the next profile sync                          |
| `none`         | no change                                         |                                                           |

The levels are in ascending order of severity. The JSON summary reports the
most severe level as `maxImpact`.

## The snapshot document

`strata snapshot` writes one JSON file per profile version. The schema id is
`strata-snapshot/1`. The file holds:

- `source`: the ref, the uid, the project the snapshot was taken in (empty
  for tenant scope), the name, the version, the cloud type, the profile type,
  the template that was used (`published`, `draft`, or `applied` for a
  cluster), the tags, the consumer clusters, and the fetch time. A snapshot
  of a cluster adds `cluster`: its uid and name, the profile selected with
  `cluster:<uid>/<profile>` if any, and the applied profiles it covers
- `variables` and `variablesHash`
- `packs[]`: for each pack, the identity fields, the two digests, the raw
  values, the canonical values, the values hash, the manifests with content
  and content hash, and the `hash` block with `identity`, `upstream`,
  `upstreamRaw`, `upstreamSource`, `authored`, and `node`
- `root`

`hash.upstreamSource` says which field fed `UPSTREAM`: `packRef.digest`,
`packSummary.digest`, or `none`.

Keep the file with the release record. `compare` and `verify` read it later.
`--raw-dir DIR` also keeps the three raw API responses. `--from-raw DIR`
builds a snapshot from them without the network.

Strata reads these endpoints for one profile:

- `GET /v1/clusterprofiles/{uid}`: metadata, consumers, and the published
  pack refs
- `GET /v1/clusterprofiles/{uid}/packs/manifests`: registry digests and
  manifest contents
- `GET /v1/clusterprofiles/{uid}/variables`: profile variables
- `GET /v1/dashboard/clusterprofiles/metadata`: resolves `<name>@<version>`,
  and `<name>@latest` to the highest version by natural order: runs of digits
  compare as numbers, the rest as text

A profile without a published template falls back to its draft. The snapshot
records the choice in `source.template`.

## Candidates: the release before it exists

At review time the new profile version is not in Palette yet. A candidate
file describes it, and `strata snapshot --from-candidate <file>` builds its
snapshot. Strata looks each pack up in its registry for what Palette fills in
on publication, and builds the documents a published profile with that
content returns. So the candidate hashes as the published version will, and
`strata same-release candidate.json record.json` after publication proves
that the approved release is the published one.

The file is YAML or JSON. `strata schema candidate` prints its JSON Schema.

```yaml
name: aws-pxk
version: 1.1.1
type: cluster              # default cluster
cloudType: aws
registry: Public Repo      # or registryUid; a layer can override either
layers:
  - name: ubuntu-aws
    version: "24.04"       # quote versions YAML would read as numbers
  - name: kubernetes
    version: 1.36.2
    valuesFile: values/kubernetes.yaml   # relative to this file
  - name: cert-manager
    version: 1.21.1
    manifests:
      - name: namespace-labels
        contentFile: manifests/namespace-labels.yaml
variables:
  - name: platform_owner
    displayName: Platform owner
    required: true
```

For each layer:

- `name`, `version`, and a registry are required. The registry is `registry`
  by name or `registryUid`, on the layer or at the top.
- `values` inline or `valuesFile`, not both. With neither, the layer gets the
  pack's default values from the registry.
- `layer` defaults to the registry's. `tag` defaults to the tag equal to the
  version, else the first tag of that version. An explicit `tag` must be one
  of the registry's tags for that version, or the version itself. Palette
  stores the tag a profile was authored with: a profile built with Terraform
  pins `24.04` where the registry names that version `LTS__24.x`. Set `tag`
  to match how the profile will be published, because it is part of
  `IDENTITY`.
- `params` is a map. `manifests[]` each have a `name` and `content` inline or
  `contentFile`.

Unknown fields are an error, so a misspelled `valuesfile` is not silently
ignored. Problems in the file, and values or manifest files that cannot be
read, are found before any request. The registry lookups are:

- `GET /v1/registries/metadata`: a registry name to its uid
- `GET /v1/packs?filters=spec.name=<name>`: the pack uid, digest, layer, type,
  and default values of the version in that registry
- `GET /v1/packs/<name>/registries/<uid>`: the tags of that version

A version or tag the registry does not have is `pack-not-found`; a registry
name that is not visible is `registry-not-found`. The snapshot has
`source.template` `candidate` and no uid. Because a published profile's
pack-ref digest equals the registry digest, a candidate that carries the
published tags, values, and manifests has the published root. The live
example checks exactly that.

## Clusters: release versus run

A cluster carries a copy of each profile version applied to it. Strata hashes
each copy exactly as it hashes the profile: the applied packs stand in for the
published packs, the cluster's manifests entry for that profile stands in for
the profile's manifests, and the profile's variable definitions come from the
cluster. So a cluster that runs a recorded version has the root of that
record, and `compare record.json cluster:<uid>/<profile>` answers whether the
cluster runs the release. An edit to the cluster's pack values shows as
`AUTHORED`, and a cluster still on an older version shows `IDENTITY`.

The values a cluster gave its variables are run-time input, not part of the
release. They are neither hashed nor recorded, because some are secrets.

Without a profile name, the snapshot covers every applied profile: packs in
the order Palette applies them, variables merged and sorted by name. Its
`source.name` and `source.version` join the profiles' with `+`, and
`source.uid` is empty. `verify` on a cluster record fetches the cluster again,
with the same selection.

Strata reads these endpoints for one cluster:

- `GET /v1/spectroclusters/{uid}`: `spec.clusterProfileTemplates`, one
  applied profile each, with its packs
- `GET /v1/spectroclusters/{uid}/profiles/packs/manifests`: registry digests
  and manifest contents, one entry per applied profile
- `GET /v1/spectroclusters/{uid}/variables`: variable definitions, one entry
  per applied profile

`strata schema snapshot` prints the JSON Schema of the document.
