# strata snapshot

Record a profile version, a cluster, or a pack version in a snapshot file.
`compare`, `same-release`, and `verify` read the file later.

The examples use the project `dev`, which holds `aws-stack@1.1.0` and
`aws-stack@1.1.1`. The cluster `payments` runs 1.1.1.

## Record a profile version

```text
{{#include ../output/snapshot-profile.txt}}
```

Keep the file with the release, for example at
`releases/<version>/<project>.json` in a repository. The name is your choice.

The file holds the root and one hash for each pack. It also holds the values in
their canonical form and as Palette gave them, the manifests, the variables, and
the clusters that used the version. [The hashes and the root](../internals/hashes.md)
explains the hashes.

```text
{{#include ../output/snapshot-record.txt}}
```

`source.projectUid` is the project. `verify` checks the record there, also
after a rename of the project. [JSON documents](../reference/documents.md#strata-snapshot1)
lists each field.

## Record the newest version

```text
{{#include ../output/snapshot-latest.txt}}
{{#include ../output/snapshot-latest-version.txt}}
```

`<name>@latest` is the highest version in natural order: `22.04` is above
`20.04`, and `1.30.6` is above `1.29.10`.

## Record what a cluster runs

```text
{{#include ../output/snapshot-cluster.txt}}
{{#include ../output/snapshot-cluster-source.txt}}
```

A cluster that runs a recorded version has the root of that record: here
`088136c929bd`, the root of `aws-stack@1.1.1`. `cluster:<cluster>` records every
profile that the cluster runs, in the order that strata gives them: see
[The order of the profiles of a cluster](../internals/palette.md#the-order-of-the-profiles-of-a-cluster).
`cluster:<cluster>/<profile>` records one. The record keeps the uid of the
cluster, so `verify` finds a renamed cluster.

`pack:<name>@<version>` records one pack version from a registry, with the
values that the registry ships. `verify` on that record reads the registry
again, so it says whether a published pack version still serves the content it
served. See [What changed between two pack versions](../use-cases/pack-versions.md).

`source.cluster.variables` holds the value that the cluster gives each
variable. The values are not part of the root. Strata writes no value for a
sensitive variable. See [Clusters give variables different values](../use-cases/cluster-values.md).

## Whether an update waits

A cluster runs its own copy of each profile. When a person edits the profile in
place, the copy stays, and Palette tells the cluster that an update waits. The
record carries that statement in `source.cluster`:

- `updatesPending`: `true` when an update of a profile that the record covers
  waits, and `false` when none waits.
- `pendingUpdates[]`: for each change that waits, `profileUid`, `profile`, the
  `message` that Palette gives, such as `Manifest 'x' is updated in the pack
  'x(x)'`, and `since`.
- `updatesPendingError`: why strata could not read it. `updatesPending` is then
  absent, and the record is still taken.

These fields are not part of the root: they are Palette's statement about the
copy, and the copy is the same with or without them. `compare` and `list
clusters --applied` say when a cluster is behind. What Palette says for each
kind of change is in
[Cluster copies](../internals/palette.md#cluster-copies).

## Record without the network

`--raw-dir` saves the API responses and the time of the read. `--from-raw`
builds the same snapshot from them later, with no network and no key. The two
options exclude each other. An optional ref after `--from-raw` fills
`source.ref`. For a directory of a cluster, a ref of the form
`cluster:<uid>/<profile>` also selects that one profile. Strata checks the ref
against nothing else in the directory. The record keeps the time of the read,
not the time of the rebuild, so `verify` reports the version as unchanged since
Palette answered:

```text
{{#include ../output/snapshot-raw.txt}}
```

`scope.json` keeps the project. A directory with no `scope.json` builds a
snapshot at tenant scope.

## Record a version before you publish it

`--from-candidate` builds a snapshot from a candidate file. See
[strata candidate](./candidate.md) and
[Review a change before you publish it](../use-cases/review.md).

## Where to keep a record

A record is a file. Strata reads a record and writes a record. Strata does not
decide where you keep a record. Two places are usual, and each one answers a
different question:

| Where | What it gives | What it costs |
| --- | --- | --- |
| The repository, beside the code that publishes | A review, an author, and a date for each release. `git log` on the file says which commit pinned a release. | The job that publishes must push. |
| An object store, beside the state of the pipeline | Every job that publishes already writes there, and nothing new needs a token that pushes. | The history is the store's, not the repository's. |

A record holds each pack's values, params, and manifests in full. Only the value
of a sensitive profile variable is left out. So a record of a profile whose pack
values or manifests hold a secret carries that secret. Keep that record in the
object store, and not in the repository.

The book uses two layouts. The use cases keep one record for each version, at
`releases/<version>/<project>.json`, and one for each cluster, at
`runs/<cluster>.json`. [A team pins each environment](../use-cases/fan-out.md)
keeps one record for each environment, at `releases/<environment>.json`. Strata
reads both.

`verify` takes a pattern, so a directory of records is one command:
`strata verify "releases/*.json" --summary`. Read them from the store into a
directory first when they live outside the repository.

## What to expect

- Exit `0`, or `2` on an error.
- Without `-o`, the snapshot goes to standard output.
- `-o releases/1.1.0/dev.json` makes the directory that the path names, so a
  release directory needs no `mkdir` first.
- When a profile version has no published template, strata reads its draft.
  `source.template` names the one that it read: `published`, `draft`, `applied`
  for a cluster, `candidate`, or `registry` for a pack record.

## Options

```text
{{#include ../output/help-snapshot.txt}}
```
