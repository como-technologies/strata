# strata snapshot

Record a profile version, a cluster, an edge host, a cluster group, an app
profile, or a pack version in a snapshot file, or each cluster of a set in a
directory. `compare`, `same-release`, `verify`, and `behind` read the file
later.

The examples use the project `dev`, which holds `aws-stack@1.1.0` and
`aws-stack@1.1.1`. The cluster `payments` runs 1.1.1.

## What snapshot reads

One page for each kind of subject, with its output and the options that it
takes:

| Page | What it holds |
| --- | --- |
| [A profile version or a pack version](snapshot/profile-version.md) | `<name>@<version>`, `<name>@latest`, and `pack:<name>@<version>`: the record of a release, and where to keep it. |
| [A cluster](snapshot/cluster.md) | `cluster:<cluster>`: the profiles that a cluster runs, its tiers, whether an update waits for it, `@pending`, and `--macros`. |
| [Edge hosts, cluster groups, and app profiles](snapshot/other-objects.md) | `edgehost:`, `clustergroup:`, and `appprofile:`: the three other objects of Palette that hold profiles. |
| [A set of clusters](snapshot/set.md) | `tag:` and `filter:`, with `--dir` and `--prune`: one record for each cluster of a set. |
| [A record with no network](snapshot/no-network.md) | `--raw-dir` and `--from-raw`, and `--from-candidate`: a record from the saved answers of Palette, or from a candidate file. |

## What to expect

- Exit `0`, or `2` on an error.
- `--macros` and `--shape` need a `cluster:` ref. `--raw-dir` takes a profile or
  a cluster: a record read back from the directory does not say `@pending`,
  `@draft`, edge host, cluster group, or app profile.
- `--validate` takes `--from-candidate` or a profile version. With `--from-raw`,
  `--dir`, or an `@draft`, cluster, edge host, cluster group, app profile, or pack
  ref, it is a `usage` error. Palette refuses the packs: exit `2`, the error
  `palette-refuses`, and no record.
- Without `-o`, the snapshot goes to standard output.
- `-o releases/1.1.0/dev.json` makes the directory that the path names, so a
  release directory needs no `mkdir` first.
- When a profile version has no published template, strata reads its draft.
  `<ref>@draft` reads the draft of a published version: see
  [The draft of a version](snapshot/profile-version.md#the-draft-of-a-version).
  `source.template` names the template that strata read: `published`, `draft`,
  `applied` for a cluster, `candidate`, or `registry` for a pack record.

## Options

```text
{{#include ../output/help-snapshot.txt}}
```

| Option | What it does | Page |
| --- | --- | --- |
| `-o`, `--output` | Writes the record to a file. | |
| `--raw-dir` | Also saves the API responses and the time of the read in a directory. | [A record with no network](snapshot/no-network.md) |
| `--from-raw` | Builds the record from the directory of `--raw-dir`, with no network and no key. | [A record with no network](snapshot/no-network.md) |
| `--from-candidate` | Builds the record from a candidate file, before the version is in Palette. | [A record with no network](snapshot/no-network.md) |
| `--validate` | Asks Palette whether it takes the packs: of a candidate in a new profile, or of a profile version as an update of itself. A refusal exits `2` and writes no record. | [Ask Palette before you publish](candidate.md#ask-palette-before-you-publish) |
| `--compact` | Writes the JSON on one line. | |
| `--macros` | For a cluster: records the digest of the value that Palette gives each macro that the content uses. | [Macros](snapshot/cluster.md#macros) |
| `--shape` | For a cluster: records the `shape` tier, with one more request. | [More of the cluster than its release](snapshot/cluster.md#more-of-the-cluster-than-its-release) |
| `--dir` | For `tag:` or `filter:`: writes one record for each cluster of the set into a directory. | [A set of clusters](snapshot/set.md) |
| `--prune` | With `--dir`: removes each cluster record of the directory whose cluster the query no longer selects. | [A set of clusters](snapshot/set.md) |
| `-q`, `--quiet` | Drops the progress messages. | |
