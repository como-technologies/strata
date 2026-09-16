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
