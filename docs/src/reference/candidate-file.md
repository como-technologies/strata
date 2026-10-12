# The candidate file

A candidate file describes a profile version before it is in Palette. It is YAML
or JSON. `strata schema candidate` prints its JSON Schema. Strata also reads the
export of a profile version that Palette writes: see
[A profile export of Palette](#a-profile-export-of-palette).

```yaml
name: aws-stack
version: 1.1.1
type: cluster              # default cluster
cloudType: aws
registry: Public Repo      # or registryUid; a layer can set either
layers:
  - name: ubuntu-aws
    version: "24.04"       # quote a version that YAML reads as a number
  - name: kubernetes
    version: 1.36.2
    valuesFile: values/kubernetes.yaml   # relative to this file
  - name: cert-manager
    version: 1.21.1
    manifests:
      - name: namespace-labels
        contentFile: manifests/namespace-labels.yaml
  - name: namespace-policy
    version: 1.0.0
    type: manifest         # manifests of your team, from no registry
    manifests:
      - name: namespace-policy
        contentFile: manifests/namespace-policy.yaml
variables:
  - name: platform_owner
    displayName: Platform owner
    required: true
```

## The fields at the top

| Field | Required | Function |
| --- | --- | --- |
| `name`, `version` | yes | The profile and its version. Neither can be empty. The version is one that Palette takes: see [Checks before any request](#checks-before-any-request). |
| `type` | no | `cluster`, `infra`, `add-on`, or `system`. The default is `cluster`. |
| `cloudType` | no | The cloud of the profile. The default is empty. |
| `registry` or `registryUid` | no | The registry for each layer that names none. It does not apply to a `manifest` layer. |
| `layers[]` | yes | One entry for each pack, in profile order. At least one. |
| `variables[]` | no | The profile variable definitions, in the form Palette returns them. |

A program can also give strata a candidate in memory, as JSON, through the
library. It then writes no file.

## The fields of a layer

| Field | Required | Function |
| --- | --- | --- |
| `name`, `version` | yes | The pack and its version. |
| `registry` or `registryUid` | yes, on the layer or at the top; no on a `manifest` layer | The registry, by name or uid. Not both. The setting on the layer has priority over the one at the top. A registry that Palette does not sync is permitted: see [A layer from a registry that Palette does not sync](#a-layer-from-a-registry-that-palette-does-not-sync). |
| `type` | no | `manifest` for a layer of manifests that comes from no registry. See [A manifest layer](#a-manifest-layer). Leave it out for a pack from a registry: the registry gives the type. |
| `values` or `valuesFile` | no | The values, inline or in a file. Not both. With neither, the layer gets the default values of the pack. |
| `layer` | no | The layer. The default comes from the registry. Palette takes the layer from the registry, so the layer of a pack from a registry must be its layer in that registry: see [The registry lookups](#the-registry-lookups). A `manifest` layer takes only `addon`: see [A manifest layer](#a-manifest-layer). |
| `tag` | no | The tag. The default is the tag that equals the version, else the first tag of that version, else the version itself. An explicit tag must be a tag of that version in the registry, or the version itself. A `manifest` layer takes any tag, and its default is the version. A floating tag, such as `1.0.x`, pins the build that the tag names now: see [Tag names](../internals/palette/packs.md#tag-names). |
| `params` | no | A map that Palette carries on a pack beside its values. Strata puts it in the authored hash, so a change to it is an `AUTHORED` change. |
| `manifests[]` | no; one or more on a `manifest` layer | Each with a `name`, and `content` inline or a `contentFile`. |

> **Set `tag` to the tag that you publish.** Palette stores the tag that a
> profile was authored with, and a registry can give one build several tags:
> see [Tag names](../internals/palette/packs.md#tag-names). The tag is part of
> `IDENTITY`, so another tag for the same build gives another root.
> `same-release` passes such a pair as `SAME CONTENT`.

## Checks before any request

Strata reads the whole file before it sends a request:

- A file that does not parse, an unknown field, or a file that breaks one of
  these rules is `candidate-invalid`. The message names the layer, by number
  and pack name.
  - `name` and `version` must not be empty.
  - `layers` needs at least one layer.
  - A layer sets `registry` or `registryUid`, or the top of the file does. Not
    both, and not neither.
  - A `manifest` layer sets no `registry` and no `registryUid`, and it has one
    manifest or more. Its `layer` is `addon`, or it has no `layer`.
  - `type` is `manifest`, or the layer has no `type`.
  - A layer sets `values` or `valuesFile`, or neither. Not both.
  - A manifest sets `content` or `contentFile`. Not both, and not neither.

  So strata reports a misspelled field such as `valuesfile`.
- A values file or a manifest file that strata cannot read is `io`.
- A candidate that breaks a rule that only the create of Palette checks is
  `candidate-invalid`. No route that takes a candidate answers these rules
  before the create, so strata fills the gap. The message names the rule, the
  code of Palette, and the layer or the manifest. Strata checks the rules in
  this order:
  1. The type takes the `layer` of each layer that gives one. `infra` takes
     `os`, `k8s`, `cni`, and `csi`. `add-on` and `system` take `addon`, `cni`,
     and `csi`. `cluster` takes each layer.
  2. When each layer gives its `layer`, a `cluster` or `infra` candidate has an
     `os`, a `k8s`, a `cni`, and a `csi` layer. On the cloud `edge-native`, it
     needs no `csi` layer.

[The gaps that strata fills](../internals/palette/profiles.md#the-gaps-that-strata-fills)
states what Palette does for each rule. A copy of the candidate of
[Write one from a published version](#write-one-from-a-published-version) has a
type that Palette refuses:

```text
{{#include ../output/candidate-refused.txt}}
```

Strata does not copy the rules that a validate route of Palette checks:

- The version.
- A pack name in two layers.
- The name of a manifest, or of a `manifest` layer.
- A manifest name two times in a layer.
- The YAML of the values and the manifests.
- The limit of 200 packs and 200 manifests.

A candidate that breaks one of them gives a record. To ask Palette before you publish, give
`--validate`: see
[Ask Palette before you publish](../commands/candidate.md#ask-palette-before-you-publish).

## The registry lookups

Strata reads `GET /v1/registries/metadata` one time for each resolve, and only
when it needs it. It needs it in three cases: a layer names its registry by
name, the listing does not have the version of a layer, or an error names a
registry. The listing gives each registry name its uid. Then, for each layer
that is not a `manifest` layer:

1. `GET /v1/packs?filters=spec.name=<name>`, in pages of 100: the pack uid, the
   digest, the layer, the type, and the default values of the version in that
   registry.
2. `GET /v1/packs/<name>/registries/<uid>`: the tags of that version, and the
   default values and the layer when the listing gives none. Empty text in the
   listing counts as none.

Palette does not sync some registries. If the listing does not have a version
from such a registry, the layer gets the pack that Palette makes at the create.
See [A layer from a registry that Palette does not sync](#a-layer-from-a-registry-that-palette-does-not-sync).

From these answers, strata checks the rules that need the registry, as
`candidate-invalid`:

- The `layer` of each layer that gives one is the layer of the pack in the
  registry. Palette gives each pack the layer of the registry, and drops the
  layer of the profile with no error. So another layer gives a root that the
  published version does not have. See
  [The layer of a pack](../internals/palette/packs.md#the-layer-of-a-pack).
- Each pack in the listing supports the `cloudType` of the candidate: see
  [A pack that supports a cloud](../internals/palette/packs.md#a-pack-that-supports-a-cloud).
  A candidate with no `cloudType` takes each pack. Only the create of Palette
  checks this rule.
- Rules 1 and 2 above, with the layer of each pack in the registry.

The pack that Palette makes for a registry that it does not sync has the layer
`addon` and no cloud type. So rule 1 refuses it in an `infra` candidate, and
each cloud takes it.

A copy of the candidate of
[Write one from a published version](#write-one-from-a-published-version) gives
`cert-manager` the layer `cni`. The type `cluster` takes `cni`, so the checks
before any request pass. The registry gives `addon`:

```text
{{#include ../output/candidate-layer-refused.txt}}
```

A version or a tag that the registry does not have is `pack-not-found`. A
registry name that the key cannot see is `registry-not-found`. A `registryUid`
that the key cannot see goes out as given, so its packs are `pack-not-found`,
and the error names the uid. A version that Spectro Cloud retired, with the
`system_state` `disabled` or `deleted`, is `pack-unavailable`. Palette does not
serve it, and refuses a profile that pins it. A version with the mark
`disabled` is not: Palette serves it, and refuses the profile on its validate
route, so give `--validate`. See
[Pack states](../internals/palette/packs.md#pack-states).

The snapshot has `source.template` `candidate`, no uid, and no `consumers`.
Strata takes the digest of each pack from the registry, as Palette does for a
published profile: see
[The pack-ref digest](../internals/palette/packs.md#the-pack-ref-digest). So a
candidate with the published tags, values, and manifests has the published
root. The live example checks that: see
[What the live example verifies](../development/live-runs.md).

## Template options

The template of a pack can have input parameters with options. Each parameter
reads one key of the values, its `targetKey`. When the value at that key is the
name of an option, the values select that option. On a create or an update of
the profile, Palette can then write the presets of that option into the values.
No route of Palette gives the presets, so strata does not apply them: see
[The presets of a template option](../internals/palette/packs.md#the-presets-of-a-template-option).

For each layer, strata reads the template of the version from the listing entry
that it already reads. When the values select an option, strata:

- Writes one warning on standard error for each selection. `-q` hides it, as it
  hides each warning.
- Records the selection in `packs[].presetOptions[]`: `parameter`, `targetKey`,
  and `option`, sorted by parameter. The field is not part of the root.

Strata does not change the values. So the root of the candidate can differ from
the root of the version that Palette publishes from the same file. For
example, the `headlamp` 0.45.0 template has the parameter `access`, at
`pack.palette.config.headlamp.access`. The value `public` selects the option
`public`, with the label `Direct`. The presets of that option set
`charts.headlamp.service.type` to `LoadBalancer`.

In this example, the stand-in gives `cert-manager` 1.21.1 a template with one
parameter, `access` at `pack.access`, and one option, `public`:

```text
{{#include ../output/candidate-preset-yaml.txt}}
{{#include ../output/candidate-preset-warning.txt}}
{{#include ../output/candidate-preset-options.txt}}
```

A value that names no option, or values with no such key, give no warning and
no field. After a warning, record the version when Palette publishes it. Then
compare that record with the candidate.

## A manifest layer

A manifest pack holds Kubernetes YAML that your team wrote. It comes from no
registry. Give its layer `type: manifest`, its version, and its manifests:

```yaml
  - name: namespace-policy
    version: 1.0.0
    type: manifest
    manifests:
      - name: namespace-policy
        contentFile: manifests/namespace-policy.yaml
```

Strata sends no request for a `manifest` layer. The pack that it builds has the
type `manifest`, no registry uid, no digest, and no pack uid. Palette gives the
manifest pack of each profile a new pack uid when it creates the profile. The
values and the manifests come from the file.

So the root of the candidate differs from the root of the published version by
that pack uid alone. `same-release` passes the pair as `SAME CONTENT`, and
`mintedUidOnly` in its JSON names the pack. See
[One manifest in two profiles](../use-cases/manifest-pack.md#5-review-a-change-before-you-publish-it)
and [Manifest pack uids](../internals/palette/packs.md#manifest-pack-uids).

Palette builds the pack of a manifest layer from its own manifest pack:

- **The tag** comes from the profile. Give the tag that you publish.
- **The version** comes from the manifest pack of Palette, and not from the
  profile. Give that version. `strata candidate` copies it from the record of a
  published version.
- **The layer** comes from the manifest pack of Palette too, and is `addon`.
  Palette drops a layer that the profile gives. So strata refuses another
  layer on a `manifest` layer: the published version does not have it.

## A layer from a registry that Palette does not sync

Palette does not sync a Helm registry that is private, or an OCI registry of the
provider type `helm` or `zarf` with sync off. Its pack listing has no chart of
such a registry, until a profile pins one. Give the layer its name, its
version, and its registry, as for another pack:

```text
{{#include ../output/nodigest-chart-file.txt}}
```

When the listing does not have the version, strata asks Palette whether the
registry syncs:

1. `GET /v1/registries/metadata`: the `kind` and `isPrivate` of the registry. A
   Helm registry that is private does not sync.
2. `GET /v1/registries/oci/summary`, only for an OCI registry: the provider type
   and `status.sync.isSyncSupported`. A registry of the provider type `pack`
   syncs.

A registry that syncs, and does not have the version, is `pack-not-found`. For
a registry that does not sync, strata builds the pack that Palette makes when it
creates the profile:

- **No pack uid.** Palette gives the uid at the create.
- **No digest.** `upstreamSource` is `none`.
- **The type** of the registry: `helm`, or `oci` with the provider type of the
  registry.
- **The layer** `addon`. Palette drops a layer that the profile gives, so
  strata refuses another layer.
- **The tag as the version.** Strata refuses a `tag` that is not equal to the
  version. Only the create of Palette checks the tag itself, so strata fills
  that gap: see
  [The gaps that strata fills](../internals/palette/profiles.md#the-gaps-that-strata-fills).
  All three refusals are `candidate-invalid`.
- **The values of the file.** Palette has no default values for such a pack.

```text
{{#include ../output/nodigest-chart-record.txt}}
{{#include ../output/nodigest-chart-pack.txt}}
```

The record of the published version has the pack uid that Palette made, so the
roots differ by that uid alone. `same-release` passes the pair as
`SAME CONTENT` with `--allow no-digest`, because the pack has no digest, and
`mintedUidOnly` names the pack:

```text
{{#include ../output/nodigest-chart-allow.txt}}
```

After the create, the listing has the chart with that uid. A record of the same
file then gets the uid, and has the published root. See
[A pack that Palette makes at the create](../internals/palette/packs.md#a-pack-that-palette-makes-at-the-create).

## A profile export of Palette

Palette writes a profile version as one file: the export. The console exports
it
([Export and import](https://docs.spectrocloud.com/profiles/cluster-profiles/export-import-cluster-profile/)),
and `GET /v1/clusterprofiles/{uid}/export` gives it. Give the export to
`snapshot --from-candidate` as it is, in YAML or in JSON. Strata reads a file
that has `spec.template.packs` as an export, and each other file as a candidate
file. So a team that keeps the export in its repository writes no second file
for strata.

Each field of the export gives one field of the candidate:

| Field of the export | Field of the candidate |
| --- | --- |
| `metadata.name` | `name` |
| `spec.version` | `version` |
| `spec.template.type` | `type`. An empty type gives `cluster`. |
| `spec.template.cloudType` | `cloudType` |
| `spec.template.packs[]` | `layers[]`, in the same order |
| `name`, `version`, `tag`, and `layer` of a pack | the same fields of the layer |
| `type` of a pack | `type: manifest` for a pack of the type `manifest`. The registry gives each other type. |
| `registry.metadata.uid` of a pack | `registryUid`. When the uid is empty, `registry.metadata.name` gives `registry`. A `manifest` pack names no registry. |
| `values` of a pack | `values`. The export omits values that are empty, so a pack with no `values` gets empty values, and not the default values of the pack. |
| `manifests[]` of a pack, with `name` and `content` | `manifests[]`, with `name` and `content` |
| `spec.variables[]` | `variables[]` |

Strata does not read `metadata.description`, `metadata.labels`, or the other
fields of `registry.metadata`. The export holds no `params`, so each layer has
no params. The root does not hold the `order` of a variable. When a field of
the export holds a number and not text, the export is `candidate-invalid`, and
the message names the field. When `spec.variables` or the `manifests` of a
pack is not a list, the export is also `candidate-invalid`, and the message
names the field. Then strata checks the candidate as it checks a
candidate file: see [Checks before any request](#checks-before-any-request).

In this example, the stand-in gives the export of `aws-stack@1.1.1` in JSON:

```text
{{#include ../output/candidate-export.txt}}
{{#include ../output/candidate-export-record.txt}}
{{#include ../output/candidate-export-same.txt}}
```

The record of the export of a version has the root of that version, while the
registry serves the same pack versions. A version with a manifest pack differs
only by the pack uid that Palette mints, as for a
[manifest layer](#a-manifest-layer). [The export of a version](../internals/palette/profiles.md#the-export-of-a-version)
gives the live run.

## Write one from a published version

[`strata candidate`](../commands/candidate.md) writes the file of a published
version, with each tag and registry uid pinned, and each value and manifest in
full.
