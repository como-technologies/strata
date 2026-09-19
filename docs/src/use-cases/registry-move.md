# A pack comes from another registry

## The problem

Some teams promote packs from one registry to another, for example from a
staging registry to a production registry. They then pin the profile to the new
registry. The pack is the same build with the same tag, version, and values. But
the registry uid, the pack uid, the digest, and often the pack type change. So
the roots differ, and `same-release` fails without `--allow registry-move`.

In this example, the prod copy of `aws-stack@1.1.1` takes `csi-aws-ebs` 1.65.0
from the OCI registry `Palette Registry`. The dev copy takes it from the Public
Repo. The Public Repo values come from the live run `20260917T150025Z`. The
uid of the OCI registry and the pack uid it gives are stand-ins.

## 1. same-release fails without the option

```text
{{#include ../output/move-exact.txt}}
```

The reason names the move. It does not name a value, because no value changed.

## 2. See what moved

```text
{{#include ../output/move-compare.txt}}
```

`IDENTITY UPSTREAM` with no `AUTHORED`: another pack reference and another
digest, with the same configuration. The impact `cluster-wide` comes from the
layer and the flags, not from Palette. No live run moved a cluster between two
such versions.

```text
{{#include ../output/move-fields.txt}}
```

## 3. Accept a move, and nothing else

```text
{{#include ../output/move-allow.txt}}
```

`--allow registry-move` passes a pair whose only difference is packs from
another registry: the same tag, version, values, manifests, and params, with
the variables unchanged. The pack type can change, because it follows the
registry.

## 4. A move with a changed value still fails

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
[What Palette does: a registry move](../internals/palette.md#a-registry-move).
