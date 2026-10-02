# The candidate file

A candidate file describes a profile version before it is in Palette. It is YAML
or JSON. `strata schema candidate` prints its JSON Schema.

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
variables:
  - name: platform_owner
    displayName: Platform owner
    required: true
```

## The fields at the top

| Field | Required | Function |
| --- | --- | --- |
| `name`, `version` | yes | The profile and its version. Neither can be empty. |
| `type` | no | `cluster`, `infra`, `add-on`, or `system`. The default is `cluster`. |
| `cloudType` | no | The cloud of the profile. The default is empty. |
| `registry` or `registryUid` | no | The registry for each layer that names none. |
| `layers[]` | yes | One entry for each pack, in profile order. At least one. |
| `variables[]` | no | The profile variable definitions, in the form Palette returns them. |

A program can also give strata a candidate in memory, as JSON, through the
library. It then writes no file.

## The fields of a layer

| Field | Required | Function |
| --- | --- | --- |
| `name`, `version` | yes | The pack and its version. |
| `registry` or `registryUid` | yes, on the layer or at the top | The registry, by name or uid. Not both. The setting on the layer has priority over the one at the top. |
| `values` or `valuesFile` | no | The values, inline or in a file. Not both. With neither, the layer gets the default values of the pack. |
| `layer` | no | The layer. The default comes from the registry. |
| `tag` | no | The tag. The default is the tag that equals the version, else the first tag of that version, else the version itself. An explicit tag must be a tag of that version in the registry, or the version itself. A floating tag, such as `1.0.x`, pins the build that the tag names now: see [Tag names](../internals/palette/packs.md#tag-names). |
| `params` | no | A map that Palette carries on a pack beside its values. Strata puts it in the authored hash, so a change to it is an `AUTHORED` change. |
| `manifests[]` | no | Each with a `name`, and `content` inline or a `contentFile`. |

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
  - A layer sets `values` or `valuesFile`, or neither. Not both.
  - A manifest sets `content` or `contentFile`. Not both, and not neither.

  So strata reports a misspelled field such as `valuesfile`.
- A values file or a manifest file that strata cannot read is `io`.

## The registry lookups

Strata reads `GET /v1/registries/metadata` once, and only when a layer names its
registry by name. It gives each registry name its uid. Then, for each layer:

1. `GET /v1/packs?filters=spec.name=<name>`, in pages of 100: the pack uid, the
   digest, the layer, the type, and the default values of the version in that
   registry.
2. `GET /v1/packs/<name>/registries/<uid>`: the tags of that version, and the
   default values and the layer when the listing gives none.

A version or a tag that the registry does not have is `pack-not-found`. A
registry name that the key cannot see is `registry-not-found`. A `registryUid`
that the key cannot see goes out as given, so its packs are `pack-not-found`,
and the error names the uid. A version that the listing gives as `disabled` or
`deleted` is `pack-unavailable`: Palette does not serve it, and refuses a
profile that pins it. See
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
`public`, with the label `Direct`. In the pack source, the presets of that
option set `charts.headlamp.service.type` to `LoadBalancer`.

In this example, the stand-in gives `cert-manager` 1.21.1 a template with one
parameter, `access` at `pack.access`, and one option, `public`:

```text
{{#include ../output/candidate-preset-yaml.txt}}
{{#include ../output/candidate-preset-warning.txt}}
{{#include ../output/candidate-preset-options.txt}}
```

A value that names no option, or values with no such key, give no warning and
no field. After a warning, record the version when Palette publishes it, and
compare that record with the candidate.

## Write one from a published version

[`strata candidate`](../commands/candidate.md) writes the file of a published
version, with each tag and registry uid pinned, and each value and manifest in
full.
