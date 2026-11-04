# The checks of the observer

`strata-observer` runs the `strata` binary on two timers and keeps the last
answer of each. It answers five questions:

- Does Palette still have the version that I recorded?
- Does this cluster run its release?
- Who changed it?
- Does the registry have newer versions of what we pin?
- What changed between two pack versions?

The observer holds no rule of its own. It reads the answers of `strata` through
the allow-list of `internal/docs`, and it writes nothing to Palette. For the
rules of the code, see [The Go of the observer](./code.md#the-go-of-the-observer).

`once` prints the summary one time. `serve` keeps the answers and shows them on
a loopback address: see [The API of the observer](./observer-api.md).

## Run it once

```bash
strata-observer once --cluster-uid UID --records DIR --work DIR --state DIR
```

`once` runs one content cycle and one versions cycle. It prints the summary as
JSON on standard output, and the log on standard error. It prints the summary
also when a cycle failed.

[The service page](../automation/observer.md#run-it-once) gives the exit codes and the flags, with the output of a run.

## The two cycles

The observer starts these commands, and no other. `internal/runner` refuses
any other command and any other flag.

| Cycle | Step | Command | Exit that is an answer |
| --- | --- | --- | --- |
| Content | 1 | `verify <record>... --summary --who --json --compact -q` | 0, 1, or 2 with a document |
| Content | 2 | `snapshot cluster:<uid> --raw-dir raw -o cluster.json -q` | 0 |
| Content | 3 | `snapshot cluster:<uid>/<profile> --from-raw raw -o side-<n>.json -q` | 0 |
| Content | 4 | `compare <record> side-<n>.json --json --compact -q` | 0 or 1 |
| Versions | 1 | `behind <record>... --no-impact --json --compact -q` | 0, 1, or 2 with a document |
| Versions | 2 | `compare pack:<registryUid>/<name>@<version> pack:<registryUid>/<name>@<newest> --json --compact -q` | 0 or 1 |

Steps 3 and 4 run for each record that the cluster runs. The side comes from
the raw answers of step 2, so these two steps make no request.
Step 2 of the versions cycle runs for each pin that is behind and has no cached
move.

- A cycle starts at the start of the process, and then after its period. Each
  wait has a random 0 to 10 percent added.
- One worker runs the cycles. One `strata` process runs at a time, and a run
  ends after 10 minutes.
- A cycle makes a directory under `--work` with mode `0700`. It writes the
  records, `raw/`, and the sides there. It removes the directory at the end,
  also after an error.
- A cluster side holds the clear values of the variables. So `--work` must be
  a volume in memory.
- With no record, a cycle runs no `verify` and no `behind`, because each needs
  a file. The versions cycle then reads no version, so `versions.readAt` is
  `null`.
- The observer reads the directory of the records again at each cycle,
  because the kubelet updates a mounted Secret.

### A failed cycle

A content cycle fails when each row of its `verify` has an `error`, or when the
`snapshot` of the cluster fails. A versions cycle fails when `behind` fails, or
when each pin has an `error`. A row with an `error` beside good rows is a
cause in [Not checked](#not-checked), and not a failed cycle. A failed `compare`
of a move does not fail the cycle: see [The newer
versions](#the-newer-versions). A failed read of one part of a record is not a
failed cycle: see [A failed read](#a-failed-read).

A failed cycle keeps the last good result. It adds 1 to `failedRuns` and keeps
the kind and the status of the error. It never keeps the message, because the
message of the kind `api` can hold the body of an answer from Palette. A good
cycle sets `failedRuns` to 0.

A `snapshot` writes a plain line and no JSON, so its kind is `unknown`. The
kind of the failed cycle then comes from `records[].error` of the `verify`, and
from `pins[].error` of the `behind`.

### A failed read

One failed read of one part of one record is not a failed cycle. It is a
`verify` row with an `error`, or a side of the cluster that `snapshot` or
`compare` could not read. Such a read tells nothing about the drift.

- The observer takes the last good answer of that part from its memory. It
  puts the answer in the new result, so the finding stays with its first
  sight.
- The record has the cause `read-error` in [Not checked](#not-checked), also
  when it holds the last good answer.
- Only a read that works clears a finding.
- A new process has no last answer in memory. The state still keeps the first
  sight of each finding of that part. A later read that shows the drift keeps
  the date.
- A record that changed since the last good answer gets no old answer.

## The records

`--records` holds one `<name>.json.gz` file for each profile, and `meta.json`.
In a cluster, the volume is four Secrets that the pipeline writes with
`observer-records.sh`: see [The records](./observer-deploy.md#the-records).

| `baseline` | `meta.json` | Meaning |
| --- | --- | --- |
| `approved` | `{"baseline":"approved","revision":"..."}` | A person approved the records. |
| `first-seen` | `{"baseline":"first-seen","firstSeenFrom":"<uid>"}` | The records come from the first sight of a cluster. |
| `unknown` | No file, or a file that the observer cannot read. | The summary says `unknown`. |

A file that is not `strata-snapshot/1`, or that is a record of a cluster, is a
`read-error` row in [Not checked](#not-checked). It does not stop the others.
The observer decodes a record only into `docs.Record`, so a variable in a
record never enters the process. It writes the unzipped file to the directory
of the cycle for `strata`, and it keeps no other copy.

The cluster runs a profile at a version. The baseline of that profile is the
record with the same name and version. A profile with no such record is a
`no-record` row in [Not checked](#not-checked). A record that the cluster does
not run still gets a `verify`, but no cluster side. Its `clusterCopy` is
`not-on-this-cluster`.

## The findings

One finding is one cause, with one `id`. The `id` is the first 16 hex digits of
the SHA-256 of the kind and the profile. The severity is `impact`, as `strata`
gives it. The observer never lowers a drift.

| `kind` | When | Names |
| --- | --- | --- |
| `published-changed` | A `verify` row is `drifted` and has no `overwrittenBy`, `renamedTo`, or `gone`. | The profile, and each changed pack. |
| `overwritten` | `verify.overwrittenBy` is set. | The profile. |
| `renamed` | `verify.renamedTo` is set. | The profile. |
| `gone` | `verify.gone` is true. | The profile. |
| `cluster-differs` | The compare of the record and the cluster side has `same` false and `sameContent` false. | The profile, and each changed pack. |
| `image-id-changed` | The compare has entries in `imageIdsChanged`, also when `same` is true. | The profile, and each `key`. |

- A finding holds `who` from `verify.who[]`: `time`, `actor`, `actorType`, and
  `action`.
- When `verify.whoError` is set, strata could not read the audit log. The row
  stays `drifted`, and the finding stays a finding. It has `whoUnread` true and
  an empty `who`. The record has no `read-error` row. The summary carries the flag, and
  never the text of the error. The log of the observer holds the text.
- When `published-changed` and `cluster-differs` hit one profile, and
  `rootLive` equals `rootB` of the compare, the observer makes one finding. It
  sets `alsoOnCluster` to true.
- `firstSeen` comes from the state. `lastSeen` is the end of the last good
  content cycle.
- `updatesPending` and `repavePending` of the cluster are in the summary only
  when a finding exists. Palette shows these two flags on its own pages, so the
  observer does not repeat them.

### Same content

`same` false with `sameContent` true makes `compare` exit 1 and print `SAME
CONTENT`. A tag name or a minted pack uid differs, and the content does not.
The observer makes no finding. The profile stays `ok`, `clusterCopy.status` is
`same-content`, and the page says "Same content".

### A new minor version of strata

A new version of `strata` can move a root with no change of content. A finding
has `reBaselineNeeded` true when the minor version of `tool.version` of its
record is lower than the minor version of the binary. The finding stays. The
observer never writes a record. This is the one compare of versions in the
observer. `behind` gives the order of the pack versions.

## Not checked

The section groups what a check did not cover, by cause. It never raises a
severity.

| `cause` | When | One row for each |
| --- | --- | --- |
| `no-upstream-digest` | `diff.upstreamUnknown` names a pack, or a row has the status `unverified`. | Pack |
| `older-registry-copy` | `pin.registrySync` is present. | Registry |
| `no-record` | A profile that the cluster runs has no record. | Profile |
| `read-error` | `records[].error` is set, a side of the cluster could not be read, or a record file cannot be read. | Record |
| `pin-error` | `pin.error` is set, or the `compare` of the move of the pin failed. | Pin |

`counts.packs` is the sum of the packs of the records that the cluster runs. A
profile with no record adds no pack. `counts.packsInFull` is `counts.packs`
minus the packs that a cause names. All the packs of a record that has a
`read-error` are named. A side that was not read is not `ok`: the profile and
its packs are `not-checked`, and `counts.packsInFull` does not count them.

## The newer versions

A pin with `behind` true is one row. The row holds `newest` and `listed` from
`newer[0]`, and `profiles` from `pinnedBy[].subject`. The observer compares the
pinned version with the newest version. It keeps `impact`, the flags, the
counts of images and charts, and the three counts of `fold`. `new` is true when
`listed` is in the last 7 days.

The cache of a move has the key `<registryUid>/<name>@<version>-><newest>` and
the version of the binary. A new binary asks again. The cache holds counts. The
changed values go out of scope after the compare, and the state never holds
them.

A compare that fails is not cached, and it does not fail the cycle. The row of
the pin stays in the list with no counts and no impact, and the cause
`pin-error` names its pack. The other pins keep their compare. The next cycle
asks again for that move only.

## The page state

The observer computes `page` at each request, from the content clock. The first
row that matches wins.

| `page` | When |
| --- | --- |
| `no-records` | `--records` holds no record file. |
| `lost` | `failedRuns` is 2 or more, and the last error is `api` with the status 401 or 403, or `transport`, `not-json`, or `timeout`. |
| `stale` | No good content cycle exists and `failedRuns` is 2 or more. Or the last good content cycle is older than two periods of `--content-every`. |
| `first-load` | No content cycle has ended well. |
| `ok` | None of the rows above. |

A wrong key with records goes to `lost`. A wrong `--cluster-uid` gives good
`verify` rows. Its `snapshot` fails with the kind `unknown`, so the page is
`stale`, and `lastError` shows the kind. A fresh install goes to `no-records`.

## The summary

The summary is one JSON document. It holds no old value, no new value, and no
value of a variable.

| Field | Content |
| --- | --- |
| `cluster` | `uid`, `name`, and the two flags of the cluster when a finding exists. |
| `page`, `strataVersion` | The page state, and the version of the `strata` binary. |
| `records` | `count`, `baseline`, `revision`, `firstSeenFrom`, and `startedAt`. |
| `content`, `versions` | `checkedAt` or `readAt`, `every`, `failedRuns`, and `lastError`: the kind and the status, or `null`. |
| `counts` | The numbers at the top of the page. |
| `findings` | The findings of the table above. |
| `profiles` | Each profile: `status`, `published`, `clusterCopy`, and its packs. |
| `newer` | One row for each pin that is behind. |
| `notChecked` | One row for each cause. |
| `limits` | `imagesNamedByATag`, `helmPacks`, and `maskedPaths`. |
| `events` | The events of the last 7 days, the newest first: see [The events](#the-events). |

The status of a profile and of a pack is `drift`, `newer`, `not-checked`, `ok`,
or `no-record`. The first status that holds wins, in that order. A pack is never
`no-record`. A finding of the kind `overwritten`, `renamed`, or `gone` names no
pack. A pack of its profile that has changes is `drift`.

`clusterCopy.status` is `same`, `same-content`, `differs`,
`not-on-this-cluster`, or `not-read`. `not-read` says that the side or the
compare of one profile failed while the others worked, and that no last good
answer exists. With a last good answer, `clusterCopy` shows it: see [A failed
read](#a-failed-read).

The pack row joins the record with the other answers. `registry` is the name in
the pin with the same `registryUid`, or the uid. `kind` is
`registryProviderType`, or `type`. `scope` is `tenant` when `source.projectUid`
is empty, and `project` otherwise. `upstream` is `hash.upstreamRaw`.
`changedPaths` names the paths of the changed values and never the values.

The limits show what a check does not cover. They count the images that a tag
names (an image with no `@sha256`), the packs of the type `helm`, and the
masked paths of each diff.

### The events

The list holds the events of the last 7 days, the newest first. Each event has
a `type`, a `reason`, a `time`, an `object`, and a `message`. The `type` is the
kind of news. A newer version stays neutral, and `not-checked` never raises a
severity.

| `type` | `reason` | When | `time` |
| --- | --- | --- | --- |
| `drift` | `DriftFound`, `Overwritten`, `Renamed`, `Gone`, `ClusterDiffers`, or `ImageIDChanged`, by the `kind` of the finding | A finding exists. | `firstSeen` of the finding. |
| `info` | `ProfileUpdated` | A finding holds `who`. One event for each write. | `time` of the write. |
| `info` | `AuditUnread` | A finding has `whoUnread` true. The message is "The audit log could not be read, so this does not say who made the change." | `lastSeen` of the finding. |
| `newer` | `NewVersion` | A pin is behind. | `listed` of the newest version. |
| `not-checked` | `OldCopy` | A pin has `registrySync`. One event for each registry. | `lastSyncedTime` of the registry. |

The `object` is the profile. For `NewVersion` and `OldCopy`, it is the profiles
that the pins name, separated by commas. A finding stays in `findings` when its
event is older than 7 days.

## The state

`--state/state.json` holds these facts, and no value:

- `startedAt`: the first start with this directory.
- `findings`: `firstSeen` of each finding, by `id`. A finding that a read
  cleared leaves the file. A finding whose part could not be read stays: see
  [A failed read](#a-failed-read).
- `moves`: the counts of each move.
- `content` and `versions`: `failedRuns` and `lastError`.

The observer writes the file to a temporary file and renames it. A file that it
cannot read is a log line and an empty state. So two runs of `once` with one
state directory reach `lost` when the cluster does not answer.

A log line holds a verdict, an exit code, a duration, and an error kind. One
more line names a profile and the `verify.whoError` of its record: strata could
not read the audit log. It never holds a value or other output of a process.
