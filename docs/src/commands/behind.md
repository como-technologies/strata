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
from one registry. A pin that is behind names the newer versions, newest first.
`bump to` names what a move to the newest version does: the impact level and
the change flags of `compare pack:<name>@<pinned> pack:<name>@<newest>`. See
[Change flags and impact](../reference/impact.md).

When Palette calls a
different newer version the latest, the line ends with
`; Palette calls <version> the latest`: see
[Which version is the newest](./packs.md#which-version-is-the-newest). The lines
under a pin name each record that pins it.

- **A newer version** is one that a person can move to. A version that Spectro
  Cloud disabled or deleted is not, and a version for none of the clouds of the
  pinned version is not either. A pre-release is not one, unless the pin is a
  pre-release or the option `--pre-release` asks for it: see
  [A pre-release](#a-pre-release). See
  [A pack that supports a cloud](../internals/palette/packs.md#a-pack-that-supports-a-cloud).
  A deprecated version is, and it says so. See
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
  the values of the pack**. A profile with values of its own stays on its
  version, and Palette shows the update to a person. Until the person applies
  it, the record pins the older version, and `behind` names the version that
  the tag names now. A live run shows it: see
  [Tag names](../internals/palette/packs.md#tag-names).
- **A version that Palette does not serve in the tenant** is not a newer
  version. In a FIPS tenant, it is a version that is not FIPS compliant. See
  [The FIPS rule](../internals/palette/packs.md#the-fips-rule).
- **A pinned version that the registry no longer lists**, or does not serve in
  the tenant, says so, as `the pinned version is absent`.
- A pack with no registry, such as a manifest pack, has no pins.

`--no-impact` skips the comparison of each bump and the question to Palette. The
comparison costs two pack reads for each pin that is behind.
`--pre-release` names a pre-release as a newer version of a pin that is not a
pre-release. `--pin` reports one pack: see [One pack](#one-pack).

## One pack

`--pin <layer>/<name>` reports the pins of one pack. The key is the layer and the
name, such as `k8s/kubernetes`. Strata reads the records first. It reads that pack
and no other pack. For each pin that is behind, it compares the move and asks
Palette about it. The summary counts the pins of the pack alone:

```text
{{#include ../output/behind-pin.txt}}
```

Exit `1`: a pin is behind. The records pin two versions of the pack, and one is
behind. The lines under a pin are as in the report for all packs.

A key that no record pins from a registry is a `usage` error. Strata checks the
key after it reads the records and before it connects to Palette. So the error
needs no key:

```text
{{#include ../output/behind-pin-none.txt}}
```

Exit `2`. A pack with no registry, such as a manifest pack, has no pins, so it
gives the same error.

`--pin` and `--no-impact` cannot go together. `--pin` compares the move and asks
Palette about it, and `--no-impact` skips both. Strata refuses the pair with exit
`2`, and it sends no request.

## A pre-release

A pipeline can publish a pre-release for each build, such as `1.66.0-rc.1`. Strata
orders versions by Semantic Versioning, as Palette does: see
[The version that Palette calls the latest](../internals/palette/packs.md#the-version-that-palette-calls-the-latest).
A pre-release is below its version, so it is no newer version of a pin that is
not a pre-release. Here
the registry has `csi-aws-ebs` `1.66.0-rc.1`, and the record pins `1.65.0`:

```text
{{#include ../output/behind-pre-release.txt}}
```

`--pre-release` names the pre-release as a newer version:

```text
{{#include ../output/behind-pre-release-lines.txt}}
```

A pin that is a pre-release is behind its final version and each later version.
`1.0.1-g0123456789ab` is behind `1.0.1`. Strata names each later pre-release too,
because the pin follows the pre-releases. Two pre-releases of one version
compare by their identifiers, as the rules of Semantic Versioning say. So the
text of a hash, not the time of the build, orders `1.0.1-g0123456789ab` and
`1.0.1-g0123456789ac`. Pin the final version, and not a pre-release.

A develop build such as `1.66.0.develop` is a pre-release too: it is below
`1.66.0`. A version with four numbers is not: `1.65.0.1` is above `1.65.0`, and
below `1.66.0`. A text with two numbers and a tail of text, such as
`1.66.develop`, is a pre-release of `1.66.0`. So `--pre-release` names it as
newer than `1.65.0`. It is below `1.66.0.develop`. See
[the rule of `@latest`](../reference/refs.md#rules).

## Palette accepts the move, or refuses it

`bump to` says what a move does. It does not say that Palette takes the move.
Palette can refuse a move: see
[The upgrade dependency](../internals/palette/validate-routes.md#the-upgrade-dependency)
for the rule.

For each pin that is behind, `behind` asks Palette about the move to the newest
version. The answer is under the `bump to` line:

```text
{{#include ../output/behind-refused.txt}}
```

- `palette accepts`: Palette gives no error for the move.
- `palette refuses: <code> on <clusters>`: Palette gives the error `<code>`, and
  names the clusters in its message. `DependencyPackUpgradeNotSupported` is the
  code of a move that the pack does not allow.
- `palette refuses: <code> <pack>: <message>`: the message names no cluster that
  runs the version, so the line gives the message of Palette.
- `palette gave no answer: <message>`: a request failed, or strata did not ask
  a record of another project. The pin stays behind, and the exit does not
  change. Strata prints `palette accepts` only when it asked each record and
  each request answered.

`--json` gives the answer in `bump.paletteValidation`: the code, the message of
Palette, and the clusters of each error. A request that failed, and a record
that strata did not ask, are in `bump.paletteValidationError`.

```text
{{#include ../output/behind-refused-json.txt}}
```

Strata asks once for each record that pins the pack in the project of the pin.
The answer holds the errors of all the records:

- A record of a profile version that Palette holds goes to
  `POST /v1/clusterprofiles/{uid}/validate/packs`. The body has the packs of the
  record, with the newest version of the pack in the place of the pin. That
  version has its default values. Palette checks the body against each cluster
  that runs the version.
- A record of a cluster goes to the same route. The uid is the uid of the
  profile that holds the pin: the profile version that the cluster runs. The
  body has the packs of that profile as the cluster runs them. The newest
  version of the pack replaces the pin. Palette checks the body against each
  cluster that runs the version, this cluster too.
- A candidate, a pack record, an edge host, a cluster group, and an app profile
  get no request. The update that waits for a cluster gets none.

Strata does not send the bump of a cluster record to the route of the cluster,
`POST /v1/spectroclusters/{uid}/validate/packs`. For a profile that the cluster
runs, that route keeps the tag of the copy that the cluster holds, and it checks
no sent tag. So it accepts each move, also a move to a version that the
registry does not have. See
[The validate routes](../internals/palette/validate-routes.md#post-v1spectroclustersuidvalidatepacks).

A record of a cluster and the record of the profile that the cluster runs pin
the same packs. So `behind` gives them the same bump: the same newest version,
the same impact level, and the same change flags. They also get the same
question and the same answer of Palette. This is the record of the cluster
`payments`, which runs `aws-stack@1.1.11`:

```text
{{#include ../output/behind-cluster.txt}}
```

The message of an error names the clusters. See
[The upgrade dependency](../internals/palette/validate-routes.md#the-upgrade-dependency)
for the field.
When Palette refuses a move, strata reads the document of the profile version.
It keeps the clusters that run the version and that the message names. This
holds for a record of a cluster too.

Strata asks in the project of the first record that pins the pack, because
Palette answers in the project of the connection. It does not ask a record of
another project. It counts the records of another project that it can ask.
The line `palette gave no answer` says how many, as in
`1 record of project <project> was not asked`.

A refusal that Palette gave stands, and this line comes under it. A record that
gets no request, such as a pack record, adds nothing to the count.
[The validate routes](../internals/palette/validate-routes.md#post-v1clusterprofilesuidvalidatepacks)
states what each route checks.

`--no-impact` sends none of these requests. Without it, strata sends one request
for each record that it asks about, for each pin that is behind. It reads the
document of each cluster record one time, and the document of a profile version
one time when Palette refuses a move.

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
  the registry failed: <message>`: the last sync failed. The message is
  from Palette. The copy is the copy of the last complete sync.
- `; the recovery of the sync is <state>: <message>`: a sync stopped before
  its end, and Palette recovers it. This note can follow each of the notes
  above.

A sync can stay in progress for a long time: see
[The sync of a registry](../internals/palette/packs.md#the-sync-of-a-registry).
A live run shows the note with the Bitnami registry of the tenant, whose sync
stays in progress: see [packs.sh](../development/live-runs.md#packssh).

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

`--since` takes a `strata-behind/1` document, by the rule of the records (see
[The documents](../reference/compatibility.md#the-documents)). A report of
another `strata-behind` id is `unsupported-schema`, which names both ids. A file
of another document, such as a record, or of no document is a `usage` error.

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
  `'releases/**/*.json'`: strata expands them. A record gives the pins of its
  packs. The record can be of a profile, a cluster, an edge host, a cluster
  group, an app profile, or a pack.
- Strata connects one time for each project of the pins. A pin is in the
  project of the first record that pins it. For each project, strata reads each
  listing of the registries one time.
- Strata reads each pack of each registry one time, in the project of the
  first record that pins it. It reads the paged listing of the versions, and the
  tags of the pack in the registry. With `--pin`, it reads the one pack of the
  key.
- For each registry with a pin that is current, strata also reads the sync of
  the registry one time. It comes from the summary listing of its kind.
- When strata cannot connect for a project, each pin of that project has the
  error. Strata still reads the pins of the other projects.
- Behind is a decision, and not an error, so run `behind` apart from `verify`:
  the two fail for different reasons, and need different people.

## Options

```text
{{#include ../output/help-behind.txt}}
```

`--format`, `--json`, `--compact`, `--color`, `--no-color`, and `-q`
(`--quiet`) are the output options that each report command takes: see
[Output formats](./index.md#output-formats).
