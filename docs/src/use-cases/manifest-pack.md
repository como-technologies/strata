# One manifest in two profiles

## The problem

A manifest pack holds Kubernetes manifests and comes from no registry. Palette
gives the manifest pack of each profile its own pack uid, also for a clone with
identical content. The pack uid is part of `IDENTITY`, so two profiles with the
same manifest never have the same root.

In this example, `dev` and `prod` each have `platform-addons@1.0.0`, with one
manifest pack and the same Namespace manifest. The two pack uids are stand-ins,
of the shape Palette gives.

## 1. Record the version in each project

```text
{{#include ../output/manifest-record.txt}}
```

## 2. Compare the records

```text
{{#include ../output/manifest-compare.txt}}
```

The root line says `SAME CONTENT`, and the note names the cause. The pack line
still shows `IDENTITY` and an impact, because the pack uid differs. `compare`
exits `1`, because the roots differ.

## 3. The records are one release

```text
{{#include ../output/manifest-same.txt}}
```

`same-release` passes the pair with or without `--allow registry-move`. It never
fails a pair because of the pack uid of a manifest pack.

## 4. A changed manifest still fails

Here the two manifests differ in one line: prod sets the Pod Security level to
`baseline`, and dev sets it to `restricted`. The report names the manifest and
the hash, not the line:

```text
{{#include ../output/manifest-different.txt}}
```

## What to do

- Use `same-release`, not the equality of roots, to compare profiles with
  manifest packs.
- A pack from a registry keeps its pack uid in the comparison, because there
  the uid names the build of the registry.

## Palette behavior

See [What Palette does: manifest pack uids](../internals/palette.md#manifest-pack-uids).
