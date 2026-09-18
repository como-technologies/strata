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

This command writes the first file for you. Write the next one by hand, or from
the version before it.

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
