# strata behind

Report the pack versions that records pin, and the newer versions that the
registries offer. `verify` asks whether Palette still has what a record holds.
`behind` asks the other question: whether the registry has moved on since.

The examples use the project `dev`, whose records hold `aws-stack@1.0.0` and
`aws-stack@1.1.1`.

## The pins that are behind

```text
{{#include ../output/behind.txt}}
```

Exit `1`: a pin is behind. Each line is one pack version that the records pin
from one registry. A pin that is behind names the newer versions, newest first,
and `bump to` names what a move to the newest version does: the impact level and
the change flags of `compare pack:<name>@<pinned> pack:<name>@<newest>`. See
[Change flags and impact](../reference/impact.md). When Palette calls a
different newer version the latest, the line ends with
`; Palette calls <version> the latest`: see
[Which version is the newest](./packs.md#which-version-is-the-newest). The lines
under a pin name each record that pins it.

- **A newer version** is one that a person can move to. A version that the
  registry has disabled or deleted is not, and a version for none of the clouds
  of the pinned version is not either. A deprecated version is, and it says so.
- **A floating tag**, such as `LTS__24.x`, moves on its own within its range,
  and `verify` sees it move. Strata reads the version that the tag names now,
  and names only the versions above it, which the tag cannot reach:
  `LTS__20.4.x` names 20.04, and 22.04 and 24.04 are out of its reach. It also
  reads a tag that names a disabled or a deleted version.
- **A pinned version that the registry no longer lists** says so, as
  `the pinned version is absent`.
- A pack with no registry, such as a manifest pack, has no pins.

`--no-impact` skips the comparison of each bump: it costs two pack reads for each
pin that is behind.

## What arrived since the last run

Keep the JSON of a run, and give it to the next run with `--since`. Each pin then
names the newer versions that the previous run did not know:

```text
{{#include ../output/behind-last-week.txt}}
{{#include ../output/behind-since-lines.txt}}
```

Last week the registry had no `csi-aws-ebs` 1.65.0. Today it has, and the report
names it as arrived. A pack that the previous run had no pin of names no
arrival.

The registry also says when Palette first listed each version: `listed since`
under a pin that is behind, and `newer[].listed` in the JSON.

## In a script

```text
{{#include ../output/behind-json.txt}}
```

`--json` writes [`strata-behind/1`](../reference/documents.md#strata-behind1).

A scheduled job that keeps one issue open for each pack that is behind is
[Upstream issues](../automation/upstream-issues.md).

## What to expect

| Exit | Meaning |
| --- | --- |
| `0` | No pin is behind. |
| `1` | A pin is behind. |
| `2` | Strata could not check a pin, or could not read a record. |

- The records are record files, or quoted patterns such as
  `'releases/**/*.json'`: strata expands them. A record of a profile, a cluster,
  an edge host, a cluster group, an app profile, or a pack each gives the pins of
  its packs.
- Strata reads each pack of each registry one time, in the project of the
  first record that pins it. It reads the paged listing of the versions, and the
  tags of the pack in the registry.
- Behind is a decision, and not an error, so run `behind` apart from `verify`:
  the two fail for different reasons, and need different people.

## Options

```text
{{#include ../output/help-behind.txt}}
```

`--format`, `--json`, `--compact`, `--color`, `--no-color`, and `-q`
(`--quiet`) are the output options that each report command takes: see
[Output formats](./index.md#output-formats).
