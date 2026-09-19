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
a YAML file of layers, a Terraform module, a chart of values. A candidate file
is a second view of that description, in the form strata reads, and a job writes
it beside the source. The fields it needs are few:

| The candidate asks for | Your source usually calls it | When it is missing |
| --- | --- | --- |
| `name`, `version` | the profile and the version to publish | required; the name is the name in Palette, with whatever prefix your pipeline adds |
| `type`, `cloudType` | the kind of profile and its cloud | `type` is `cluster` by default |
| `layers[].name`, `layers[].version` | each pack and its version | required |
| `layers[].registry` | the registry a layer comes from | a top-level `registry` covers every layer that names none |
| `layers[].valuesFile` | the values file of a pack | with no `values` and no `valuesFile`, the layer gets the default values of the pack |
| `layers[].tag` | the tag, when it is not the version | the tag that equals the version, or the first tag of that version |
| `variables[]` | the profile variables | none |

Two rules decide whether the roots agree, so a job that writes candidates must
follow them:

- **The version is the tag by default.** When your pipeline pins a pack by a tag
  that is not the version, the candidate must say `tag`.
- **Palette's spelling wins.** Strata takes the form Palette returns, so a
  variable field such as `display_name` is `displayName` in the candidate.

Prove the mapping once, before a pipeline depends on it: write a candidate from
a version that Palette already has, and check that the two are one release. The
steps below do exactly that, and
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
