# strata behind

Report the pack versions that records pin, and the newer versions that the
registries offer. Also report the profile versions that records hold, and the
newer versions of each profile that Palette holds. `verify` asks whether Palette
still has what a record holds. `behind` asks the other question: whether the
registry or the pipeline has a newer version since then.

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

The first line counts each pin one time, by its state: `behind`, `current`, or
`not checked`. So the counts add up to the pins. A pin whose comparison failed is
`not checked`, also when it is behind. The count in parentheses is the pins with a
floating tag, and each of them is also behind or current. The profile versions
have counts of their own, in the same form, and they can also be `gone`.

When Palette calls a
different newer version the latest, the line ends with
`; Palette calls <version> the latest`: see
[Which version is the newest](./packs.md#which-version-is-the-newest). The lines
`pinned by` under a pin name each record that pins it. For a cluster with many
profiles, the line names the profile that holds the pack: see
[A cluster with many profiles](#a-cluster-with-many-profiles).

The line `next patch` names the newest patch of the minor version of the pin. It
comes when the newest version is of another minor version, as `1.36.2` is for the
pin `1.35.3`. A team can take the patch before the move. The line gives the
command that says what the move to the patch does, and strata sends no request for
it. Run the command:

```text
{{#include ../output/behind-next-patch.txt}}
```

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

A line such as `aws-stack@1.1.1 carries cni-cilium-oss@1.20.1` names the newer
profile version that pins a newer version of the pack. See
[The newer profile version that carries a newer pack](#the-newer-profile-version-that-carries-a-newer-pack).
The last lines name the profile versions of the records: see
[Newer versions of a profile](#newer-versions-of-a-profile).

`--no-impact` skips the comparison of each bump and the question to Palette. The
comparison costs two pack reads for each pin that is behind. For a profile
version that is behind, it reads the pack manifests and the variables of the two
versions. See [The move to the newest profile version](#the-move-to-the-newest-profile-version).
`--pre-release` names a pre-release as a newer version of a pin or of a profile
version that is not a pre-release. `--pin` reports one pack: see [One pack](#one-pack).

`--behind-only` shows only the rows that are behind, or that strata could not
check: see [Only the rows that are behind](#only-the-rows-that-are-behind).

## Only the rows that are behind

On many records, most rows are current. `--behind-only` omits each pin and each
profile version that is current. The first line still counts each one:

```text
{{#include ../output/behind-only.txt}}
```

The exit does not change. A row that strata could not check stays, because the
exit is `2` for it. A profile version that is `gone` stays too. `--json` keeps
each pin and each profile version, because `--since` reads all the pins of the
previous run. To keep the pins that are behind in a script, select them with
`.pins[] | select(.behind)`.

## A cluster with many profiles

A record of a cluster holds each profile that the cluster runs. When it holds two
or more, its title gives their count, such as `4 profiles on cluster payments`. So
the title does not say which profile holds a pack. The line `pinned by` of a pin
names the profile of the cluster that holds the pack. The line of a profile
version names that version. Here the cluster `payments` runs `aws-stack@1.0.0` and
three add-on profiles, and `--behind-only` omits the rows that are current:

```text
{{#include ../output/behind-only-cluster.txt}}
```

The record names the profile of each pack that the cluster reconciled, in the
tier `running`: see
[More of the cluster than its release](./snapshot/cluster.md#more-of-the-cluster-than-its-release). When the record holds
no tier `running`, or the tier does not name the pack, the line gives the title of
the record. A record of a cluster group holds no such tier. `--json` keeps the title
of the record in `subject`, and gives the title with the profile in `owner`:

```text
{{#include ../output/behind-owner-json.txt}}
```

## Newer versions of a profile

A pipeline publishes the versions of a profile, and the definition of a cluster
pins the version that the cluster runs. `behind` names the newer versions of each
profile version that a record holds:

- A record of a profile version holds that version.
- A record of a cluster, an edge host, or a cluster group holds each profile that
  it runs.
- A candidate, a pack record, and a record of an app profile hold none.

Here the pipeline published `aws-stack` 1.1.0 and the pre-release 1.2.0-rc.1 in
the project `dev`, after the record of 1.0.0. The project `prod` holds 1.3.0, and
the tenant holds 2.0.0:

```text
{{#include ../output/behind-profile-lines.txt}}
```

Exit `1`: a profile version is behind, as a pin is. The line `[profile]` names the
profile, the version that the records hold, and the scope of the version. Then it
names the newer versions, newest first. The line under it gives the time that
Palette created the newest version. Palette gives no time of the publish.

- **Palette selects the versions** of the same name in the scope of the version.
  So `behind` does not name 1.3.0 of `prod` or 2.0.0 of the tenant. See
  [The versions of a profile](../internals/palette/profiles.md#the-versions-of-a-profile).
- **Strata orders the versions,** as it orders the versions of a pin: see
  [A pre-release](#a-pre-release). Palette gives the list in no order, and no route
  says that a version is newer. See
  [A newer version of a profile](../internals/palette/profiles.md#a-newer-version-of-a-profile).
- **A version that Palette no longer holds** is `gone`. Its document answers HTTP
  404, so strata cannot read the other versions. `verify` says that the record is
  gone. The exit does not change.
- **A version that strata cannot read** is `not checked`. The line under it gives
  the error, and the exit is `2`.

`--pre-release` names the pre-release too:

```text
{{#include ../output/behind-profile-pre-release-lines.txt}}
```

The record of a cluster holds each profile that the cluster runs. The cluster
`payments` runs `aws-stack@1.0.0` and three add-on profiles:

```text
{{#include ../output/behind-profile-cluster-lines.txt}}
```

`--json` gives each profile version in `profiles[]`: see
[`strata-behind/1`](../reference/documents.md#strata-behind1).

```text
{{#include ../output/behind-profile-json.txt}}
```

Strata reads the document of each profile version one time in each project, and
the document of its newest version. Each part of `behind` reads that one copy.
The parts are the newer versions, the time of the create, the move, the carriers,
and the names of the clusters in a refusal. The comparison of the move also reads
the pack manifests and the variables of the two versions. `--no-impact` skips the
comparison, and not the other reads. With `--pin`, strata reads no profile version.

## The move to the newest profile version

For a profile version that is behind, `bump to` names what a move to the newest
version does, as for a pin. Strata compares the two versions, as
`compare <name>@<held> <name>@<newest>` does. The line gives the impact level of
the comparison, and each change flag of its packs, in the order of the legend:

```text
{{#include ../output/behind-profile-move-lines.txt}}
```

`compare` of the two versions gives the same impact level:

```text
{{#include ../output/behind-profile-move-compare.txt}}
```

`--json` gives the move in `profiles[].bump`. `bump.packs[]` holds each pack that
the move changes, adds, or removes. A pack gives its key, its status, its version
on each side, its flags, and `valuesChanged` when its values change:

```text
{{#include ../output/behind-profile-move-json.txt}}
```

- **Strata adds no rule of a change.** The level, the flags, and the packs come
  from the comparison. See [Change flags and impact](../reference/impact.md).
- **Strata does not ask Palette about the move of a profile.** The bump of a
  profile version has no `palette accepts` line. To ask Palette about the move of
  a cluster, run `strata compare cluster:<cluster> <name>@<newest> --repave`. See
  [Ask Palette if the change repaves](./compare/repave.md).
- **Strata names what the update checks.** A line names each required variable
  with no default value that the newest version adds. Palette refuses an update
  that gives such a variable no value, with `InvalidVariable`. The profile
  version 1.0.0 in [The pins that are behind](#the-pins-that-are-behind) has
  this line for `platform_owner`. Another line names a move of Kubernetes that
  skips a minor version: see
  [A move that skips a minor version](#a-move-that-skips-a-minor-version).
  `bump.updateChecks` gives both.
- **A comparison that fails** makes the profile version `not checked`. The line
  under it gives the error, and the exit is `2`.
- `--no-impact` skips the comparison, as it skips the comparison of a pin.

## The newer profile version that carries a newer pack

A pipeline moves a change up three levels. A new pack version goes to a
registry, a new profile version pins it, and the definition of a cluster pins
that profile version. For each pin that is behind, `behind` says how far the
change went. A line under the pin names the newer profile version that carries a
newer version of the pack. Here the registry published `csi-aws-ebs` 1.66.0 and
`cert-manager` 1.22.0 after the publish of `aws-stack@1.1.0`:

```text
{{#include ../output/behind-carrier-lines.txt}}
```

Read the chain from the top of each pin to its end:

1. **The pack.** The line of the pin names the newer versions of the pack in the
   registry.
2. **The profile version.** `aws-stack@1.1.0 carries csi-aws-ebs@1.65.0` names the
   newest version of the profile that pins a newer version of the pack. The version
   of the pack can be between the pinned version and the newest one, as 1.65.0 is
   here. `no newer version of aws-stack@1.0.0 carries a newer cert-manager` says
   that the change stopped at the registry.
3. **The cluster.** The lines `pinned by` name the records that pin the old
   version. When a newer profile version carries the pack, the definition of the
   cluster is the step that waits: move it to that profile version.

- **The profile versions** are the versions that a record of the pin holds: see
  [Newer versions of a profile](#newer-versions-of-a-profile). A version whose
  template holds no pack of the name of the pin, such as another profile of a
  cluster, gets no line. A version with no newer version gets no line.
- **A newer version of the pack** follows the rule of the pin: see
  [A pre-release](#a-pre-release).
- **A version that strata cannot read** gives a line that starts with
  `strata could not read which newer version`, and ends with the error. The pin
  stays behind, and the exit does not change.

`--json` gives the chain in `pins[].carriers[]`: see
[`strata-behind/1`](../reference/documents.md#strata-behind1).

```text
{{#include ../output/behind-carrier-json.txt}}
```

No route of Palette names the profile versions that pin a version of a pack. So
strata reads the template of each newer version from its document,
`GET /v1/clusterprofiles/{uid}`, as `list --pack` does. It reads only the newer
versions of a profile version that holds a pin that is behind, one request for
each. It reads them newest first, and it stops at the first version that carries
a newer version of the pack.

Strata already read the document of the newest version for the time of its
create, so that version costs no second request. `--no-impact` does not skip these
reads. With `--pin`, strata reads no profile version, so a pin has no line of a
carrier.

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

- `palette accepts`: Palette gives no error for the packs of the move. The route
  does not check the version of Kubernetes, so a line of strata can follow: see
  [A move that skips a minor version](#a-move-that-skips-a-minor-version).
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
document of each cluster record one time. When Palette refuses a move, strata
takes the names of the clusters from the document of the profile version. It
reads that document one time in each project: see
[Newer versions of a profile](#newer-versions-of-a-profile).

## A move that skips a minor version

`palette accepts` covers the packs of the move. Palette checks the version of
Kubernetes only at the update, and no validate route checks it. One update moves
the Kubernetes pack one minor version at most. So strata makes the check, and
Palette is the owner of the rule: see
[The gaps that strata fills](../internals/palette/profiles.md#the-checks-of-an-update).
Here the record pins `kubernetes` 1.34.9, and the newest version is 1.36.2:

```text
{{#include ../output/behind-skip.txt}}
```

The line under the answer of Palette says that the move skips a minor version.
Palette refuses the update with `K8sUpgradeNotSupported`, HTTP 400. The line
names the next step: the newest version of the next minor version in the
registry, here 1.35.6. When the registry has no such version, the line ends with
`move to a version of 1.35 first`.

- **The rule holds for each pack of the `k8s` layer**, on each cloud, such as
  `kubernetes-eks`. A move of more than one minor version skips. A move to a
  higher major version skips too.
- **Palette compares the version that each cluster runs** with the version of
  the move. Strata compares the pinned version.
- **A version in which Palette reads no version**, such as `1.29.x`, gets no
  check, as in Palette.

`--json` gives the skip in `bump.updateChecks.skippedMinor`:

```text
{{#include ../output/behind-skip-json.txt}}
```

The check sends no request. `--no-impact` skips it with the bump. See
[What the update checks](../internals/palette/validate-routes.md#what-the-update-checks)
for the rule, and [validate-packs.sh](../development/live-runs.md#validate-packssh)
for the live check.

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
| `0` | No pin and no profile version is behind. |
| `1` | A pin or a profile version is behind. |
| `2` | Strata could not check a pin or a profile version, or could not read a record. |

- The records are record files, or quoted patterns such as
  `'releases/**/*.json'`: strata expands them. A record gives the pins of its
  packs. The record can be of a profile, a cluster, an edge host, a cluster
  group, an app profile, or a pack.
- Strata connects one time for each project of the pins and of the profile
  versions. A pin or a profile version is in the project of the first record that
  holds it. For each project, strata reads each listing of the registries one time.
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

`--project <uid>` or `--project-name <name>` gives the project of a record from
tenant scope, which names no project. Strata reads a record that names a project
in that project. `--project ''` is tenant scope. See
[The project](../configure.md#the-project).

`--format`, `--json`, `--compact`, `--color`, `--no-color`, and `-q`
(`--quiet`) are the output options that each report command takes: see
[Output formats](./index.md#output-formats).
