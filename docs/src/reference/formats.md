# Report formats

`--format terminal` is the default. `--format markdown` is for a pull request
comment or an issue. `--format json`, or `--json`, is for a script or a language
model.

## The terminal report

```text
{{#include ../output/tag-verify.txt}}
```

| Line | Content |
| --- | --- |
| first | The two sides and their uids. A cluster side reads `<name>@<version> on cluster <cluster>`, with the cluster uid. |
| `root` | The two roots, and `IDENTICAL`, `SAME CONTENT`, or `CHANGED`. |
| `note` | A cause of the result. [strata compare](../commands/compare.md#notes-in-the-report) lists the notes. |
| `consumers` | The number of clusters that used each side. |
| `variables` | `unchanged`, or `CHANGED` with one line for each added or removed variable, and one line for each changed field. |
| `cluster values` | Only between two clusters with different variable values: one line for each value, as `<profile>.<variable>: "old" -> "new"`. A sensitive variable shows `(sensitive)`. |
| a pack | The layer, the name, the version, and the flags. Below a changed pack: the upstream digest, the changed values, the manifest changes, and the impact. |
| `DRIFT:` or `OK:` | Only after `verify`: the verdict. |

The variable traits are `required`, `sensitive`, `hidden`, and `immutable`. A
value in the `cluster values` block never changes `IDENTICAL`, because the
values of a cluster are not part of the release. A changed variable definition
does change it.

- Strata cuts a value longer than 120 characters, and gives its length. `--full`
  and the JSON document keep the full value.
- Strata compares a value that is not valid YAML, such as an unquoted Go
  template, as trimmed text. The report says so.
- `--full` adds a unified diff of the raw values and of each changed manifest.

### Colors

| Color | Used for |
| --- | --- |
| bold red | `repave` and `may-repave`, removed packs, old values |
| yellow | `cluster-wide`, changed packs |
| cyan | `in-place` |
| dim | `sync` and `none`, unchanged packs |
| green | added packs, new values |

Strata colors the report only when standard output is a terminal, and not when
`TERM` is `dumb`. `--color=always` forces color. `--color=never` or `--no-color`
removes it. `CLICOLOR_FORCE` forces color for a pipe, and `NO_COLOR` has
priority over it. The diffs of `--full` use the same colors.

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
- one table row for each pack, with the layer, name, version, flags, and impact
- one collapsible `diff` block for the variables, and one for the cluster values
  when two clusters give different values
- one collapsible `diff` block for each changed pack, with the changed values,
  the upstream digest, and the manifest changes
- with `--full`, the unified diffs in `diff` code blocks

`verify` adds a `Verify` line and the verdict.

## The JSON document

The JSON is never colored and never cut. `--compact` writes it on one line.
[JSON documents](./documents.md) describes each document.

## The one-line reports

`verify` with many files, `same-release`, `consumers`, and `list` print one line
for each item and a summary. Each command page shows the lines:
[verify](../commands/verify.md), [same-release](../commands/same-release.md),
[consumers](../commands/consumers.md), [list](../commands/list.md).
`--format markdown` writes the same lines as a table.
