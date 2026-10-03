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
| [A cluster](snapshot/cluster.md) | `cluster:<cluster>`: the profiles that a cluster runs, its tiers, whether an update waits for it, and `@pending`. `--macros` for a version, a cluster, or a cluster group. |
| [Edge hosts, cluster groups, and app profiles](snapshot/other-objects.md) | `edgehost:`, `clustergroup:`, and `appprofile:`: the three other objects of Palette that hold profiles. |
| [A set of clusters](snapshot/set.md) | `tag:` and `filter:`, with `--dir`, `--prune`, and `--json`: one record for each cluster of a set, and the report as a document. |
| [A record with no network](snapshot/no-network.md) | `--raw-dir` and `--from-raw`, and `--from-candidate`: a record from the saved answers of Palette, or from a candidate file. |

## What to expect

- Exit `0`, or `2` on an error.
- `--macros` takes a profile version, a `cluster:` ref, or a `clustergroup:`
  ref. `--shape` needs a `cluster:` ref. With another ref, each is a `usage`
  error. `--raw-dir` takes a profile or a cluster: a record read back from the
  directory does not say `@pending`, `@draft`, edge host, cluster group, or app
  profile.
- `--validate` takes `--from-candidate` or a profile version. With `--from-raw`,
  `--dir`, or an `@draft`, cluster, edge host, cluster group, app profile, or pack
  ref, it is a `usage` error. To ask Palette about a draft, write its candidate
  with `strata candidate <ref>@draft -o <file>`. Then give that file to
  `--from-candidate`. If Palette refuses the packs, strata exits `2` with the
  error `palette-refuses`, and writes no record.
- `--format` and `--json` need `--dir`: they set the format of the report of
  a set. A record is always JSON.
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
| `--from-candidate` | Builds the record from a candidate file, or from a profile export of Palette, before the version is in Palette. | [A record with no network](snapshot/no-network.md) |
| `--validate` | Asks Palette whether it takes the packs: of a candidate in a new profile, or of a profile version as an update of itself. A refusal exits `2` and writes no record. | [Ask Palette before you publish](candidate.md#ask-palette-before-you-publish) |
| `--compact` | Writes the JSON on one line: the record, or the document of a set. | |
| `--macros` | For a profile version, a cluster, or a cluster group: records the digest of the value that Palette gives each macro that the content uses. | [Macros](snapshot/cluster.md#macros) |
| `--shape` | For a cluster: records the `shape` tier, with one more request. | [More of the cluster than its release](snapshot/cluster.md#more-of-the-cluster-than-its-release) |
| `--dir` | For `tag:` or `filter:`: writes one record for each cluster of the set into a directory. | [A set of clusters](snapshot/set.md) |
| `--prune` | With `--dir`: removes each cluster record of the directory whose cluster the query no longer selects. | [A set of clusters](snapshot/set.md) |
| `--format` | With `--dir`: the format of the report, `terminal`, `markdown`, or `json`. | [The report as JSON](snapshot/set.md#the-report-as-json) |
| `--json` | With `--dir`: the same as `--format json`. Writes the `strata-set/1` document. | [The report as JSON](snapshot/set.md#the-report-as-json) |
| `-q`, `--quiet` | Drops the progress messages. | |
