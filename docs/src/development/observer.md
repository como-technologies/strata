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

## Run it once

```bash
strata-observer once --cluster-uid UID --records DIR --work DIR --state DIR
```

`once` runs one content cycle and one versions cycle. It prints the summary as
JSON on standard output, and the log on standard error. It prints the summary
also when a cycle failed.

| Exit | Meaning |
| --- | --- |
| `0` | No finding exists, and the two cycles ended well. |
| `1` | A finding exists, as with `strata verify`. The other cycle can have failed: `failedRuns` says so. |
| `2` | No finding exists and a cycle failed, or a flag is wrong. |

| Flag | Default | What it does |
| --- | --- | --- |
| `--strata` | `strata`, on `PATH` | Names the binary. The session image has `/usr/local/bin/strata`. |
| `--cluster-uid` | required | Names the cluster. The name comes from the read of the cluster. |
| `--records` | required | The directory of `*.json.gz` records and `meta.json`. |
| `--work` | required | A volume in memory for the files of a cycle. |
| `--state` | required | A directory for `state.json`. |
| `--content-every` | `15m` | The period of the content cycle. |
| `--versions-every` | `6h` | The period of the versions cycle. |

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
  a file.
- The observer reads the directory of the records again at each cycle,
  because the kubelet updates a mounted Secret.

### A failed cycle

A content cycle fails when each row of its `verify` has an `error`, or when the
`snapshot` of the cluster fails. A versions cycle fails when `behind` fails, or
when each pin has an `error`. A row with an `error` beside good rows is a
cause in [Not checked](#not-checked), and not a failed cycle.

A failed cycle keeps the last good result. It adds 1 to `failedRuns` and keeps
the kind and the status of the error. It never keeps the message, because the
message of the kind `api` can hold the body of an answer from Palette. A good
cycle sets `failedRuns` to 0.

A `snapshot` writes a plain line and no JSON, so its kind is `unknown`. The
kind of the failed cycle then comes from `records[].error` of the `verify`, and
from `pins[].error` of the `behind`.

## The records

`--records` holds one `<name>.json.gz` file for each profile, and `meta.json`.

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
| `read-error` | `records[].error` is set, or a record file cannot be read. | Record |
| `pin-error` | `pin.error` is set. | Pin |

`counts.packs` is the sum of the packs of the records that the cluster runs. A
profile with no record adds no pack. `counts.packsInFull` is `counts.packs`
minus the packs that a cause names. All the packs of a record that has a
`read-error` are named.

## The newer versions

A pin with `behind` true is one row. The row holds `newest` and `listed` from
`newer[0]`, and `profiles` from `pinnedBy[].subject`. The observer compares the
pinned version with the newest version. It keeps `impact`, the flags, the
counts of images and charts, and the three counts of `fold`. `new` is true when
`listed` is in the last 7 days.

The cache of a move has the key `<registryUid>/<name>@<version>-><newest>` and
the version of the binary. A new binary asks again. A compare that fails is not
cached. The cache holds counts. The changed values go out of scope after the
compare, and the state never holds them.

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
| `events` | One event for each finding, the newest first. |

The status of a profile and of a pack is `drift`, `newer`, `not-checked`, `ok`,
or `no-record`. The first status that holds wins, in that order. A pack is never
`no-record`. `clusterCopy.status` is `same`, `same-content`, `differs`,
`not-on-this-cluster`, or `not-read`. `not-read` says that the side or the
compare of one profile failed while the others worked.

The pack row joins the record with the other answers. `registry` is the name in
the pin with the same `registryUid`, or the uid. `kind` is
`registryProviderType`, or `type`. `scope` is `tenant` when `source.projectUid`
is empty, and `project` otherwise. `upstream` is `hash.upstreamRaw`.
`changedPaths` names the paths of the changed values and never the values.

The limits show what a check does not cover. They count the images that a tag
names (an image with no `@sha256`), the packs of the type `helm`, and the
masked paths of each diff.

## The state

`--state/state.json` holds these facts, and no value:

- `startedAt`: the first start with this directory.
- `findings`: `firstSeen` of each finding, by `id`. A finding that is gone
  leaves the file.
- `moves`: the counts of each move.
- `content` and `versions`: `failedRuns` and `lastError`.

The observer writes the file to a temporary file and renames it. A file that it
cannot read is a log line and an empty state. So two runs of `once` with one
state directory reach `lost` when the cluster does not answer.

A log line holds a verdict, an exit code, a duration, and an error kind. It
never holds a value or the output of a process.
