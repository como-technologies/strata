# A pack comes from another registry

## The problem

Some teams promote packs from one registry to another, for example from a
staging registry to a production registry. They then pin each pack to the new
registry. The pack is the same build with the same tag, version, and values. But
the registry uid, the pack uid, the digest, and often the pack type change. So
the roots differ, and `same-release` fails without `--allow registry-move`.

In this example, the prod copy of `aws-stack@1.1.1` takes `csi-aws-ebs` 1.65.0
from the OCI pack registry `Palette Registry`. The dev copy takes it from the
pack registry `Public Repo`. Prod also holds `1.1.2`, which moved the pack and
changed a CSI value on top of the move. The Public Repo values are the real
ones. The uid of the OCI registry and the pack uid it gives are stand-ins: see
[A pack that moves between registries](../internals/palette/packs.md#a-pack-that-moves-between-registries).

**Who runs this.** A person at a terminal, or the promotion gate, which runs
`same-release` for a job: see [The promotion gate and the retire gate](../automation/gates.md).

**Follow along.** `just playground registry-move` serves this tenant on this machine,
with no key and no cost.

## 1. Record each project's copy

```text
{{#include ../output/move-record.txt}}
```

## 2. same-release fails without the option

```text
{{#include ../output/move-exact.txt}}
```

The reason names the move. It does not name a value, because no value changed.

## 3. See what moved

```text
{{#include ../output/move-compare-csi.txt}}
```

`IDENTITY UPSTREAM` with no `AUTHORED`: another pack reference and another
digest, with the same configuration. The impact `cluster-wide` comes from the
layer and the flags, not from Palette. A live run moved the OS pack of a
running cluster between two registries at one version, and Palette replaced no
node. So on the OS, Kubernetes, or kernel layer, such a move takes the level of
its other flags, and not `repave`: see
[A pack that moves between registries](../internals/palette/packs.md#a-pack-that-moves-between-registries).

The JSON document names the fields that moved. For each pack whose identity
changed, it gives the pack uid, the registry uid, and the upstream digest on
each side:

```text
{{#include ../output/move-fields.txt}}
```

The pack uid and the registry uid differ, and the upstream digest differs with
them, because each registry stores its own copy of the build.

## 4. Accept a move, and nothing else

```text
{{#include ../output/move-allow.txt}}
```

`--allow registry-move` passes a pair whose only difference is packs from
another registry: the same tag, version, values, manifests, and params, with
the variables unchanged. The pack type can change, because it follows the
registry.

## 5. A move with a changed value still fails

`aws-stack@1.1.2` in prod moved the pack and changed a CSI value on top of the
move. Record it:

```text
{{#include ../output/move-rerecord.txt}}
```

The same command as step 4 now fails:

```text
{{#include ../output/move-allow-edited.txt}}
```

The prod copy moved and also changed a value. The policy accepts only the move,
so the pair fails and the reason names the value.

## What to do

- Use `--allow registry-move` only when your promotion moves packs between
  registries.
- A `manifest` pack comes from no registry. It never passes as a move.

## Palette behavior

A real move to an OCI registry changes the pack type from `spectro` to `oci`,
and the registry uid, pack uid, and digest. See
[What Palette does: a pack that moves between registries](../internals/palette/packs.md#a-pack-that-moves-between-registries).
