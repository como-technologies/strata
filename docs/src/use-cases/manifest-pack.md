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

<div class="playground-demo">

`just playground manifest-pack` serves this tenant on this machine,
with no key and no cost.

</div>

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

## 5. Review a change before you publish it

A team that keeps its own manifests in a profile reviews a change to them before
it publishes the version. A [candidate file](../reference/candidate-file.md)
describes that version. Write one from the published 1.0.0 in dev:

```text
{{#include ../output/manifest-candidate.txt}}
```

The layer has `type: manifest` and no registry. Keep the file in a repository.
Review each change to the manifest in a pull request. Make the record of the
file:

```text
{{#include ../output/manifest-candidate-record.txt}}
```

Strata sends no request to a registry for a `manifest` layer. The record has no
pack uid for the layer, because Palette gives the uid when it creates the
profile. So the root differs from the root of the published version by that uid
alone, and `same-release` passes the pair:

```text
{{#include ../output/manifest-candidate-same.txt}}
```

`mintedUidOnly` names each pack whose only difference is the pack uid. Here the
file comes from the published 1.0.0, so the pair passes. Do the same after you
publish a changed file. Record the new version. Then give `same-release` the
record of the file and the record of the version. A pass shows that Palette
holds the release that you reviewed.

## What to do

- Use `same-release`, not the equality of roots, to compare profiles with
  manifest packs.
- Give a manifest pack `type: manifest` in a candidate file, with no registry.
- A pack from a registry keeps its pack uid in the comparison, because there
  the uid names the build of the registry.

## Palette behavior

See [What Palette does: manifest pack uids](../internals/palette/packs.md#manifest-pack-uids).
