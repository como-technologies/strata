# Change flags and impact

## The change flags

A changed pack shows one or more flags. Each flag comes from one hash of the
pack. See [The hashes and the root](../internals/hashes.md).

| Flag | Changes when | Meaning for the release |
| --- | --- | --- |
| `IDENTITY` | the type, tag, version, pack uid, or registry uid of the pack changes | The release points at another pack build. The name and the layer are the key of the pack, so a change to either gives an `ADDED` line and a `REMOVED` line. |
| `UPSTREAM` | the digest on the pack reference changes | The content of the pack changed under the same version. A push again under that version does this, and a floating tag that the registry moved does it too. See [A pack version pushed again](../internals/palette.md#a-pack-version-pushed-again) and [Tag names](../internals/palette.md#tag-names). A pack whose `upstreamSource` is `none` never shows this flag. |
| `AUTHORED` | the values, the attached manifests, or the params change | The configuration of the release changed. |

Other pack lines:

| Line | Meaning |
| --- | --- |
| `unchanged` | The node hash is the same on both sides. |
| `ADDED` | The pack is only on the second side. |
| `REMOVED` | The pack is only on the first side. |
| `tag <a> -> <b>` | Only the tag name changed, for the same name, layer, type, version, pack uid, and registry uid. It replaces `IDENTITY`. |

## The impact levels

Strata calculates an impact for each changed pack, from its layer and flags. The
impact is a hint. It is not an answer from Palette.

| Level | Condition | Meaning |
| --- | --- | --- |
| `repave` | OS, Kubernetes, or kernel layer with `IDENTITY`, other than a renamed tag | Palette replaces the nodes. |
| `may-repave` | OS, Kubernetes, or kernel layer with `AUTHORED` | Node or control plane configuration changed. Palette can replace the nodes. |
| `cluster-wide` | CNI or CSI layer with `IDENTITY` or `AUTHORED` | Each node gets the change. A repave is possible. |
| `in-place` | any other layer with `IDENTITY` or `AUTHORED` | Palette upgrades or configures the pack in place. |
| `sync` | only `UPSTREAM`, on any layer | Palette applies the change at the next profile sync. |
| `none` | no change, or only a tag renamed for the same build | The clusters run the same build. |

A pack key is `<layer>/<name>`. So a release that exchanges one pack for a pack
of another name gives one `ADDED` line and one `REMOVED` line, and no `IDENTITY`
change. Such a pack takes the impact of its layer, as an identity change does. That is
`repave` on the OS, Kubernetes, or kernel layer. It is `cluster-wide` on the
CNI or CSI layer, and `in-place` on any other layer.

The list goes from the most severe level to the least severe. The JSON summary
gives the most severe level of a comparison as `maxImpact`. A renamed tag
together with a changed value on the OS layer is `may-repave`, as the changed
value alone is.

A change of the pack order alone moves no flag and no level. Every pack stays
`unchanged`, and `maxImpact` is `none`. The report adds the line `note: pack
order changed between the two profiles`.

## Where to see it

- The terminal report: `impact:` below each changed pack.
- The markdown comment: the `impact` column.
- The JSON document: `packs[].impact.level` and `summary.maxImpact`.
- [Pull request comments](../automation/pr-comments.md): the label threshold.
