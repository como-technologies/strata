# strata candidate

`strata candidate <ref>` writes a candidate file from a profile version that
Palette already has.

## What a candidate file is

A profile version is a stack of packs with their values, and the variables of
the profile. Palette holds a version after somebody publishes it. A candidate
file holds the same content as YAML, in your repository, before Palette has it.

A person reads and edits the file. A [record](./snapshot.md) holds the same
version as hashes, which a machine compares.
[The candidate file](../reference/candidate-file.md) gives each field.

Strata does not publish a candidate, and strata never writes to Palette. Your
pipeline publishes the version. Strata tells you what that version will be.

## Why you write one

- **To review a change before the version exists.** The change is a diff in a
  pull request. `strata snapshot --from-candidate` calculates the root of the
  version to come. `strata compare` then names each layer that changes, and the
  impact on a cluster. See
  [Review a change before you publish it](../use-cases/review.md).
- **To keep the version in git.** `--files-dir` writes the values of each layer
  to their own file. A change to one value is then a change to one line, with an
  author and a reason.
- **To prove that Palette has the release you approved.** Record the published
  version. Then run `strata same-release candidate.json published.json`. Exit
  `0` says that the two are one release.

This command writes the first file for you. Write the next one by hand, from the
version before it, or from what your repository already holds.

## Write one from your own source

A team that publishes with a pipeline already describes each profile somewhere:
a YAML file of layers, a Terraform module, a chart of values. A candidate file is
that same description, in the form strata reads. A job writes the candidate file
beside the source. The file needs few fields:

| The candidate asks for | Your source usually calls it |
| --- | --- |
| `name`, `version` | the profile and the version to publish; the name is the name in Palette, with whatever prefix your pipeline adds |
| `type`, `cloudType` | the kind of profile and its cloud |
| `layers[].name`, `layers[].version` | each pack and its version |
| `layers[].registry` | the registry a layer comes from |
| `layers[].valuesFile` | the values file of a pack |
| `layers[].tag` | the tag, when it is not the version |
| `variables[]` | the profile variables |

[The candidate file](../reference/candidate-file.md#the-fields-of-a-layer) gives
each field, the ones that are required, and each default.

Four rules decide whether the roots agree, so a job that writes candidates must
follow them:

- **The tag is the version by default, when the registry lists that tag.** A
  pipeline can pin a pack under another tag. The candidate must then give that
  tag in `tag`.
- **Strata uses the names that Palette uses.** A variable field such as
  `display_name` is `displayName` in the candidate file.
- **Strata applies no pack preset.** The documents that strata builds for a
  candidate carry an empty `presets` list.
  [What no run covers](../internals/palette/limits.md#what-no-run-covers) names the
  open question about the values that a preset option needs.
- **A candidate pins a pack version, and not a tag.** A record of a profile
  pinned under a floating tag gives a candidate with that version and that tag.
  After the registry moves the tag to another version, `snapshot
  --from-candidate` stops with `pack-not-found`. Give the new version in the
  candidate.

Prove the mapping before a pipeline depends on it. Write a candidate file from a
version that Palette already has. Then check that the two are one release. The
steps below do this, and
[Build the snapshot of a candidate](#build-the-snapshot-of-a-candidate) shows
the answer. Run it again when the shape of your source changes.

The examples use the project `dev`, which holds `aws-stack@1.1.1`.

## Write a candidate file

```text
{{#include ../output/candidate-write.txt}}
{{#include ../output/candidate-files.txt}}
```

`--files-dir` puts the values of each layer and each manifest in a file. Keep
this form in a repository: a pull request then shows a change to a value as a
change to one line.

```text
{{#include ../output/candidate-yaml.txt}}
```

The file pins the tag and the registry uid of each layer. It carries the values,
the params, the manifests, and the variables.

## Build the snapshot of a candidate

```text
{{#include ../output/candidate-snapshot.txt}}
```

Strata finds each pack in its registry, and builds the documents that Palette
gives for a published profile with that content. Here the file comes from the
published 1.1.1, so its root equals the root of that version:

```text
{{#include ../output/candidate-published.txt}}
{{#include ../output/candidate-same.txt}}
```

The two roots stay equal when the registry still has the same pack versions.

After a person edits the published version in place, `same-release` between
the candidate and the version fails and names the edited value.

## What to expect

- The ref is a profile uid, `<name>@<version>`, `<name>@latest`, or a profile
  record. A cluster ref or a pack ref is an error: `profile-ref-required`. A
  pack is one layer of a profile version, and a cluster holds a copy of one.
- `--files-dir` needs `-o`.
- Exit `0`, or `2` on an error.

[The candidate file](../reference/candidate-file.md) describes each field,
and how to write a file by hand.

## Options

```text
{{#include ../output/help-candidate.txt}}
```
