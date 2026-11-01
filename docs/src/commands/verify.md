# strata verify

Make sure that each record agrees with Palette now. For each record, strata
reads the uid and the project from the file, fetches that uid again, and
compares the roots.

The examples use the project `dev`, which holds `aws-stack@1.1.1`, and the cluster
`payments` that runs it.

## Check one record

```text
{{#include ../output/edited-verify-ok.txt}}
```

One file prints the full report and a verdict line. Exit `0`.

## A record that drifted

```text
{{#include ../output/edited-verify.txt}}
```

Exit `1`. The published version changed in place: same uid, same version, other
content. [What Palette does](../internals/palette/profiles.md#edits-in-place) names who
can change it. See
[A published version is edited in place](../use-cases/edited-in-place.md).

## A record of a draft

`verify` of a record of `<ref>@draft` compares the draft with the published
version. Before the publish, it reports `DRIFT`: `published version
<name>@<version> differs from the draft recorded <time>`. After the publish, it
verifies: `published version <name>@<version> holds the draft recorded
<time>`. See
[Review a draft before its publish](../use-cases/draft.md#4-after-the-publish-the-version-holds-the-draft).

A version that nobody published has only its draft. Its record is a record of
the draft, with no `@draft` in the ref, and `verify` gives it the same verdicts.

## Who changed it

`DRIFT` says that the object changed. It does not say who changed it.
`--who` reads that from the audit log of Palette: for a record that drifted,
strata reads each write to the object of the record since the record was taken.
The object is the profile, the app profile, the cluster, the edge host, or the
cluster group. Palette has who and when. The record has what the content was.

```text
{{#include ../output/edited-verify-who.txt}}
```

Each line gives the time, the actor, the actor type, and the action, oldest
first. The actor is the name of a person, or the name of a Palette service,
such as `system`. The actor type and the action are as Palette gives them.
When Palette logs a summary of the change, the line ends with it. When a
person added a note to the entry, the line ends with `; note: <note>`. When the
log holds no write since the record, the line is
`who: no audit entry since the record`.

For a cluster record, strata reads the log by the uid of the cluster alone, with
no kind. Palette logs the create and the delete of a virtual cluster under the
kind `virtualCluster`, and not `spectrocluster`. The uid finds the entries of
both kinds.

For a cluster record, the write that overwrote a version that the cluster runs
is an entry of the profile, and not of the cluster. So strata also reads the log
of each profile in `profilesOverwritten`. A line of a write to an object other
than the object of the record names the kind and the name of that object after
the action:

```text
{{#include ../output/overwritten-cluster-who.txt}}
```

```text
{{#include ../output/edited-verify-who-json.txt}}
```

`--json` puts the same entries in `verify.who`. Each entry has `time`, `actor`,
`actorType`, `actorUid` for a person, `action`, `summary` when Palette gives
one, `userMsg` when the entry has a note, `resource` (the `kind`, the `name`,
and the `uid` of the object of the write), and `auditUid`. `auditUid` names the
entry, and `GET /v1/audits/{auditUid}` reads it again. An entry carries no
request id.

- Strata reads the log only for a record that drifted. An unchanged record
  costs no request.
- `verify-report.sh --who` gives the option to the check of each record that
  did not verify, so the issue of a drifted record lists the writes. See
  [Name who changed the record](../automation/drift-issues.md#name-who-changed-the-record).
- With many records, the line of a drifted record ends with the last write.
- Palette keeps no audit log for a pack in a registry. `--who` reads nothing
  for a pack record.
- What the log holds for each writer:
  [The audit log](../internals/palette/profiles.md#the-audit-log).

## When it changed

The document of a profile, an app profile, a cluster, an edge host, or a
cluster group gives the time of its last change,
`metadata.lastModifiedTimestamp`. So a `DRIFT` verdict ends with
`; Palette last changed it <time>`, with no more requests:

```text
{{#include ../output/edited-verify-verdict.txt}}
```

`--json` gives the time in `verify.lastModified`, for each record, whatever
its status. With many records, each entry of `records` gives it in
`lastModified`. The time is of the last change of the document. `--who` names
the writes.

A pack record gives no time. The pack listing gives the create time of a pack
in `metadata.lastModifiedTimestamp`, also when the content under the version
changes. See
[What a registry answers about a pack](../internals/palette/packs.md#what-a-registry-answers-about-a-pack).

## Check many records

Give two or more files, or a pattern in quotes. Strata expands the pattern, so
it works in any shell or scheduler:

```text
{{#include ../output/edited-bulk.txt}}
```

Each record gets one line: `OK`, `DRIFT`, or `ERROR`. A record that strata
cannot read or check is an `ERROR` line, and strata checks the other records.

```text
{{#include ../output/edited-bulk-json.txt}}
```

`--summary` prints the summary also for one file, so a script always reads one
document shape.

## The verdicts

| Verdict | Cause | Exit |
| --- | --- | --- |
| `OK: published version <v> is unchanged since <time>` | The root is the same. | `0` |
| `OK: cluster <c> running <v> is unchanged since <time>` | A cluster record: the cluster runs the recorded release. | `0` |
| `DRIFT: cluster <c> running <v> differs from the copy recorded <time>` | A cluster record: the cluster runs other content now. | `1` |
| `DRIFT: published version <v> differs from the copy recorded <time>` | An edit in place. | `1` |
| `DRIFT: published version <v> now names <v2>: the version recorded <time> was overwritten in place` | The uid holds another version now. | `1` |
| `DRIFT: cluster <c> runs <v>, but uid <uid> now names <version>: the version was overwritten in place` | A cluster record: its profile uid holds another version now. | `1` |
| `DRIFT: cluster <c> runs <v>, but uid <uid> no longer exists` | A cluster record: its profile uid is gone. | `1` |
| `DRIFT: published version <v> was renamed to <name>@<v>` | The uid holds the same version under another name. | `1` |
| `DRIFT: published version <v> no longer exists: uid <uid> is gone from Palette since the record of <time>` | A record of a version that was deleted, such as by a retire. Each pack of the record is `REMOVED`. | `1` |
| `DRIFT: cluster <c> no longer exists: deleted <time>` | A cluster record: Palette deleted the cluster, and keeps its document with the state `Deleted`. Each pack of the record is `REMOVED`. | `1` |
| `DRIFT: edge host <h> no longer exists: uid <uid> is gone from Palette since the record of <time>` | A record of an edge host that Palette no longer has. A cluster group, or a cluster whose document is gone, gets the same verdict. | `1` |
| `OK: pack <name>@<version> in registry <r> is unchanged since <time>` | A pack record: the registry holds the recorded copy. | `0` |
| `DRIFT: pack <name>@<version> in registry <r> differs from the copy recorded <time>` | A pack record: the registry holds other content now. | `1` |
| `DRIFT: <subject> has the content recorded <time>, but macro <m> has another value now` | A record of a version, a cluster, or a cluster group with the digests of its macro values (`snapshot --macros`): Palette gives a macro another value now. The root is the same. | `1` |
| `DRIFT: <subject> has the content recorded <time>, but pack <key> names image <b> now, not <a>` | A pack names another machine image in `imageId` now. The root is the same. See [The OS pack names a new machine image](../use-cases/os-image.md). | `1` |
| `INVALID: Palette marks pack <key> invalid now (<reason>)`, under the verdict | Palette marks a pack of the version invalid now: the registry deleted its tag. It does not change the verdict. See [A registry deletes the tag of a pack](../use-cases/invalid-pack.md). | the exit code of the verdict |
| `UNVERIFIED: <subject> has the content recorded <time>, but pack <key> has no upstream digest, so a change of the archive does not show; --allow-no-digest accepts it` | The content is the same, and a pack from a registry has no digest: an equal root does not show an equal archive for it. Drift wins over this verdict. | `1` |

A verdict of a cluster record names `cluster <c> running <v>`. A verdict of an
edge host record names `<v> on edge host <h>`. A verdict of a cluster group
record names `<v> pinned by cluster group <g>`. A verdict of a pending record
names `<v> pending for cluster <c>`, as the heading of `compare` does. A verdict
of a cluster that applies no profile names `cluster <c>`.

`--allow-no-digest` accepts a pack with no upstream digest: the verdict is `OK`,
the exit is `0`, and the report still names the pack in a `note`. A manifest
pack has no archive, so it never makes a record `UNVERIFIED`. See
[The content digest](../internals/hashes.md#the-content-digest).

A pack whose values hold `********` gets a `note` and a line `masked: <path>`
under the pack. The note does not change the verdict or the exit. See
[A secret changes in a pack value](../use-cases/masked-value.md).

Each verdict of a cluster record ends with `; N variable value changed` when the
cluster gives a variable another value than the record holds.

A verdict of a cluster, edge host, or cluster group record ends with the state
of the object when the state is not `Running` for a cluster, `in-use` for an
edge host, or active for a cluster group: `; the cluster is Deleting`, `; the
edge host is ready`, or `; the cluster group is inactive`. The state does not
change the verdict or the exit. With many records, the line ends with `; the
state is <state>`.

A record whose packs are in another order now is `DRIFT`, with `no packs
changed, the packs in another order` in the summary line. The order is content:
see
[What Palette does](../internals/palette/clusters.md#the-order-of-the-profiles-of-a-cluster).

A renamed tag in place is `DRIFT` too, with the result `SAME CONTENT` and no
node impact. It changes no build, but it edits a published version. See
[One pack build under two tag names](../use-cases/tag-name.md).

For a cluster record, strata reads each profile that the cluster runs, one
request for each. See [A version bump overwrites a version](../use-cases/overwritten.md).

## The sync of the registry of a pack record

Palette keeps a copy of each registry. A sync job reads the registry and
refreshes the copy. `verify` of a pack record compares the record with that
copy, and not with what the registry serves now. So the verdict of a pack
record ends with the last sync of the registry:

```text
{{#include ../output/pack-verify.txt}}
```

- `; the last complete sync of the registry started <time>`: the last sync
  completed. The time is when that sync started, not when it completed.
- `; a sync of the registry is in progress since <time>, and the last complete
  sync of the registry started <time>`: a sync runs now. The copy is the copy
  of the last complete sync until the sync that runs completes.
- `; the last complete sync of the registry started <time>; the last sync of
  the registry failed: <message>`: the last sync failed, and the message is
  from Palette. The copy is the copy of the last complete sync, so `OK` says
  only that the copy did not change.
- `; the recovery of the sync is <state>: <message>`: a sync stopped before
  its end, and Palette recovers it. The state is `Interrupted` or
  `Recovering`. This note can follow each of the notes above.
- No note: the registry does not sync, or Palette does not give the sync status.

A sync can stay in progress for a long time. Then the copy can be much older
than the time of the sync that runs. See
[The sync of a registry](../internals/palette/packs.md#the-sync-of-a-registry).

The sync status is a note beside the verdict. When Palette answers the sync
request with a 5xx, or the request does not complete, strata writes a note to
stderr and gives the verdict with no sync note. The status and the exit of the
verdict do not change: an unchanged pack is `OK` and exits `0`.

With many records, the line of a pack record ends with the same note. `behind`
ends the line of a pin that is current with the same note, when the last sync
of the registry did not complete. See
[A pin that is current against an older copy](./behind.md#a-pin-that-is-current-against-an-older-copy).

```text
{{#include ../output/pack-verify-json.txt}}
```

`--json` puts the same in `verify.registrySync`:

- `lastSyncedTime`, the start of the last complete sync.
- `lastRunTime`, the start of the last sync.
- `status`: `Completed`, `InProgress`, or `Failed`.
- `message`, when Palette gives one.
- `syncRecovery`, when a recovery is not `Recovered`.

When strata cannot read the sync status, `verify.registrySync` is absent, and
`verify.registrySyncError` gives the reason, such as `HTTP 500: <body>`. Strata
sends one request for each pack record: the summary listing of the registries
of the type of the pack. See
[Other reads](../reference/palette-api.md#other-reads).

## The object of the record

A record names an object: a profile version, a cluster, an edge host, or a
cluster group. `verify` says when that object changed its state, went, or took
another name. Here the cluster `payments` goes. It still runs the recorded
copy, so the record is `OK`, and the verdict names the state:

```text
{{#include ../output/verify-state.txt}}
```

Exit `0`. After the delete, Palette keeps the document of the cluster, with the
state `Deleted` and the time of the delete. See
[What Palette does](../internals/palette/clusters.md#a-deleted-cluster):

```text
{{#include ../output/verify-deleted.txt}}
```

Exit `1`. Each pack of the record is `REMOVED`. A person renames the profile
`aws-stack` to `aws-stack-legacy`. The uid and the version stay:

```text
{{#include ../output/verify-renamed.txt}}
```

Exit `1`. A rename is not an overwrite: the uid names the same version. Over
many records, each line says the same:

```text
{{#include ../output/verify-objects-bulk.txt}}
```

## The tiers of a cluster record

`verify` of a cluster record reads the tiers that the record carries again, and
the report names each tier that changed: see
[More of the cluster than its release](snapshot/cluster.md#more-of-the-cluster-than-its-release).
The verdict follows the release alone, so a changed tier does not make a record
`DRIFT`. A record written before the tiers carries none, and verifies as before.
A record of a cluster group carries the `policy` tier of the group: see
[The config that a cluster group gives](snapshot/other-objects.md#the-config-that-a-cluster-group-gives).
A record of a later release can carry a tier that this release does not know.
Palette gives strata no such tier, so `verify` does not compare it, and a note
names it. See
[A value that strata does not know](../reference/compatibility.md#a-value-that-strata-does-not-know).

## Each kind of record

`verify` reads again what the record names, in the way that it was taken:

| Record | Read again |
| --- | --- |
| a profile version | the profile uid |
| a cluster | the cluster uid, and each profile that it runs |
| `cluster:<cluster>@pending` | the update that waits for the cluster now. Once the cluster takes it, nothing waits, and the record verifies against the copies that the cluster runs. |
| an edge host | the edge host uid, and the manifests and variables of each profile bound to it |
| a cluster group | the group uid, and the manifests and variables of each profile that it pins |
| an app profile | the app profile uid, and its tiers |
| a pack version | the registry, and the sync status of the registry |

A tier of an app profile has no digest, so `verify` of an app profile with a
helm tier is `UNVERIFIED` unless `--allow-no-digest` accepts it. See
[An app profile is updated in place](../use-cases/app-profiles.md).

A later release can record an object of a new kind. `verify` cannot read such
an object from Palette, so it refuses the record with `usage`. It sends no
request, and it needs no key. A record with no uid, such as the record of a
candidate, gets `no-uid` in the same way.

## Run it on a schedule

`scripts/ci/verify-report.sh` runs `verify` over each record, and keeps one GitHub
issue open for each record that drifted. See
[Drift issues](../automation/drift-issues.md).

## What to expect

| Exit | Meaning |
| --- | --- |
| `0` | Each record is unchanged. |
| `1` | A record drifted, or a record is unverified. |
| `2` | Strata could not check a record. |

- Strata checks each record in the project that the record names. A record from
  tenant scope has no project, so strata checks it in the configured project.
- `--project <uid>` or `--project-name <name>` checks each record in that
  project, not in the project of the record. `--project ''` is tenant scope.
- `--format markdown` writes one table for many files, and the full report for
  one file.
- `--json` writes [`strata-diff/1`](../reference/documents.md#strata-diff1) for
  one file, and [`strata-verify-summary/1`](../reference/documents.md#strata-verify-summary1)
  for many.

## Options

```text
{{#include ../output/help-verify.txt}}
```

`--format`, `--json`, `--compact`, `--color`, `--no-color`, and `-q`
(`--quiet`) are the output options that each report command takes: see
[Output formats](./index.md#output-formats).
