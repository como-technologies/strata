# One manifest in two profiles

## The problem

Most packs come from a registry. `ubuntu-aws` and `kubernetes` are builds that a
registry holds, each with a version and a digest.

A manifest pack is not one of those. It holds Kubernetes YAML that somebody on
your team wrote, such as the Namespace in this example. It comes from no
registry, and it has no build to point at. You add it to a profile as a layer,
in the same way as a pack from a registry.

Palette gives the manifest pack of each profile its own pack uid. A copy with
the same content gets its own uid too. The pack uid is part of `IDENTITY`, so
two profiles with the same manifest never have the same root.

In this example, `dev` and `prod` each have `platform-addons@1.0.0`, with one
manifest pack and the same Namespace manifest. Prod also has `1.0.1`, whose
manifest sets another Pod Security level. The pack uids are stand-ins, of the
shape Palette gives.

**Who runs this.** A person at a terminal, working out why two roots differ.

**Follow along.** `just playground manifest-pack` serves this tenant on this machine,
with no key and no cost.

## 1. Record the version in each project

```text
{{#include ../output/manifest-record.txt}}
```

## 2. Compare the records

```text
{{#include ../output/manifest-compare-note.txt}}
```

The root line says `SAME CONTENT`, and the note names the cause. The pack line
still shows `IDENTITY` and an impact, because the pack uid differs. `compare`
exits `1`, because the roots differ.

## 3. The records are one release

```text
{{#include ../output/manifest-same.txt}}
```

`same-release` passes the pair, and it needs no option. It never fails a pair
because of the pack uid of a manifest pack.

## 4. A changed manifest still fails

`platform-addons@1.0.1` in prod sets the Pod Security level to `baseline`, where
1.0.0 keeps `restricted`. Record it:

```text
{{#include ../output/manifest-rerecord.txt}}
```

The same command as step 3 now fails, and names the manifest, not the line:

```text
{{#include ../output/manifest-different.txt}}
```

`compare --full` names the line:

```text
{{#include ../output/manifest-full-diff.txt}}
```

## What to do

- Use `same-release`, not the equality of roots, to compare profiles with
  manifest packs.
- A pack from a registry keeps its pack uid in the comparison, because there
  the uid names the build of the registry.

## Palette behavior

See [What Palette does: manifest pack uids](../internals/palette/packs.md#manifest-pack-uids).
