# The checks of the observer

`strata-observer` runs the `strata` binary on two timers and keeps the last
answer of each. It answers six questions:

- Does Palette still have the version that I recorded?
- Does this cluster run its release?
- Who changed it?
- Does the registry have newer versions of what we pin?
- What changed between two pack versions?
- Which digest does each tag run as on this cluster?

The observer holds no rule about a profile. It reads the answers of `strata`
through the allow-list of `internal/docs`, and it writes nothing to Palette. The
sixth question needs `--read-pods`. It comes from the Kubernetes API, not from
`strata`: see [The read of the pods](#the-read-of-the-pods). A cycle sends `GET`
requests only. One click on the page sends one `POST` to Palette, through `strata`:
see [The answers of Palette](#the-answers-of-palette).

For the rules of the code, see [The Go of the
observer](./code.md#the-go-of-the-observer).

`once` prints the summary one time. `serve` keeps the answers and shows them on
a loopback address: see [The server of the observer](./observer-api.md).

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

| Run | Step | Command | Exit that is an answer |
| --- | --- | --- | --- |
| Content | 1 | `verify <record>... --summary --who --full --json --compact -q` | 0, 1, or 2 with a document |
| Content | 2 | `snapshot cluster:<uid> --raw-dir raw -o cluster.json -q` | 0 |
| Content | 3 | `snapshot cluster:<uid>/<profile> --from-raw raw -o side-<n>.json -q` | 0 |
| Content | 4 | `compare <record> side-<n>.json --full --json --compact -q` | 0 or 1 |
| Versions | 1 | `behind <record>... --no-impact --json --compact -q` | 0, 1, or 2 with a document |
| Versions | 2 | `compare pack:<registryUid>/<name>@<version> pack:<registryUid>/<name>@<newest> --full --json --compact -q` | 0 or 1 |
| Click | 1 | `behind <record> --pin <layer>/<name> --json --compact -q` | 0, 1, or 2 with a document |

Steps 3 and 4 run for each record that the cluster runs. The side comes from
the raw answers of step 2, so these two steps make no request.
Step 2 of the versions cycle runs for each pin that is behind and has no cached
move.

`--full` adds the unified text of the changes to the document of `verify` and of
`compare`. The text stays in memory: see [The text of a
diff](./observer-api.md#the-text-of-a-diff). The click is the one row that asks
Palette. It runs for one pack, when a person clicks "Ask Palette", and never on a
timer. See [The answers of Palette](#the-answers-of-palette).

With `--read-pods`, the content cycle lists the pods before step 1: see [The read of
the pods](#the-read-of-the-pods).

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

A content cycle fails in these cases:

- The directory of the records cannot be read, or a record cannot be written to
  the directory of the cycle.
- The `verify` command fails, or each row of its answer has an `error`.
- The `snapshot` of the cluster fails.
- The cluster runs at least one record, and each side or `compare` fails.

A versions cycle fails in these cases:

- The directory of the records cannot be read, or a record cannot be written.
- The `behind` command fails, or each pin of its answer has an `error`.

In both cycles, a command fails when it exits with a code that is no answer, when
it gives no document, or when the `schema` of its document is not the expected one.
A row with an `error` beside good rows is a cause in [Not checked](#not-checked), and
not a failed cycle. A failed `compare` of a move does not fail the cycle: see [The
newer versions](#the-newer-versions). A failed read of one part of a record is not a
failed cycle: see [A failed read](#a-failed-read). A failed read of the pods is
no failed cycle either: see [A failed read of the pods](#a-failed-read-of-the-pods).

A failed cycle keeps the last good result. It adds 1 to `failedRuns` and keeps
the kind and the status of the error. It never keeps the message, because the
message of the kind `api` can hold the body of an answer from Palette. A good
cycle sets `failedRuns` to 0.

A `snapshot` writes a plain line and no JSON, so its kind is `unknown`. The
kind of the failed cycle then comes from `records[].error` of the `verify`, and
from `pins[].error` of the `behind`.

The observer reads a document only when its `schema` is the one that the
command promises:

- `verify` writes `strata-verify-summary/1`.
- `compare` writes `strata-diff/1`.
- `behind` writes `strata-behind/1`.
- `snapshot` of the cluster writes `strata-snapshot/1`.

A document with another `schema`, or with none, is a failed read of the kind
`schema`. It makes no finding, because `--strata` can name another binary. A
`compare` with such a document makes the `clusterCopy` of its profile
`not-read`, as any other failed `compare` does.

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

A record file that the observer cannot read is a failed read too. The file is a
bad gzip, or it holds another `schema`. The file gives no record in this cycle.

- When the last cycle read a record from that file, this record replaces the file,
  with both parts carried. Its findings stay with their first sights, and no clear
  goes for them. The cause `read-error` names the profile and the file.
- A file with no last record is blind. A new process has no last record, and a file
  can be bad from the first cycle. The observer then holds each first sight of the
  state that no readable record can give, and it sends no clear for it. A readable
  record still decides for its own profile.
- A later read of the file decides: the finding clears, or it keeps its date.

## The read of the pods

`--read-pods` adds a read to the start of each content cycle. `internal/kube` lists
the pods of the cluster, and `internal/model/images.go` builds the rows of the
images from the list. With no flag, the observer makes no call to Kubernetes, and
`GET /v1/images` says `off`. With the flag, the process exits 2 when
`KUBERNETES_SERVICE_HOST` or `KUBERNETES_SERVICE_PORT` is not set. The read plays no
part in `page`.

### The package `internal/kube`

`internal/kube` is the one place that calls the Kubernetes API, and it uses the
standard library only. The kubelet sets the two variables of the address, and
`--kube-dir` names the directory of the token and the CA.

- It sends one request, `GET /api/v1/pods?limit=500`. It sends no other method and
  no other path.
- It follows `continue` until the list ends, so a cluster of more than 500 pods
  takes more than one request.
- It reads the token and the CA again at each call, because the kubelet rotates the
  token. It keeps no connection between two calls.
- A request ends after 30 seconds, and the observer reads 128 MiB of one answer at
  most.
- It decodes `metadata.name` and `metadata.namespace`, the `name` and the `image` of
  `spec.containers[]` and `spec.initContainers[]`, and the `name` and the `imageID`
  of `status.containerStatuses[]` and `status.initContainerStatuses[]`. It decodes no
  `env`, no node name, and no `state`.
- It joins a container with its status by the name of the container. The observer
  matches the image of the spec. It never reads the `image` of the status, because the
  runtime rewrites it.
- The namespace and the name of a pod stay in memory, to count the pods of an image.
  They reach no answer, no log, and no state.
- A failure is an error with a kind and a status, and never a message. The answer of
  the API server can name an object. [The error of a
  read](../reference/observer-api.md#the-error-of-a-read) lists the kinds.

### A failed read of the pods

A failed read is not a failed cycle, because the pods are not the content. The cycle
continues with its commands.

- The content gets the cause `pods-not-read`. Its row has no profile and no pack, and
  no status of a profile or a pack changes.
- The content keeps the containers and the time of the last good read, as for any
  failed read: see [A failed read](#a-failed-read). `readAt` of the document is the
  time of that read, and `error` holds the kind of the failure.
- A read that never ended well gives the state `not-read` and no row.
- The state keeps the first sight of the image findings of each record, because a
  failed read tells nothing about the drift.
- The log has one line `pods` for each read. It holds a verdict and a duration. A failed
  read adds the kind and the status. It holds no pod.

### The images

A row is one image reference, as the spec of a pod names it. The observer joins it
with the packs of the records that the cluster runs.

- **The match.** The exact text of an image in a pack wins. If there is none, the
  observer compares the name and the tag. It removes `docker.io/library/`,
  `docker.io/`, and the part after `@` first. A host other than Docker Hub stays in
  the name, so a mirror string matches a pack only when the pack names the same
  string. `match` is `none` when no pack matches.
- **The digest of a container.** An `imageID` that holds `@sha256:` gives the digest
  after the `@`. An `imageID` with no such part is `no-repo-digest`: a local image has
  an id and no repository digest. An empty `imageID` is `not-started`.
- **The first digest.** The state keeps it, for each reference, from the first sight.
  When two digests run at that sight, the digest of more pods wins, and the smaller
  text wins a tie. A read that failed adds none.
- **A real digest** is a digest that a repository holds. `no-repo-digest` and
  `not-started` are no real digest. They read "Not checked", they never read "OK", and
  they make no finding.

The status of an image comes from its real digests. The first row that holds wins:

| `status` | When |
| --- | --- |
| `split` | Two real digests or more run at one time. |
| `moved` | One real digest runs, and it differs from the first digest. |
| `steady` | One real digest runs, and it is the first digest. |
| `not-checked` | No real digest runs. |

A real digest is `first-digest` when it is the first digest, or when the state holds
no first digest for the reference. Another real digest is `another-digest`. `moved`
clears when the digest comes back, or at a new revision of the records.

An image makes a finding when its `match` is not `none`, its reference names a tag,
and its status is `moved` or `split`. A reference that names a digest cannot move, so
`byDigest` is true and no finding exists. An image that no pack names, such as an
agent of Palette, shows in the list and makes no finding.

## The records

`--records` holds one `<name>_<version>.json.gz` file for each profile version, and
`meta.json`.
In a cluster, the volume is four Secrets that the pipeline writes with
`observer-records.sh`: see [The records](./observer-deploy.md#the-records).

| `baseline` | `meta.json` | Meaning |
| --- | --- | --- |
| `approved` | `{"baseline":"approved","revision":"..."}` | A person approved the records. |
| `first-seen` | `{"baseline":"first-seen","firstSeenFrom":"<uid>"}` | The records come from the first sight of a cluster. |
| `unknown` | No file, or a file that the observer cannot read. | The summary says `unknown`. |

A file that is not `strata-snapshot/1`, or that is a record of a cluster, is a
`read-error` row in [Not checked](#not-checked). It does not stop the others.
The observer decodes a record only into `docs.Record`. The bytes of a variable
in a record can reach memory, but they reach no type of the observer. It writes
the unzipped file to the directory of the cycle for `strata`, and it keeps no
other copy.

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
| `image-moved` | An image matches a pack, its reference names a tag, and its status is `moved`. | The profile, each pack that the image matches, and each image reference. |
| `image-split` | The same, with the status `split`. | The same. |

- A finding holds `who` from `verify.who[]`: `time`, `actor`, `actorType`, and
  `action`.
- When `verify.whoError` is set, strata could not read the audit log. The row
  stays `drifted`, and the finding stays a finding. It has `whoUnread` true and
  an empty `who`. The record has no `read-error` row. The summary carries the flag, and
  never the text of the error. The log of the observer holds the status only.
- When `published-changed` and `cluster-differs` name one profile, and
  `rootLive` equals `rootB` of the compare, the observer makes one finding. It
  sets `alsoOnCluster` to true.
- `firstSeen` comes from the state. `lastSeen` is the end of the last good
  content cycle.
- `updatesPending` and `repavePending` of the cluster are in the summary only
  when a finding exists. Palette shows these two flags on its own pages, so the
  observer does not repeat them.
- A finding of the pods comes from [the images](#the-images). It holds the new field
  `images`, with the references. It has one finding for each kind and profile. Its
  `impact` is empty, because no `strata` command gives an impact for a digest. Its
  `reBaselineNeeded` is false, because no record moves.
- A finding of the pods sets the status of no profile and of no pack. `counts.drift`
  does not count it. `counts.findings` counts it, so the list "Needs attention" and
  the count in the nav hold it. `once` exits 1 when any finding exists.

### Same content

`same` false with `sameContent` true makes `compare` exit 1 and print `SAME
CONTENT`. A tag name or a minted pack uid differs, and the content does not.
The observer makes no finding. The profile stays `ok`, `clusterCopy.status` is
`same-content`, and the page says "Same content".

### A new minor version of strata

A new version of `strata` can move a root with no change of content. A finding
has `reBaselineNeeded` true when `tool.version` of its record is lower than the
version of the binary in the major number, or in the minor number with the same
major number. The patch number plays no part. The finding stays. The observer
never writes a record. This is the one compare of versions in the observer.
`behind` gives the order of the pack versions.

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
| `pods-not-read` | The read of the pods failed in the newest content cycle. | The cluster: one row, with no profile, no pack, and no registry |

`counts.packs` is the sum of the packs of the records that the cluster runs. A
profile with no record adds no pack. `counts.packsInFull` is `counts.packs`
minus the packs that a cause names. All the packs of a record that has a
`read-error` are named. A side that was not read is not `ok`: the profile and
its packs are `not-checked`, and `counts.packsInFull` does not count them.

## The newer versions

A pin with `behind` true is one row. The row holds `newest` and `listed` from
`newer[0]`, and `profiles` from `pinnedBy[].subject`. The observer compares the
pinned version with the newest version. It keeps `impact`, the words of strata
for the impact in `impactText`, the flags, the counts of images and charts, and the
three counts of `fold`. `new` is true when `listed` is in the last 7 days. `steps`
lists each version in `newer[]`, from the oldest to the newest, with its `listed`
time.

A move of an OS pack can change no value, manifest, image, or chart: the version
and the upstream digest change, and the image of the nodes with them. The counts are
then all 0, so the card of the move says that the content is the same, and it gives
`impactText`.

The cache of a move has the key `<registryUid>/<name>@<version>-><newest>` and
the version of the binary. A new binary asks again. The cache in the state holds
counts. The memory holds the changed values and the text of the diff of each move,
and the state never holds them.

A compare that fails is not cached, and it does not fail the cycle. The row of
the pin stays in the list with no counts and no impact, and the cause
`pin-error` names its pack. The other pins keep their compare. The next cycle
asks again for that move only. The move still counts as used in that cycle, so the
answer of Palette about it stays: see [The answers of Palette](#the-answers-of-palette).

## The answers of Palette

A click on "Ask Palette" asks Palette about the move of one pack to its newest
version. [The click](./observer-api.md#the-click) tells how the server takes it. The
move must be one that the last good versions cycle found for the profile.

The click runs `behind` on the record of the profile with `--pin`, and reads the
`bump` of the pin in the document. An answer holds the `profile`, the `key`, the
`newest` version, the `verdict`, the `refusals`, and the `status`. The click waits for
the slot of the worker. A versions cycle can run during the wait, so the click reads the
move again when it has the slot.

| `verdict` | When |
| --- | --- |
| `no-error` | `bump.paletteValidation` is there, and its `accepts` is true. |
| `refuses` | `accepts` is false. `refusals` holds each error: `pack`, `code`, `message`, and `clusters`. |
| `no-answer` | The pin has an `error`, `bump.paletteValidationError` is set, the document has no such pin, the document does not list the newest version of the page, or the run failed. `status` is the HTTP status of the failure, or 0. |

- A pin that failed, or that is not behind any more, has no newer version in the
  document. The answer then names the newest version of the last versions cycle, the
  version that the page shows. A `no-answer` never has an empty `newest`.
- The registry can gain a version after the last versions cycle. The document then lists
  the version of the page and a newer one. The answer is for the newer version, and the
  page shows it beside the move. The observer holds one answer for a pack of a profile.

- Palette checks the default values of the new version. It does not check the values
  of the team, so `no-error` is not "Palette takes the move". The page never reads
  "accepts".
- `no-answer` is no finding, no event, and no cause of "Not checked". It changes no
  count and no status. The summary does not change.
- The memory holds `no-error` and `refuses` until the move changes or the process
  restarts. The move changes when the pin, its newest version, or the `strata` binary
  changes. A second click sends nothing. A versions cycle drops the answer of a move
  that is not in its list. A move whose `compare` failed is in the list, so its answer
  stays.
- A `no-answer` stays in memory until the next click, which asks again.
- A refusal holds the message of Palette and the names of clusters. Only the detail
  tier serves it.

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

The summary is one JSON document. It holds no old value, no new value, no value
of a variable, no text of a diff, and no answer of Palette.

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
`registryProviderType`, or `type`. `upstream` is `hash.upstreamRaw`.
A pack row holds `fold`, the three counts of its changed values. It names no
path, because a path can hold the name of a list item, and that name is a value
of the cluster. Only the detail tier sends a path. The profile row holds `scope`:
`tenant` when `source.projectUid` is empty, and `project` otherwise.

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
| `drift` | `DriftFound`, `Overwritten`, `Renamed`, `Gone`, `ClusterDiffers`, `ImageIDChanged`, `ImageMoved`, or `ImageSplit`, by the `kind` of the finding | A finding exists. | `firstSeen` of the finding. |
| `info` | `ProfileUpdated` | A finding holds `who`. One event for each write. | `time` of the write. |
| `info` | `AuditUnread` | A finding has `whoUnread` true. The message is "The audit log could not be read, so this does not say who made the change." | `lastSeen` of the finding. |
| `newer` | `NewVersion` | A pin is behind. | `listed` of the newest version. |
| `not-checked` | `OldCopy` | A pin has `registrySync`. One event for each registry. | `lastSyncedTime` of the registry. |

The `object` is the profile. For `NewVersion` and `OldCopy`, it is the profiles
that the pins name, separated by commas. A finding stays in `findings` when its
event is older than 7 days.

The two messages of the pods are:

- `ImageMoved`: "A tag runs another digest than the first one that the observer saw."
- `ImageSplit`: "Pods run one tag with two digests."

## The alerts

[Alerts](../automation/observer.md#alerts) tells a team what the observer sends. This
section tells how the code decides. It runs only with `--alerts`. Without the flag, the
observer has no engine, and the step does nothing.

The step runs at the end of each content cycle, also after a cycle that failed. Five
rules decide what it queues:

1. **An item is a finding, a newer version, or the loss of contact.** A finding is one
   item, with the id of the finding. A newer version is one item for each profile that
   pins the pack. The page state `lost` is the item `lost`. `subject.go` is the one file
   of `internal/alert` that reads the types of `internal/model`, from `alert.SubjectsOf`.
2. **The rule of an item is the rule of the lowest level that covers it.**
   The levels run from `pack` to `cluster`. In one level, a live mute wins over a rule
   that sends. Among the rules that send, the first one in the file wins. A mute
   that has ended is not live. An item under a mute stays as it is, and queues nothing.
   A message of the item that waits for a retry goes no more. An item with no rule leaves
   the state.
3. **An item that opens under a rule `now` queues one message.** An item under a rule
   `digest` queues none, and it waits for the digest.
4. **An item that is gone queues one clear message.** It does so only when its open
   message went, its channel is still in the rules, and its rule still sends at once. A
   cycle clears an item only when it read the news of the item. A failed cycle clears
   nothing. `HeldIDs` keeps the findings of a part that the cycle could not read. A cycle
   that read no pod clears no image finding. A versions cycle that never ended well clears
   no newer version. A clear needs a check that proves it.
5. **A digest queues one message for each channel.** It does so when a period has
   passed and an item is open under a digest rule. The message lists the open items.
   A digest has no clear message. A digest that did not go is made again from the items
   that are open at the next step, for its channel only. So a channel that comes back
   gets one digest, and a channel that got its digest gets no second one in the period.

The step plans under the lock of the observer, and it sends after it releases the lock.
So a slow webhook never holds a request of the page. A channel that fails keeps its later
messages, so the order of a channel holds. The step removes the messages that went, and it
marks their items as sent. The queue keeps 100 messages at most. An item whose open
message the queue dropped queues it again at the next step.

## The state

`--state/state.json` holds these facts, and no value:

- `startedAt`: the first start with this directory.
- `findings`: `firstSeen` of each finding, by `id`. A finding that a read
  cleared leaves the file. A finding whose part could not be read stays: see
  [A failed read](#a-failed-read).
- `moves`: the counts of each move.
- `content` and `versions`: `failedRuns` and `lastError`.
- `images`: the `revision` of the records, and `first`, which maps the id of an image
  to its first digest. The id is the first 16 hex digits of the SHA-256 of `image`,
  a newline, and the reference. The file holds no reference, no pod name, and no
  namespace. A new revision of the records empties `first`. A read of `meta.json` that
  failed gives the baseline `unknown` and no revision, and it empties nothing. `first`
  holds 2048 images at most: above that, the observer drops the images that the last
  read did not show.
- `alerts`: the open alerts, `items`, by id, and the messages that did not go,
  `outbox`. It also holds `digestAt`, the time of the last digest. An item holds
  the news, the kind, the profile, the packs, one fixed sentence, the channel, and
  the time that its message went. A message of the outbox holds the channel and the
  text. The text names a profile and its packs, and no value. Without `--alerts`, the
  lists are empty.

The observer writes the file to a temporary file and renames it. A file that it
cannot read is a log line and an empty state. So two runs of `once` with one
state directory reach `lost` when the cluster does not answer. The file holds no
text of a diff and no answer of Palette. A restart drops both.

The manifests make `state` an `emptyDir`. A new pod of the observer then starts with
an empty state. It loses the first sight of each finding. It takes the first digests
again from the pods that it sees.

A log line holds a verdict, an exit code, a duration, an error kind, and a
status. One more line says that strata could not read the audit log. It names
the profile and the status of `verify.whoError`: `HTTP <status>`, or
`no answer` when the text has no status. The line for a click holds the verdict and
the status, and no profile and no pack. The line for an alert holds the name of the
channel, a verdict, and, for a send that failed, the error kind and the status. A log
line holds no output of a process. It holds no body of an answer of Palette, no
address of a webhook, and no text of a message.
