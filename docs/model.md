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

The live example (`examples/live/`) settled it for each registry kind a
profile pins packs from. On a published profile, the `digest` Palette stamps
on each pack ref equals the digest Palette lists for that pack version. For a
Helm pack and an OCI pack, it also equals the digest the registry itself
serves, read without Palette by `tenant.sh digest`. Run `20260916T175442Z`:

| pack | type, registry | version | pack-ref digest | what the digest is |
|---|---|---|---|---|
| ubuntu-aws | `spectro`, pack registry | 24.04 | `sha256:2df05daa4eb4...` | the pack registry's digest for the version, as Palette lists it |
| kubernetes | `spectro`, pack registry | 1.36.2 | `sha256:c4f7f20f40c7...` | the same |
| cni-cilium-oss | `spectro`, pack registry | 1.20.1 | `sha256:917d9de9a696...` | the same |
| csi-aws-ebs | `spectro`, pack registry | 1.65.0 | `sha256:bd09e8b4187b...` | the same |
| nginx | `helm`, Helm registry | 25.1.5 | `08816cde6bea...` | the sha256 of the chart archive, as the repository index records it |
| argocd | `oci`, OCI pack registry | 10.4.0.1 | `sha256:99cac22f712b...` | the manifest digest of `<base path>/spectro-packs/archive/argocd:10.4.0.1` |

The `spectro` digests also appear for those versions in `examples/INDEX.md`,
fetched from the registry a day earlier, and in the first run,
`20260916T153806Z`. So for all three kinds the pack-ref digest is a digest of
the pack version's content, the fallback order, pack-ref digest first and
registry digest second, needs no change, and strata needs no registry reads
of its own.

Details the runs showed:

- A Helm digest is bare hex, without the `sha256:` prefix the other kinds
  carry. Strata hashes the digest as Palette stores it, so this matters only
  when the digest is compared with another tool's output.
- The `nginx` 25.1.5 entry in the Bitnami index points at a chart in an OCI
  registry. The digest is still the sha256 of the chart archive: the chart
  layer's digest, not the manifest digest (`sha256:7504fcfcdf30...`).
- Palette stamps the digest from its own copy of the registry, and the copy
  can lag. On 2026-09-16 Palette listed `nginx` up to 25.1.5 from Bitnami
  while the repository index had 25.1.12.

Not covered: a version pushed again under the same tag, for any kind, so
whether a published profile's pack-ref digest follows such a push is
unobserved; Helm registries that need credentials; and OCI registries that
hold Helm charts rather than packs. The tenant had none of the last two.

### Pack schema and presets

Strata hashes neither the pack schema nor the preset definitions, and it does
not need to. Palette does not store them on a profile: in the live run, the
pack refs and the manifests entries of a published profile carried no schema
and no presets, although the registry lists 20 presets for `cni-cilium-oss`
and 6 for `kubernetes`. They belong to the pack version in the registry, so a
change to them under the same version is a change to the pack's content, and
the pack-ref digest that feeds `UPSTREAM` tracks that content. A preset that a
profile applies ends up in the pack's values, which `AUTHORED` covers. The
hash scheme, and with it `strata-snapshot/1`, stays as it is.

### A real registry move

`examples/live/registry-move.sh` moved a pack the way a team promotes one:
profile version 1.0.0 took every pack from the pack registry (`Public Repo`),
and a version bump to 1.0.1 with the provider's `immutable-clusterprofiles`
flag on re-pinned `csi-aws-ebs` 1.65.0 to an OCI pack registry
(`Palette Registry`). The flag kept 1.0.0 under its own uid. Run
`20260916T233556Z`, `strata compare` of the two records:

| field | 1.0.0, pack registry | 1.0.1, OCI registry |
|---|---|---|
| type | `spectro` | `oci` |
| registryUid | `5eecc89d0b150045ae661cef` | `64eaff453040297344bcad5d` |
| packUid | `6aa410ec144aaec79d547a9e` | `6aa41627144aaece1b2e2452` |
| digest (`UPSTREAM`) | `sha256:bd09e8b4187b...` | `sha256:a746e1468908...` |
| tag, version | `1.65.0` | `1.65.0` |
| values (`AUTHORED`), manifests, params | | unchanged |

The other three packs did not change. The pack type follows the registry, as
the registries profile shows too (`helm` from a Helm registry, `oci` from an
OCI registry), so a real move to another kind of registry changes it. The
registry-move policy had required an equal type, from a fixture that
rewrote only uids and digests, and failed this pair with `IDENTITY: type
spectro -> oci`. It now accepts a type change between two registries, and
still refuses a pack that comes from no registry, a `manifest`. With that
change the pair passes as `REGISTRY MOVE` with one pack re-pinned. After an
edit of 1.0.1's CSI values in place through the API, the same check fails and
names the `AUTHORED` leaf
(`charts.aws-ebs-csi-driver.storageClasses[name=spectro-storage-class].reclaimPolicy`).

### Editing a published version in place

The same run edited version 1.0.1 of its profile in place through the API,
as the Terraform provider does, changing one value. Palette accepted the edit.
The profile kept its uid and its version, stayed published, and no new version
appeared. `strata verify` on the record taken before the edit exited `1` with
`DRIFT` and named the changed leaf. A published version is immutable only
because a check says so; the scheduled `verify` is that check.

#### A version that clusters run

`examples/live/in-place.sh` edits and then bumps the add-on profile version
that two clusters run, with the Terraform provider's
`immutable-clusterprofiles` flag off (run `20260916T213303Z`) and on (run
`20260916T233714Z`). Cluster A ran `strata-live-addon@1.0.0` before the edit.
Cluster B was created from the edited 1.0.0.

Each cluster runs its own copy of a profile's manifests. The copy has its own
uid, and `GET /v1/spectroclusters/<uid>/profiles/packs/manifests` serves it.
`cluster:<uid>` records that copy, so a cluster record shows what the cluster
was given, not what its profile holds now:

| step | flag off | flag on |
|---|---|---|
| edit 1.0.0 in place | Terraform sends the edit as a `PUT` | Terraform refuses the plan; Palette accepts the same edit through the API |
| cluster A, running before the edit | keeps its old copy: its record from before the edit verifies, and the version's record from before the edit compares `IDENTICAL` against it | the same |
| the version's record from before the edit | `verify` reports `DRIFT` | the same |
| cluster B, created after the edit | runs the edited content: `compare cluster:A/<profile> cluster:B/<profile>` shows the change and notes that both sides are the same version, edited in place | the same |
| the update pushed to cluster A without resolving its notification | cluster A's copy takes the edit within three minutes | the same |
| the update accepted on cluster A | cluster A matches cluster B, and A's record from before the edit reports `DRIFT` | not yet run |
| version bump to 1.0.1 | the profile keeps its uid, which now names 1.0.1: `verify` on the 1.0.0 record reports it overwritten by `strata-live-addon@1.0.1`. Both clusters keep their copies, which still name 1.0.0, and stay listed as consumers of the uid | not yet run |

So `compare` of two cluster records shows a change exactly when the clusters
run different content, and strata needs no record of the updates that wait for
a cluster.

An edit through the API must keep each manifest's uid. In the flag-on run the
edit first sent the manifest without it. Palette replaced the manifest with a
new one, and when cluster A's update was then accepted, A's copy of the old
manifest was dropped, leaving the add-on with no manifest on the cluster.
`api.sh edit` keeps the uids, and `in-place.sh` checks that it did.

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
  `cluster:<cluster>/<profile>` if any, and the applied profiles it covers.
  The ref stays as typed, a name or a uid. `verify` reads only the uids, the
  cluster's and the project's, so a renamed cluster or project still verifies.
  `cluster.variables` holds the value the cluster gives each variable of those
  profiles, with the profile's name and uid. It is outside the root, so two
  clusters on one release have one root whatever their values. A sensitive
  variable is listed with `sensitive: true` and no value, and no hash of the
  value either: strata keeps secrets out of every document
- `variables` and `variablesHash`
- `packs[]`: for each pack, the identity fields, the two digests, the raw
  values, the canonical values, the values hash, the manifests with content
  and content hash, and the `hash` block with `identity`, `upstream`,
  `upstreamRaw`, `upstreamSource`, `authored`, and `node`
- `root`

`hash.upstreamSource` says which field fed `UPSTREAM`: `packRef.digest`,
`packSummary.digest`, or `none`.

Keep the file with the release record. `compare` and `verify` read it later.
`--raw-dir DIR` also keeps the three raw API responses, and `scope.json` with
the project they came from. `--from-raw DIR` builds a snapshot from them
without the network, in that project, so `verify` checks the snapshot there. A
directory saved without `scope.json` builds a snapshot at tenant scope.

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

To start from a version that is already published, `strata candidate <ref>
-o profile.yaml` writes its candidate file. The ref is a profile uid,
`<name>@<version>`, or a profile record; a cluster is refused. The file pins
each layer's `tag` and `registryUid` and spells out its `values`, `params`,
and `manifests`, and carries the variables, so `snapshot --from-candidate`
turns it back into the version's root while the registry still serves those
pack versions. `--files-dir DIR` writes each layer's values to
`DIR/values/<pack>.yaml` and each manifest to
`DIR/manifests/<pack>/<manifest>.yaml`, referenced from the file, which is
the form a repository keeps. When the published version is later edited in
place, `same-release` between the candidate's snapshot and the version fails
and names the edited leaf.

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

## Versions and compatibility

Strata has three interfaces, and each is versioned on its own.

**The Rust library.** The crate follows semantic versioning over the public
API that `cargo doc` shows under `strata::`. While the version is 0.x, a minor
release, such as 0.4 to 0.5, can change that API, and a patch release, such
as 0.4.0 to 0.4.1, does not. Crate-internal code, such as the canonical form,
is not part of the API. Each release is tagged `v<version>`.

**The documents.** Each JSON document carries a schema id:
`strata-snapshot/1`, `strata-diff/1`, `strata-verify-summary/1`,
`strata-chain/1`, `strata-consumers/1`, and `strata-list/1`. The id changes
only when a reader of the old id would misread the document: a field removed,
renamed, or given a new meaning, or a hash computed differently. Adding a
field keeps the id. Strata ignores fields it does not know, so it reads a
record of its own id that a later release wrote. `source.projectUid` and
`source.cluster` joined `strata-snapshot/1` this way. Roots are comparable
only within one snapshot schema id. Strata refuses a snapshot of another id
with `unsupported-schema`, which names both ids.

**The command line.** Scripts can rely on the exit codes `0`, `1`, and `2`, on
the JSON documents, and on the error `kind` names. New kinds can appear, and
an existing kind keeps its meaning. The terminal and markdown text can change
in any release. The candidate file format rejects fields it does not know, so a
file that uses a field added in a later release fails on an older strata with
`candidate-invalid`.
