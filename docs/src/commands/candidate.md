# strata candidate

Write a candidate file from a profile version or a record. A candidate file
describes a profile version. `strata snapshot --from-candidate` builds its
snapshot, with the root of the version after somebody publishes it.

Start a file from a published version with this command. Then change the file in
a pull request, and compare the candidate with the last release:
[Review a change before you publish it](../use-cases/review.md).

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
