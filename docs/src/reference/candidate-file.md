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

## The fields of a layer

| Field | Required | Function |
| --- | --- | --- |
| `name`, `version` | yes | The pack and its version. |
| `registry` or `registryUid` | yes, on the layer or at the top | The registry, by name or uid. |
| `values` or `valuesFile` | no | The values, inline or in a file. Not both. With neither, the layer gets the default values of the pack. |
| `layer` | no | The layer. The default comes from the registry. |
| `tag` | no | The tag. The default is the tag that equals the version, or else the first tag of that version. An explicit tag must be a tag of that version in the registry, or the version itself. |
| `params` | no | A map that Palette carries on a pack beside its values. Every profile of the live runs has it empty. Strata puts it in the authored hash, so a change to it is an `AUTHORED` change. |
| `manifests[]` | no | Each with a `name`, and `content` inline or a `contentFile`. |

> **Set `tag` to the tag that you publish.** Palette stores
> the tag that a profile was authored with. A profile from Terraform pins `24.04`,
> where the registry names that version `LTS__24.x`. The tag is part of
> `IDENTITY`, so another tag for the same build gives another root.
> `same-release` passes such a pair as `SAME CONTENT`.

## Checks before any request

Strata reads the whole file before it sends a request:

- An unknown field, or a file that breaks a rule, is `candidate-invalid`. So
  strata reports a misspelled field such as `valuesfile`.
- A values file or a manifest file that strata cannot read is `io`.

## The registry lookups

For each layer, strata reads the registry:

1. `GET /v1/registries/metadata`: a registry name to its uid.
2. `GET /v1/packs?filters=spec.name=<name>`: the pack uid, digest, layer, type,
   and default values of the version in that registry.
3. `GET /v1/packs/<name>/registries/<uid>`: the tags of that version.

A version or a tag that the registry does not have is `pack-not-found`. A
registry name that the key cannot see is `registry-not-found`.

The snapshot has `source.template` `candidate` and no uid. The digest on the
pack reference of a published profile equals the registry digest, so a
candidate with the published tags, values, and manifests has the published root.
The live example checks that: see [Live runs](../development/live-runs.md).

## Write one from a published version

[`strata candidate`](../commands/candidate.md) writes the file of a published
version, with each tag and registry uid pinned, and each value and manifest
spelled out.
