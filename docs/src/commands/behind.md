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

- **A newer version** is one that a person can move to. A version that Spectro
  Cloud disabled or deleted is not, and a version for none of the clouds of the
  pinned version is not either. A deprecated version is, and it says so. See
  [Pack states](../internals/palette/packs.md#pack-states).
- **A newer version that drops an architecture** says so: `1.36.2 (drops arm64)`.
  The registry names the processor architectures of most versions, `amd64` or
  `amd64,arm64`, and a newer version can support fewer than the pinned one.
  Strata says nothing when the listing names the architectures of only one of
  the two. See
  [The architectures of a version](../internals/palette/packs.md#the-architectures-of-a-version).
- **A floating tag**, such as `LTS__24.x`, names a version that changes.
  Strata reads the version that the tag names now, and shows it as `now`. It
  also reads a tag that names a disabled or a deleted version. A pin of an
  older version is behind the version that the tag names. `LTS__20.4.x` names
  20.04, so a pin of 20.04 under it is behind only 22.04 and 24.04.
- **Palette moves a profile under a floating tag only when the profile keeps
  the values of the pack.** A profile with values of its own stays on its
  version, and Palette shows the update to a person. Until the person applies
  it, the record pins the older version, and `behind` names the version that
  the tag names now. See [Tag names](../internals/palette/packs.md#tag-names).
- **A version that Palette does not serve in the tenant** is not a newer
  version. In a FIPS tenant, it is a version that is not FIPS compliant. See
  [The FIPS rule](../internals/palette/packs.md#the-fips-rule).
- **A pinned version that the registry no longer lists**, or does not serve in
  the tenant, says so, as `the pinned version is absent`.
- A pack with no registry, such as a manifest pack, has no pins.

`--no-impact` skips the comparison of each bump: it costs two pack reads for each
pin that is behind.

## A pin that is current against an older copy

Palette keeps a copy of each registry, and a sync job refreshes the copy.
`behind` compares each pin with that copy, as `verify` of a pack record does.
When the last sync of a registry did not complete, the copy is the copy of an
earlier sync. A pin that is `current` is then current against that older copy,
and its line names the sync:

```text
{{#include ../output/behind-stale.txt}}
```

- `; a sync of the registry is in progress since <time>, and the last complete
  sync of the registry started <time>`: a sync runs now. The copy is the copy
  of the last complete sync until the sync that runs completes.
- `; the last complete sync of the registry started <time>; the last sync of
  the registry failed: <message>`: the last sync failed, and the message is
  from Palette. The copy is the copy of the last complete sync.
- `; the recovery of the sync is <state>: <message>`: a sync stopped before
  its end, and Palette recovers it. This note can follow each of the notes
  above.

A sync can stay in progress for a long time: see
[The sync of a registry](../internals/palette/packs.md#the-sync-of-a-registry).
A pin whose registry completed its last sync has no note. Palette shows the
sync of each registry, and the copy is the copy of the last run. A pin that is
behind has no note either: a newer version in the copy is a newer version in
the registry. `--json` gives the sync in `pins[].registrySync`, with the fields
of `verify.registrySync`: see
[The sync of the registry of a pack record](./verify.md#the-sync-of-the-registry-of-a-pack-record).
When strata cannot read the sync, the pin stays current, and
`pins[].registrySyncError` gives the reason.

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

## A pin that strata cannot check

```text
{{#include ../output/nodigest-chart-behind.txt}}
```

Exit `2`: strata could not check a pin, and the line under the pin says why.
Here the record is a candidate with a chart from a registry that Palette does
not sync. Palette lists a pack of such a registry only while a profile pins the
pack, so the registry has no version to compare with. See
[A pack that Palette makes at the create](../internals/palette/packs.md#a-pack-that-palette-makes-at-the-create).
The JSON keeps the error on the pin, with `unsynced`: see
[Errors](../reference/errors.md).

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
- For each registry with a pin that is current, strata also reads the sync of
  the registry one time, from the summary listing of its kind.
- Behind is a decision, and not an error, so run `behind` apart from `verify`:
  the two fail for different reasons, and need different people.

## Options

```text
{{#include ../output/help-behind.txt}}
```

`--format`, `--json`, `--compact`, `--color`, `--no-color`, and `-q`
(`--quiet`) are the output options that each report command takes: see
[Output formats](./index.md#output-formats).
