# Report formats

`--format terminal` is the default. `--format markdown` is for a pull request
comment or an issue. `--format json`, or `--json`, is for a script or a language
model.

Every report goes to standard output. Progress notes go to standard error: see
[Settings](./settings.md).

## The terminal report

```text
{{#include ../output/tag-verify.txt}}
```

| Line | Content |
| --- | --- |
| first | The two sides and their uids. A cluster side reads `<name>@<version> on cluster <cluster>`, with the cluster uid. The update that waits for a cluster reads `<name>@<version> pending for cluster <cluster>`. An edge host side reads `<name>@<version> on edge host <host>`, and a cluster group side reads `<name>@<version> pinned by cluster group <group>`. A pack side reads `pack <name>@<version>`, with the pack uid. |
| `root` | The two roots, and `IDENTICAL`, `SAME CONTENT`, or `CHANGED`. |
| `note` | A cause of the result. [strata compare](../commands/compare.md#notes-in-the-report) lists the notes. |
| `palette` | Only with `--repave`: the answer of Palette, `repave` or `no repave`, for the cluster side. See [strata compare](../commands/compare/repave.md). |
| `images` | The number of images and charts that changed, and the number of packs that carry them. Below a changed pack, one line for each changed image or chart. |
| `consumers` | The number of clusters that used each side. When one side is a cluster, a candidate, or a pack, the line is a sentence about the profile side alone: see [strata compare](../commands/compare/versions.md#compare-two-versions). Between two clusters, or two packs, there is no line. |
| `variables` | `unchanged`, or `CHANGED` with one line for each added or removed variable, and one line for each changed field. Between two packs there is no line. |
| `cluster values` | Only between two clusters with different variable values: one line for each value, as `<profile>.<variable>: "old" -> "new"`. A sensitive variable shows `(sensitive)`. |
| `cluster tiers` | Only between two cluster records: the state of each tier (`applied`, `policy`, `running`, `shape`, `labels`), and one line for each changed field. See [Two clusters on one release differ](../use-cases/cluster-tiers.md). |
| a pack | The layer, the name, the version, and the flags. Below a changed pack: the upstream digest, the changed values, the manifest changes, and the impact. `upstream unknown` says that the pack has no upstream digest. |
| `DRIFT:`, `UNVERIFIED:`, or `OK:` | Only after `verify`: the verdict. |
| `who:` | Only after `verify --who` on a record that drifted: the writes in the audit log since the record, one line each. |

The variable traits are `required`, `sensitive`, `hidden`, and `immutable`. A
value in the `cluster values` block never changes `IDENTICAL`, because the
values of a cluster are not part of the release. A changed variable definition
does change it.

- Strata cuts a value longer than 120 characters, and gives its length. `--full`
  and the JSON document keep the full value.
- Strata cuts a root to its first twelve characters. An upstream digest keeps
  the name of its algorithm, and its hash is cut to twelve characters:
  `sha256:ff3df77ebc9c`. The markdown comment cuts them the same way, and the
  JSON document keeps them full.
- Strata compares a value that is not valid YAML, such as an unquoted Go
  template, as trimmed text. The report says so.
- `--full` adds a unified diff of the raw values and of each changed manifest.

### Colors

| Color | Used for |
| --- | --- |
| bold green | `IDENTICAL`, `SAME CONTENT`, `OK`, `ADDED`, and the same-content note |
| bold red | `CHANGED`, `DRIFT`, `REMOVED`, `repave`, and `may-repave` |
| bold yellow | `UNVERIFIED`, the other notes, and `variables CHANGED` |
| blue | `IDENTITY`, and a renamed tag |
| magenta | `UPSTREAM` |
| yellow | `AUTHORED`, `cluster-wide`, the `~` of a changed pack, and the pack-order note |
| dim yellow | `IMAGES` and `CHARTS`: details of `AUTHORED`, so `AUTHORED` reads as the flag |
| cyan | `in-place`, and the `@@` line of a unified diff |
| red | old values, and the `-` of a removed line |
| green | new values, and the `+` of an added line |
| dim | `sync`, `none`, unchanged packs, uids, and roots |

Strata decides the color in this order. `--color=never` or `--no-color`
removes it. `--color=always`, or `--color` with no value, forces it. Then
`NO_COLOR` removes it. Then `CLICOLOR_FORCE` forces it, also for a pipe and
also when `TERM` is `dumb`. Then `TERM=dumb` removes it. Then a terminal on
standard output gets it. The diffs of `--full` use the same colors.

## The markdown comment

```text
{{#include ../output/compare-markdown.txt:1:2}}
```

{{#include ../output/compare-markdown.txt:3:}}

The comment has:

- a header table with the two uids, the two roots, the consumers of each side,
  and the fetch times
- a `Consumers:` line in place of that row, when only one side is a profile
  version: against a candidate, a cluster, or a pack
- a result line with the result, the maximum impact, the pack counts, and the
  state of the variables
- a `Note:` line for each note of the terminal report, and the order note as a
  quotation after the pack blocks. [strata compare](../commands/compare.md#notes-in-the-report)
  lists the notes
- one table row for each pack, with the layer, name, version, flags, and impact
- one collapsible `diff` block for the variables, and one for the cluster values
  when two clusters give different values
- one collapsible `diff` block for each changed pack, with the changed values,
  the upstream digest, and the manifest changes
- with `--full`, the unified diffs in `diff` code blocks

`verify` adds a `Verify` line and the verdict. The `Verify` line states the
status in the word of the verdict: `OK`, `UNVERIFIED`, or `DRIFT`.

## The JSON document

The JSON is never colored and never cut. `--compact` writes it on one line.
[JSON documents](./documents.md) describes each document.

## The one-line reports

`verify` with many files or with `--summary`, `same-release`, `consumers`,
`behind`, `list`, and `packs` print one line for each item. `verify`,
`same-release`, and `behind` close with a summary line. `consumers`, `list`,
and `packs` head each group with a count. `snapshot` of a `tag:` or `filter:`
set prints one `wrote` or `removed` line for each file. Each command page shows
the lines: [verify](../commands/verify.md),
[same-release](../commands/same-release.md),
[consumers](../commands/consumers.md), [behind](../commands/behind.md),
[list](../commands/list.md), [packs](../commands/packs.md), and
[snapshot](../commands/snapshot.md). `--format markdown` writes the same lines
as a table.
