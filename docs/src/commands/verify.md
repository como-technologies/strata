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

## Who changed it

`DRIFT` says that the object changed. It does not say who changed it, or when.
`--who` reads that from the audit log of Palette: for a record that drifted,
strata reads each write to the profile, to the cluster of a cluster record, or
to the edge host of an edge host record, since the record was taken. Palette has who and when. The record has what the
content was.

```text
{{#include ../output/edited-verify-who.txt}}
```

Each line gives the time, the actor, the actor type, and the action, oldest
first. The actor is the name of a person (`user`), or the name of a Palette
service (`service`), such as `system`. When Palette logs a summary of the
change, the line ends with it. When the log holds no write since the record,
the line is `who: no audit entry since the record`.

```text
{{#include ../output/edited-verify-who-json.txt}}
```

`--json` puts the same entries in `verify.who`. Each entry has `time`, `actor`,
`actorType`, `actorUid` for a person, `action`, `summary` when Palette gives
one, and `auditUid`. Palette gives no request id: `auditUid` names the entry,
and `GET /v1/audits/{auditUid}` reads it again.

- Strata reads the log only for a record that drifted. An unchanged record
  costs no request.
- With many records, the line of a drifted record ends with the last write.
- Palette keeps no audit log for a pack in a registry. `--who` reads nothing
  for a pack record.
- What the log holds for each writer:
  [The audit log](../internals/palette/profiles.md#the-audit-log).

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
| `OK: pack <name>@<version> in registry <r> is unchanged since <time>` | A pack record: the registry holds the recorded copy. | `0` |
| `DRIFT: pack <name>@<version> in registry <r> differs from the copy recorded <time>` | A pack record: the registry holds other content now. | `1` |
| `DRIFT: cluster <c> running <v> has the content recorded <time>, but macro <m> has another value now` | A cluster record with the digests of its macro values (`snapshot --macros`): Palette gives a macro another value now. The root is the same. | `1` |
| `UNVERIFIED: <subject> has the content recorded <time>, but pack <key> has no upstream digest, so a change of the archive would not show; --allow-no-digest accepts it` | The content is the same, and a pack from a registry has no digest: an equal root does not show an equal archive for it. Drift wins over this verdict. | `1` |

`--allow-no-digest` accepts a pack with no upstream digest: the verdict is `OK`,
the exit is `0`, and the report still names the pack in a `note`. A manifest
pack has no archive, so it never makes a record `UNVERIFIED`. See
[The content digest](../internals/hashes.md#the-content-digest).

Each verdict of a cluster record ends with `; N variable value changed` when the
cluster gives a variable another value than the record holds.

A record whose packs are in another order now is `DRIFT`, with `no packs
changed, the packs in another order` in the summary line. The order is content:
see
[What Palette does](../internals/palette/clusters.md#the-order-of-the-profiles-of-a-cluster).

A renamed tag in place is `DRIFT` too, with the result `SAME CONTENT` and no
node impact. It changes no build, but it edits a published version. See
[One pack build under two tag names](../use-cases/tag-name.md).

For a cluster record, strata reads each profile that the cluster runs, one
request for each. See [A version bump overwrites a version](../use-cases/overwritten.md).

## The tiers of a cluster record

`verify` of a cluster record reads the tiers that the record carries again, and
the report names each tier that changed: see
[More of the cluster than its release](snapshot.md#more-of-the-cluster-than-its-release).
The verdict follows the release alone, so a changed tier does not make a record
`DRIFT`. A record written before the tiers carries none, and verifies as before.

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
| a pack version | the registry |

A tier of an app profile has no digest, so `verify` of an app profile with a
helm tier is `UNVERIFIED` unless `--allow-no-digest` accepts it. See
[An app profile is updated in place](../use-cases/app-profiles.md).

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
  project, not in the project of the record.
- `--format markdown` writes one table for many files, and the full report for
  one file.
- `--json` writes [`strata-diff/1`](../reference/documents.md#strata-diff1) for
  one file, and [`strata-verify-summary/1`](../reference/documents.md#strata-verify-summary1)
  for many.

## Options

```text
{{#include ../output/help-verify.txt}}
```
