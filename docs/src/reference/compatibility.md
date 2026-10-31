# Versions and compatibility

Strata has three interfaces. Each has its own version rules.

## The Rust library

The crate follows semantic versioning over the public API that `cargo doc` shows
under `strata::`. While the version is 0.x, a minor release, such as 0.4 to
0.5, can change that API. A patch release, such as 0.4.0 to 0.4.1, can add to
it. A patch release removes nothing from it, and changes nothing in it. Code
inside the crate, such as the canonical form, is not part of the API. Each
release has the tag `v<version>`. The crate builds with Rust 1.88 or later. The
crate is not on crates.io, so a dependency names the tag: see
[The library](../automation/library.md#add-the-dependency).

[Release 0.24.0](#release-0240-changes-that-need-action) lists the changes that
need action, for the command line, the documents, and the two libraries.

## The documents

Each JSON document has a schema id, such as `strata-snapshot/1`. The id changes
only when a reader of the old id reads the document wrong:

- a field is removed, renamed, or gets a new meaning, or
- a hash is calculated differently.

A new field keeps the id. Strata ignores fields that it does not know, so it
reads a record of its own id that a later release wrote. `source.projectUid` and
`source.cluster` joined `strata-snapshot/1` in this way. After 0.20.0,
`consumers` of `strata-list/1` counts the cluster templates with the clusters,
and `consumersByKind` joined it with the count of each kind.

Two values changed after 0.21.0 with no new id. In `strata-packs/1`, a version
that the listing gives no type has the `type` `spectro`, where 0.21.0 wrote
empty text. The schema now gives the field the pack types. A record of a
cluster leaves out a label whose value is not text, where 0.21.0 kept it with
empty text. The labels are outside the root, so no root changes.

After 0.22.1, the descriptions of `method` in `preflight --schema rules` come
from the library of strata. The values `GET` and `POST`, and the default
`GET`, stay. A rule file reads the same, and the id stays.

After 0.23.0, a check of Palette in a rule file can have `each`. An entry of
`checks[]` in the report of `strata preflight` has `requests[]` for such a
rule, in place of `ran` and `answer`. Both are new fields, so the ids stay.
`preflight --schema rules` and `preflight --schema report` list them.

After 0.23.0, the world that a rule reads, and `world.json` of `--out`, have the
fields `accountKind` and `routeCloud`. A rule or a custom probe can read them in
place of its own map of the engine. No field changed.

After 0.23.0, a check of Palette with `each` and `reproduce` lines of its own has
no automatic `curl` line for each request. Before, `reproduce[]` of its entry in
the report showed each request two times. The `reproduce` lines that call Palette
read `PALETTE_API`, with the header `ProjectUid`, as the automatic lines do. Some
read `PALETTE_HOST`, which nothing set.

After 0.24.0, a record of a cluster holds why strata could not read the
notifications and the repave in `source.cluster.errors`, as `updatesPending`
and `pendingRepave`. A tier holds its read errors in its own `errors`, in the
same shape. Release 0.24.0 wrote `updatesPendingError` and `pendingRepaveError`
for these two. Strata reads an older record into `errors`, so `compare`,
`verify`, and the notes give the same answer for it. The id stays, because the
root does not cover these fields. An older strata that reads a new record shows
no reason, and the record still compares. A script that reads
`updatesPendingError` or `pendingRepaveError` must read `errors` too. In the
library, `ClusterSource` has no field `updates_pending_error` or
`pending_repave_error`. Its methods `updates_pending_error()` and
`pending_repave_error()` give the same text.

After 0.24.0, the schema states the shape of each field that it left open. A
variable has its twelve fields, and `name` is required. A segment of a leaf is
a key, an integer, or an object with a `name`. The `params` of a pack are an
object of text, but for `installOrder`, which is an integer. A tier has the
keys of its `fields`. An `error` has a `kind` and a `message`. A time has
`format: date-time`. A document stays the same, so the ids stay, no root
changes, and a variable has its keys in the same order.

One case differs. A variable that Palette gives with no `name` has an empty
`name`. Before, its `name` was `null`. Its record has another `variablesHash`,
and strata does not read a record that holds `name: null`. In the library,
`Snapshot::variables` and `Candidate::variables` are `Vec<Variable>` in place
of `Vec<Value>`. `Leaf::segments` is `Vec<Segment>` in place of `Vec<Value>`.
The function `diff::group_variables` takes `&[Variable]`.

A field can also become absent, and that keeps the id as well. For most fields
absent is a value: a field that is false, empty, or zero is left out, and each
such field says so at its own definition in
[JSON documents](./documents.md). Eight fields are different, and for these
absent means unknown: `source.consumers`, `rootLive`, `maxImpact`,
`packsChanged`, `error`, `updatesPending`, `repavePending`, and the `value`,
`old`, or `new` of a sensitive variable. `updatesPending` joined
`source.cluster` in 0.10.0, and `strata-list/1` in the same release: absent
means that strata could not read the notifications, and
`errors.updatesPending` says why. `repavePending` joined `source.cluster` in
0.18.0: absent means a record of an older release, or of an object that is not
a cluster. In 0.7.0
`source.consumers` became absent on a record that no consumers request
produced, where 0.6.3 wrote `count: 0` and stated a count that nothing counted.
A reader that takes absent for zero reads such a record wrong, so this page
names each field that changed this way, with the release that changed it. The
id stays, because the root does not cover these fields and every record of the
id still compares.

One root changed in 0.18.0 with no new id. A record of an app profile now
holds the install order of each tier, and lists the tiers in that order. See
[Edge hosts, cluster groups, and app profiles](../commands/snapshot/other-objects.md).
So when the tiers of an app profile have an install order, its root differs
from the root that 0.17.0 gives. `verify` of such an older record shows a
change. An app profile whose tiers have no install order keeps its root.

Roots are comparable only within one snapshot schema id. Strata refuses a
snapshot of another id with `unsupported-schema`, which names both ids. A file
whose id names another document, or no document, is not a snapshot. The report
that `behind --since` reads follows the same rule. The JSON Schema of each
document holds its id as a `const`. So a validator that reads
`strata schema <kind>` refuses a document of another id too.

### The fields of release 0.18.0

Release 0.18.0 added the fields below to the records, and kept the id
`strata-snapshot/1`. The root covers none of them, except the two params of an
app tier. A field of a tier is a key of `fields` in that tier, in
`source.cluster.tiers`. For each field, absent or null means an older record,
or a part that strata could not read. The table tells which. `repavePending` is
in the list of eight fields above.

| Field | What absent or null means |
| --- | --- |
| `pendingRepave` | No repave waits. When `repavePending` is `true`, strata could not read the repave, and `errors.pendingRepave` says why. |
| `rbacs` and `namespaces` of `policy` | `null`: strata did not read the route. Palette answered HTTP 404 or 405, or `--from-raw` read a directory that an older version saved. Absent: Palette answered another error, and `errors` of the tier names the field. A record of strata 0.19 holds `null` in that case. |
| `resources` and `rbac` of `policy` | A record of 0.18.0 or later. Only a record of 0.17.0 or before holds them, in place of `rbacs` and `namespaces`. |
| `updateWorkerPoolsInParallel`, `controlPlaneHealthCheckTimeout`, `clusterMetaAttribute`, and `applySetting` of `policy` | A record of 0.17.0 or before. A newer record holds all four. When Palette gives no value, the value is `false` or empty text. |
| `agentUpgrades` of `policy` | A record of 0.17.0 or before, or of 0.19.0 or later. Only 0.18.0 wrote it, as `allowed` or `paused`. From 0.19.0, `spectroComponentsUpgradeForbidden` replaces it, only when the cluster has the annotation. |
| `cloudType` of `shape` | A record of 0.17.0 or before. |
| `policy` of a cluster group | A record of 0.17.0 or before. The group had no `policy` tier. |
| `invalid` and `invalidReason` of a pack | Palette does not mark the pack invalid. A record of 0.17.0 or before cannot say. |
| `masked` of a pack | No value of the pack holds the mask. Strata finds `masked` in the values when it reads a record of 0.17.0 or before. |
| `installOrder` and `containerRegistryUid` in the `params` of an app tier | Palette gives the tier none. A record of 0.17.0 or before holds neither, so a tier with an install order has another root. |

`compare` and `verify` read an older record on the fields that it holds, and a
record without a tier still verifies. See
[The policy tier](../commands/snapshot/cluster.md#the-policy-tier) and
[The shape on each cloud](../commands/snapshot/cluster.md#the-shape-on-each-cloud).

### A value that strata does not know

A later release can also add a value to a closed set, such as a new `template`,
and keep the id. Strata reads such a value with no error, and keeps its text.
Each document that strata writes from the record holds the same text. These
fields take a value of a later release:

- `tool.impl` of a record and of `strata-behind/1`.
- `source.template`, `source.cluster.kind`, the name of each tier in
  `source.cluster.tiers`, and `upstreamSource` in the `hash` of each pack, in
  `strata-snapshot/1`.
- `registryProviderType` of each pack, in `strata-snapshot/1`.
- `bump.impact` and each value of `bump.flags[]`, in `strata-behind/1`.
- `pinnedState` of a pin, and `state` of a newer version, in `strata-behind/1`.

A release does not rename or remove a value of these sets and keep the id.

The commands do this with such a value:

- `compare` compares the text. Two records of the same new `kind` compare their
  tiers, as two cluster records do. Two records that carry the same new tier
  compare its `fields`. Only two cluster records compare the values of their
  variables, so two records of a new `kind` do not.
- `verify` refuses a record whose `source.cluster.kind` it does not know, with
  `usage` and exit `2`. Strata cannot read such an object from Palette, so it
  sends no request.
- `verify` does not compare a tier that it does not know, because Palette gives
  strata no such tier. A note in the report names the tier.
- A record with a new `template` is not a draft. A pack with a new
  `upstreamSource` has a digest, so strata does not call its upstream unknown.
- `behind --since` reads the time of the previous report, and the pinned and
  the newer versions of each pin. It does not read the `bump` of a pin.

`strata schema` agrees: each of these fields takes each value of its set, or
any other text. The other closed sets of these two documents are text, so
strata reads each value of them:

- the `type` and the `layer` of a pack
- the `type` of a profile
- the `status` of a sync

Strata only writes its other documents, such as `strata-packs/1` and
`strata-diff/1`, and does not read them. So their closed sets stay closed. The
`state` of a version and of a registry in `strata-packs/1` is an exception: it
takes any text. A program can read such a document with the types of the
library, such as `ops::PackBuild`. Then a value of a later release gives an
error, except in that `state`.

## The command line

These do not change in a release, so a script can use them:

- the exit codes `0`, `1`, and `2`
- the JSON documents
- the error `kind` names: see [Errors](./errors.md)

After 0.21.0, the kinds `transport` and `not-json` give the field `method`, and
their message names the method of the request. In 0.21.0, the message said
`GET` also for a `POST`.

After 0.22.1, `world.json` of `preflight --watch --out` holds `events: []` when
Palette gives `items: null` for the events of the cluster. It held `null`
before. This is the case at the start of a build, when the cluster has no event.
A jq program that reads `.events[]` now works in that case.

After 0.22.1, `preflight cluster:<c>/<profile>` and `preflight
cluster:<c>@pending` give a `usage` error and exit `2`. The checks read the
whole cluster. These two forms failed before with a name lookup.

After 0.22.1, an entry of `evidence[]` in the report of `strata preflight`
has no `ref` when its reference is empty. It had `"ref": ""` before. The
output of `preflight --schema report` no longer lists `ref` as required. A
program that reads `ref` must read an absent `ref` as empty text.

After 0.22.1, `behind` writes its report when strata cannot connect for the
project of some pins. Each pin of that project has `error`, the run goes on, and
the exit code is `2`, as for each pin error. Before, the run could stop with no
report.

After 0.22.1, `preflight cluster:<name>` no longer takes a source of 24 hex
digits as a uid with no check. Strata tries each id as a uid first, and then as
a name, as `strata snapshot` does. A name costs one more request, a 404.

The terminal text and the markdown can change in any release. The candidate
file rejects fields that it does not know. So a file with a field from a later
release fails on an older strata with `candidate-invalid`. A new value of a
closed set, such as a new `type` of a layer, fails in the same way. A person
writes the file, so strata refuses the value and does not guess.

## Release 0.24.0: changes that need action

Release 0.24.0 is a minor release, so it can change the API of the library. This
section lists each change that can break a script, a program that reads a
document, or code that uses a library. A change that only adds a field or a
value is in the sections above.

### For a user of the command line or of a document

- **`strata-preflight`: two rules left the catalog.** `gke-pool-multiple-3` and
  `profile-cloud-mismatch` are gone. The validate route of Palette makes these
  two checks, and the rule `palette-validate` shows its answer.
  `palette-validate` is a probe of the `palette` tier. It needs the key, and it
  changes the exit code only with `--strict`. So a GKE pool with a size that is
  not a multiple of 3 now gives exit `0`. It gave exit `1`. With `--offline`, no
  rule checks the size of the pool. To stop a build for this fault, run the
  `palette` tier with the key and `--strict`.
- **`--rule` refuses an id that is not in the catalog.** The error is `usage`,
  the exit code is `2`, and strata sends no request. The message names the id
  and tells how to list the ids: `strata-preflight --rules`. Before, such an id
  selected no rule, and the run showed `OK: no check ran` with exit `0`. So
  remove `gke-pool-multiple-3` and `profile-cloud-mismatch` from your scripts. An
  id of a rule in the catalog is not an error, also when the rule does not
  apply to the engine.
- **`palette-validate`, for a profile of another cloud type.** Palette refuses
  the body with HTTP `500` and `ClusterProfilesNotSupported`. Strata now reads
  this refusal as the answer of the route, and does not send the request again.
  The verdict is `failed`, with the message of Palette. Before, the verdict was
  `error` and the exit code was `2`. Now the exit code is `0`, or `1` with
  `--strict`.
- **`instance-offering` is in the `palette` tier.** It asks Palette with one
  request. Before, it was in the `cloud` tier and ran the tool of the cloud. A
  run with `--tier cloud` does not run it now. A run with `--no-exec` and the key
  runs it.
- **`ssh-key-pair` applies to `eks` only.** It also runs only for a cluster with
  a dynamic VPC or with no subnet id. On `aws`, the rule `iaas-pod-identity`
  sends the key pair to the properties route, which refuses a key pair that the
  region lacks. A script that reads `ssh-key-pair` in the report of an `aws` run
  must read `iaas-pod-identity`.
- **Nine rules send one request for each item.** They are `template-outside`,
  `dedicated-hosts`, `vsphere-compute-resources`, `edge-host-in-use`,
  `edge-host-health`, `edge-host-arch`, `edge-host-resources`, `edge-host-nic`,
  and `edge-agent-version`. Their entries in `checks[]` have `requests[]`, and
  have no `ran` and no `answer` of their own. A program that reads `.ran` of
  these entries must read `.requests[]`. These rules now check each profile,
  pool, or host. Before, they checked the first. So `--strict` can give exit `1`
  where it gave `0`.
- **`preflight cluster:<c>` reads the variables of the cluster.** It sends one
  more request, `GET /v1/spectroclusters/{uid}/variables`. Each profile of the
  entity has the variables of the cluster. A sensitive value has the mask that
  Palette gives. `entity.json` of `--out` has the variables. An error of this
  request stops the run with exit `2`. An answer that the route is not there
  gives no variables. The key must have permission to read the variables of the
  cluster.
- **A change file merges `variables` by name.** A variable of the file replaces
  the variable with that name. A new name adds a variable. Before, the list of
  the file replaced the list. A change file cannot remove a variable now.
- **`snapshot --from-candidate --validate`, with a disabled pack.** The import
  validate route refuses the candidate with HTTP `500` and
  `DisabledPacksForbidden`. Strata now reads this as a refusal of Palette. The
  error kind is `palette-refuses`, with the code. It was `api`. The kind names
  stay, but this case gives another kind. The exit code stays `2`, and strata
  writes no record. A program that reads `error.kind` must read
  `palette-refuses` for this case.
- **`behind` asks Palette about each pin that is behind.** This applies to a run
  with no `--no-impact`. Strata sends one request for each record that pins the
  pack, in the project of the pin. It also reads the document of each cluster
  record. So a run sends more requests and takes more time. The exit code does
  not change. The key must have permission to send `POST .../validate/packs`. If
  it does not, the pin has `paletteValidationError`, and the report shows
  `palette gave no answer`.

### For a user of the library of strata

- **The five old module paths are gone.** Release 0.22 moved five modules and
  kept the old paths. Release 0.24.0 removes the old paths. Use the new path:
  - `strata::color` is now `strata::term::color`.
  - `strata::progress` is now `strata::term::progress`.
  - `strata::layer` is now `strata::content::layer`.
  - `strata::impact` is now `strata::diff::impact`.
  - `strata::version` is now `strata::reference::version`.
- `validation::PaletteRefusal` (also `diff::PaletteRefusal`) has the new field
  `clusters: Vec<String>`. A struct literal must add `clusters: Vec::new()`. A
  pattern with no `..` must add `clusters`.
- `ops::Bump` has the new fields `palette_validation:
  Option<ProfileValidation>` and `palette_validation_error: Option<String>`. A
  struct literal must add both, with `None`. Or use `Bump::of`, which sets both
  to `None`.
- `api::codes::ValidateRoute` has the new variant `ClusterCreate`. A `match`
  with no wildcard arm must add it.
- `ops::profile_validation` now gives `DisabledPacksForbidden` of the import
  validate route in the answer. It gave `Err(Error::Api)`.
- `Error::class()` now gives `Terminal` for an HTTP `5xx` with
  `ClusterProfilesNotSupported`. It gave `Transient`.

### For a user of the library of strata-preflight

- `jq::Jq::run`, `jq::Jq::first`, and `jq::Jq::render` have a fourth argument,
  `item: Option<&Value>`. It is the value of `$item`, for a check with `each`.
  Give `None` to get the behavior of 0.23.0. With `None`, a program that reads
  `$item` does not compile.
- `rule::Check::Palette` has the new field `each: Option<String>`. A struct
  literal must add `each: None`. A pattern with no `..` must add `each`.
- `probe::Outcome` has the field `ran: probe::Ran`, in place of `ran:
  Option<String>` and `answer: Option<String>`. `Ran::Nothing` is a check that
  ran no call. `Ran::Exec` has the command and its answer. `Ran::Request` has
  one `probe::Call`, and `Ran::Requests` has one for each item of `each`. A
  struct literal and each read of `ran` or `answer` must change.
- `report::CheckResult` has the new field `requests: Vec<Call>`. A struct
  literal must add `requests: vec![]`. A pattern with no `..` must add the
  field.
- `world::World::from_cluster` sends one more request, for the variables of the
  cluster. A `PaletteApi` of a test that counts the requests sees it.
