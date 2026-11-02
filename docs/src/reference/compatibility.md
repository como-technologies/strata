# Versions and compatibility

Strata has three interfaces. Each has its own version rules.

## The Rust library

The crate follows semantic versioning over the public API that `cargo doc` shows
under `strata::`. While the version is 0.x, a minor release, such as 0.4 to
0.5, can change that API. A patch release, such as 0.4.0 to 0.4.1, can add to
it. A patch release removes nothing from it, and changes nothing in it. Code
inside the crate, such as the mask of a secret, is not part of the API.

Each release has the tag `v<version>`. The crate builds with Rust 1.88 or later.
The crate is not on crates.io, so a dependency names the tag: see
[The library](../automation/library.md#add-the-dependency).

Four sections list the changes that need action:
[Release 0.27.0](#release-0270-changes-that-need-action),
[Release 0.26.0](#release-0260-changes-that-need-action),
[Release 0.25.0](#release-0250-changes-that-need-action), and
[Release 0.24.0](#release-0240-changes-that-need-action). They cover the command
line, the documents, the CI drop-ins, and the two libraries.

After 0.25.0, `error::Error` has three new variants: `StaleRecord`,
`ClusterTemplateGoverns`, and `NoCloudConfig`. A `match` on `Error` with no
wildcard arm must name them. `ops::palette_repave` and `api::Palette::cloud_config`
gave `Error::Usage` for these cases, and a caller that matched `Usage` must
match the new variants. `ops::PaletteQuestions` and `ops::check_verifiable` are
new. `ops::verify_with` makes the same check as `check_verifiable`, so its
errors do not change.

After 0.25.0, `error::Error::PackUnavailable` holds its `state` as a
`model::pack_state::PackState`, not as text. Its message and its JSON stay the same.
`ops::ListedProfile::profile_type` is an `Option<Open<ProfileType>>`, not an
`Option<String>`. `Open::as_str` gives the text. `text::plural` is public, and
it takes any type, so it picks a verb or an ending as well as a noun.
`text::schema_json` and `json::value_or_empty_object` are new.

After 0.25.0, `term::color::Paint::resolve` has a second argument,
`term::color::Stream`: `Stdout` or `Stderr`. Give `Stream::Stdout` to get the
behavior of 0.25.0. `api::codes::ValidateRoute::refuses_answer` takes the body
of the error as `&serde_json::Value`, in place of text. Parse the text one time,
and give the value.

After 0.25.0, `ops::RUNNING_STATE`, `ops::ERROR_STATE`, and `ops::DELETED_STATE`
are deprecated. They stay for one minor release. `model::cluster_state::ClusterState`
names each state, and `ClusterState::Running.is(text)` replaces
`text == RUNNING_STATE`. `ClosedSet::parse` reads the text of a state.
`closed_set::impl_closed_set` is the public macro `strata::impl_closed_set`. Its
table takes an expression for each text, and a literal is one.

After 0.25.0, `reference::version::compare` follows Semantic Versioning 2.0.0
for two versions that parse: see
[The order of versions](#the-order-of-versions). `reference::version::is_pre_release`,
`reference::version::numbers`, and `reference::version::pick` are new.
`ops::BehindOptions` has the field `pre_release`. A struct literal must set it,
or use `..Default::default()`.

After 0.25.0, `diff::Diff` and `diff::PackDiff` have the field
`version_move: Option<diff::VersionMove>`. A struct literal or a pattern with no
`..` must name it. `diff::VersionMove` is a new enum with the values `Forward`
and `Back`.

After 0.25.0, `content::canonical_json` is public. The fuzz package of strata
calls it. It gives the text that each hash covers. A change to that text moves
the hashes, so a program must not compare it between releases. Read the hashes
of a record instead.

After 0.25.0, `json::set_at` is public. It puts a value at the object keys of a
document, and it never panics. `strata-preflight` calls it.

After 0.25.0, the types of Palette are in the new module `model`. The root of the
crate holds no module of them. Each old path stays as a `pub use` for one minor
release, and release 0.27.0 removes the old paths. Use the new path:

- `strata::cloud` is now `strata::model::cloud`.
- `strata::cluster_template` is now `strata::model::cluster_template`.
- `strata::conjunction` is now `strata::model::conjunction`.
- `strata::oci_provider` is now `strata::model::oci_provider`.
- `strata::pack_state` is now `strata::model::pack_state`.
- `strata::pack_type` is now `strata::model::pack_type`.
- `strata::profile_type` is now `strata::model::profile_type`.
- `strata::repave` is now `strata::model::repave`.
- `strata::scope` is now `strata::model::scope`.
- `strata::validation` is now `strata::model::validation`.
- `strata::content::layer` is now `strata::model::layer`.

`strata::cluster_state`, which is new after 0.25.0, is `strata::model::cluster_state`
and has no old path. The module `registry` was private, and it is now
`api::registry`. It is not part of the API.

After 0.25.0, these items are no longer public, because no program outside the
crate uses them. Use the public function that reaches each one:

- `candidate::pack_snapshot`: `ops::snapshot_for` reads a `pack:` ref.
- `candidate::warn_preset_options`: `ops::snapshot_from_candidate` and
  `ops::snapshot_from_loaded` give the warning.
- `ops::app_consumers`: `ops::consumers_with` reads an app profile.
- `api::codes::REFUSAL_CODES`: `Error::class` applies the codes.
- `api::Palette::virtual_clusters`, `api::Palette::cluster_search_with_virtual`,
  and `api::SearchedClusters`: `api::Palette::virtual_cluster_search` reads the
  virtual clusters.

After 0.25.0, `ops::expand_records` and `ops::is_pattern` are public. `expand_records`
gives the record files that a command line names, and it gives `Error::Pattern` for
a pattern that no file matches. `api::connect_from_env`, `api::connect_with`, and
`api::project_uid_named` are public. They build the client that `strata` and
`strata-preflight` use, from the environment.

After 0.25.0, `Ref::parse` reads `cluster:<uid>@pending/` as the ref of
`cluster:<uid>@pending`. It read the uid `<uid>@pending`, and that ref printed as
the text of another ref. `Ref::parse` reads `tag:+` and any tag text with no label
as a uid, as it reads `tag:`. It read a set of clusters with no label.

After 0.25.0, the library of `strata-preflight` changes in these ways:

- `rule::release_in_range` orders a release that names three numbers with
  `reference::version::compare`, so `1.0.1-g0123456789ab` is below `1.0.1`. A
  bound must start with a number that parses: `>=4.x` is not a range now.
- `rule::require_release` is new. It gives `Error::Usage` for a release that does
  not parse. The command calls it for `--release`. `rule::release_in_range` still
  puts a release that does not parse in each range.
- `rule::Fix::line` and `confidence::Confidence::text` are new. They give the fix
  line and the confidence text that the three forms of the report write.
- `run::RunInputs` and `run::WatchInputs` have the field `paint: Paint` in
  place of `color: bool`. `Paint::off()` replaces `false`.
- `report::render` and `report::terminal` take `&Paint` in place of `bool`.
- `probe::LiveBackend` has private fields. `LiveBackend::new(palette, no_exec)`
  builds it. It keeps the answer of each request, and it is a `PaletteApi`, so
  `Palette::new(&backend)` reads through it.
- `report::label_for` is gone. `CheckResult::label` gives the word.
- `rule::Tier::name`, `Level::name`, `Severity::name`, and `Status::name` are
  gone. `ClosedSet::as_str` gives the same text, and `Tier::ALL` is the constant
  of the trait. `Tier::parse` is `Tier::from_input`.
- `rule::Check::kind` gives a `rule::CheckKind`, in place of text.
  `rule::Evidence::kind` is a `rule::EvidenceKind`, in place of `String`.
- `world::CLOUDS` is gone. `world::clouds()` gives the same texts in the same
  order, and `world::Engine` names each engine.
- `rule::Catalog::parse_section` compiles each program, and gives `Err` for a
  program that cannot run. `jq::Jq::check` and `jq::Jq::check_template` do the
  same compile.
- `rule::Check::Palette` holds a `rule::PaletteCheck`, with the fields `method`,
  `path`, `body`, `each`, and `expr`. `Check::Palette { .. }` becomes
  `Check::Palette(PaletteCheck { .. })` in a literal and in a pattern. The YAML
  of a rule and `preflight --schema rules` do not change.
- `world::World::to_value` gives a `Result<Value>` in place of a `Value`. It gives
  `Error::Input` when the world has a `clusterUid`, and its `entity` or the
  `metadata` of its `entity` is neither an object nor `null`. Release 0.25.0 gave
  the value. `run::run` and `run::watch` give the same error. Release 0.25.0 ran
  the checks.
- `fixture::load` gives `Error::Input` for a fixture, a base, or a `_patch` that
  is not a JSON object. It gives the same error for a world with a `clusterUid`
  and a `metadata` that is neither an object nor `null`. Release 0.25.0 gave a
  value in these cases. One case panicked: a base or a `_patch` that is not an
  object, in a file with a key such as `_note`.

After 0.26.0, the library of `strata-preflight` changes in these ways:

- `confidence::promotion` takes a third argument, `release: Option<&str>`. Give
  `None` to get the result of 0.26.0. With a release, it counts only the runs of
  that release, and a rule that does not apply to the release has no entry.
- `confidence::Ledger::counts_for` counts a run with no release for no release.
  Release 0.26.0 counted such a run for each release. A run of an older release
  is not in the counts either. The ids of these runs are in the new field
  `confidence::Counts::older`.
- `confidence::Counts` has the field `older: Vec<String>`, and
  `confidence::Confidence` has the field `older_runs: u32`.
  `confidence::PromotionReport` has the field `release: Option<String>`. A struct
  literal must set each new field.
- `confidence::Confidence::text` gives `no runs` when the counts hold no run. It
  ends with `older runs <n>` when `older_runs` is not `0`.
- A `report::CheckResult` has a `confidence` for a rule that has runs of older
  releases only. It holds no run of the release, and its `older_runs` is not `0`.

## The documents

Each JSON document has a schema id, such as `strata-snapshot/1`. The id changes
only when a reader of the old id reads the document wrong:

- a field is removed, renamed, or gets a new meaning, or
- a hash is calculated differently.

One exception is a field outside the root that only says why another field is
absent. It can move with no new id. Strata reads the older place in a record.
After 0.24.0, the read errors of a cluster moved in this way. A document that
strata only writes carries the move, and a script that reads it must change.

A new field keeps the id. Strata ignores fields that it does not know, so it
reads a record of its own id that a later release wrote. `source.projectUid` and
`source.cluster` joined `strata-snapshot/1` in this way. After 0.20.0,
`consumers` of `strata-list/1` counts the cluster templates with the clusters,
and `consumersByKind` joined it with the count of each kind.

Two values changed after 0.21.0 with no new id. In `strata-packs/1`, a version
that the listing gives no type has the `type` `spectro`, where 0.21.0 wrote
empty text. The schema now gives the field the pack types. A record of a
cluster omits a label whose value is not text, where 0.21.0 kept it with
empty text. The labels are outside the root, so no root changes.

After 0.26.0, two documents have new fields, and both keep their ids.
`confidence` of a check in `strata-preflight/1` has `olderRuns`, the number of runs
of older releases that the counts omit. `strata-preflight-promotion/1` has
`release`, and `olderRuns` in each entry of `confidence`. `olderRuns` is `0` when
the run names no `--release`. A reader that ignores unknown fields reads both.

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
same shape. Releases 0.8.0 to 0.24.0 wrote `updatesPendingError`, and releases
0.19.0 to 0.24.0 wrote `pendingRepaveError`. Strata reads an older record into
`errors`, so `compare`, `verify`, and the notes give the same answer for it. The
id stays, because the root does not cover these fields. An older strata that
reads a new record shows no reason, and the record still compares.

After 0.25.0, a read of a `Snapshot` with serde moves the two older fields into
`errors`. So `serde_json::from_str::<Snapshot>` of a record of 0.24.0 gives the
same `errors` as `Snapshot::load`. Release 0.25.0 moved them in
`Snapshot::load` alone. A `ClusterSource` that serde reads alone does not move
them, because it is not a record.

The record of a cluster is also in `strata-diff/1` and in
`strata-verify-summary/1`, so both carry the same rename. `compare --format
json` has it in `a.cluster` and `b.cluster`. `verify --summary --format json`
has it in `diff.a.cluster` and `diff.b.cluster` of a row of `records[]`. The ids
stay. Strata only writes these two documents, so it reads no older place there.

A script that reads `updatesPendingError` or `pendingRepaveError` in a record or
in one of these two documents must read `errors` too. In the library,
`ClusterSource` has no field `updates_pending_error` or `pending_repave_error`.
Its methods `updates_pending_error()` and `pending_repave_error()` give the same
text. The keys of `ClusterSource::errors` are `Open<ClusterRead>`, so a key of a
later release loads too.

After 0.24.0, the schema states the shape of each field that it left open. See
[The shape of each open field](../commands/schema.md#the-shape-of-each-open-field).
A record that strata takes from Palette stays the same. So the ids stay, no root
changes, and a variable has its keys in the same order.

After 0.25.0, two descriptions of the tier `shape` and the description of
`source.cluster.errors` have new words in `strata schema`. No key, type, or id
changes.

After 0.25.0, `strata schema` and `strata-preflight --schema` write a link of a
doc comment as a plain code span in each description. The description of the
method `GET` read ``[`PaletteApi::get_json`]``, and now reads
`` `PaletteApi::get_json` ``. No key, type, or id changes.

After 0.25.0, `type` of a profile in `strata-list/1` takes each profile type or
any other text, as `registryProviderType` does. `strata schema list` says so
with `anyOf`. The text in a document stays the same, and the id stays.

One case differs. A variable with no `name` has an empty `name`. Before, its
`name` was `null`. Palette requires a name, so only a raw directory or a
profile export that a person made gives such a variable. Its record has another
`variablesHash`, `root`, and `contentDigest`.

Strata does not read a record that holds `name: null`. `verify`, `compare`,
`behind`, and `same-release` stop on it with `snapshot-schema` and exit `2`. The
id stays. Palette requires a name, so only a raw directory that a person made
gave such a record.

After 0.25.0, the message of `snapshot-schema` names the field that does not
match, such as `variables[0].name`, and says to take the record again.
Before, it gave the serde error alone. In the library, `Error::SnapshotSchema`
has the new field `field`. It holds the path of the field, and it is empty when
the whole document does not match. A `match` with no `..` must name it.

In the library, `Snapshot::variables` and `Candidate::variables` are
`Vec<Variable>` in place of `Vec<Value>`. `Leaf::segments` is `Vec<Segment>` in
place of `Vec<Value>`. The function `diff::group_variables` takes
`&[Variable]`.

After 0.25.0, a pack of a record holds no `masked`. Release 0.25.0 and before
wrote it for a pack whose values hold a mask. Strata finds the paths in the
values, so `compare`, `verify`, and `same-release` give the same paths as before.
Strata ignores `masked` when it reads a record of 0.25.0 or before. The id stays,
because the root does not cover the field. Strata 0.25.0 that loads a new record
finds the paths in the values too.

A script that reads `packs[].masked` of a record must read `valuesCanon`:
each value that is the text `********`. In the library, `Pack` has no field
`masked`. Its method `masked_paths()` gives the same list.

After 0.24.0, strata reads the variables of a candidate file by type. Before, it
changed each value of the wrong type into the default of the field. Four cases
differ:

- A variable with no `name`, or with `name: null`, gives `candidate-invalid`.
  Before, it loaded with `name: null`.
- A flag that is not a boolean gives `candidate-invalid`, as in
  `required: "yes"`. Before, it loaded as `false`.
- A list or a map in a field of text gives `candidate-invalid`, as in
  `defaultValue: [a]`. Before, it loaded as empty text.
- A number or a flag in a field of text keeps its text: `defaultValue: 8080` is
  `8080`. Before, it was empty text. The record has another `variablesHash` and
  another root: the root of the text that Palette holds. `--create` and
  `--validate` send that text.

`format` and `inputType` are the exception to the last case. Each holds a name
of a set of Palette, so a number or a flag there is the default. A null in a
field is the default of the field: `null`, `~`, and a field with no value. This
is as before.

After 0.24.0, a param of a layer of a candidate must be text, and `installOrder`
must be an integer. Another value gives `candidate-invalid`, which names the
layer and the param. Before, the candidate took any value.

After 0.24.0, a tier of an app profile whose `installOrder` is not an integer
has no `installOrder` in its record. It sorts as a tier at 0. Before, the
record kept the value, and the tier sorted first. Palette gives an integer, so
no record changes in practice.

A field can also become absent, and that keeps the id as well. For most fields
absent is a value. A field that is false, empty, or zero is omitted, and each
such field says so at its own definition in
[JSON documents](./documents.md). Eight fields are different. For these,
absent means unknown: `source.consumers`, `rootLive`, `maxImpact`,
`packsChanged`, `error`, `updatesPending`, `repavePending`, and the `value`,
`old`, or `new` of a sensitive variable.

- `updatesPending` joined `source.cluster` in 0.8.0, and `strata-list/1` in the
  same release. Absent means that strata could not read the notifications, and
  `errors.updatesPending` says why.
- `repavePending` joined `source.cluster` in 0.18.0. Absent means a record of an
  older release, or of an object that is not a cluster.

In 0.7.0 `source.consumers` became absent on a record that no consumers request
produced, where 0.6.3 wrote `count: 0` and stated a count that nothing counted.
A reader that takes absent for zero reads such a record wrong. So this page
names each field that changed this way, with the release that changed it. The
id stays, because the root does not cover these fields and every record of the
id still compares.

One root changed in 0.18.0 with no new id. A record of an app profile now
holds the install order of each tier, and lists the tiers in that order. See
[Edge hosts, cluster groups, and app profiles](../commands/snapshot/other-objects.md).
So when the tiers of an app profile have an install order, its root differs
from the root that 0.17.0 gives. `verify` of such an older record shows a
change. An app profile whose tiers have no install order keeps its root.

One root changes after 0.25.0 with no new id. The canonical text of a string
now writes `\uXXXX` for these characters: DEL, U+0080 to U+009F, U+2028,
U+2029, U+FFFE, and U+FFFF. A YAML parser does not read them again as
themselves: see [The canonical form](../internals/hashes.md#the-canonical-form).
Each hash that covers such a character changes: `valuesHash`, `contentHash`,
`variablesHash`, and so the `root`. `verify` of an older record that holds such
a character shows a change. A record with none of them keeps each hash.

`compare`, `same-release`, and the gates show the change for two records of one
release that two releases took. [Release 0.26.0](#release-0260-changes-that-need-action)
gives the action.

One saved directory changes after 0.25.0, with no new id. Some values of a
cluster group have no canonical form, because a key is a map or a list. An
example is a template with no quotes. `RawCluster::save_dir` now writes
`********` over each password in such values. A directory that an older version
saved holds such a password in clear. The new `save_dir` gives `serialize` for
values that it cannot mask.

[Release 0.26.0](#release-0260-changes-that-need-action) says who has such a
directory, and what to do with it.

The tier of a cluster group is not part of the root, so no root changes. The
`valuesHash` of such values covers the text. So a record of a new directory has
another `valuesHash` than a record of an old directory.

After 0.26.0, each pool of the tier `shape` keeps `overrideClusterAPIConfig` and
`overrideHealthCheckConfiguration` when the pool sets them. Before, the tier
kept neither, so a change of one on a pool did not show. `strata schema` lists
both keys in `pools[]`. A pool that sets neither gives the same tier and the
same hash as before. The tiers are not part of the root, so no root changes and
the ids stay.

A cluster with a pool that sets an override has another hash of the tier
`shape` than its record from 0.18.0 to 0.26.0. `compare` of that record with
the cluster names the override in the tier `shape`. The exit stays the same,
because a tier difference does not change it. Record the cluster again to keep
the override. A record of 0.17.0 or before has no `cloudType`, so `compare`
reads the older fields alone and shows no change.

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
| `masked` of a pack | A record of 0.25.0 and before holds it, only when a value of the pack holds the mask. A new record never holds it: strata finds the paths in the values. |
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
  `source.cluster.tiers`, and the key of each entry in `source.cluster.errors`,
  in `strata-snapshot/1`.
- `upstreamSource` in the `hash` of each pack, in `strata-snapshot/1`.
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
  sends no request, and the run needs no key.
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
`strata-diff/1`, and does not read them. So their closed sets stay closed. Two
fields are exceptions, and take any text. These are the `state` of a version and
of a registry in `strata-packs/1`, and the `type` of a profile in
`strata-list/1`. A program can read such a document with the types of the
library, such as `ops::PackBuild`. Then a value of a later release gives an
error, except in those two fields.

### The note for a value that Palette gives

A later Palette release can add a value to a closed set that strata reads from Palette. Strata
keeps the text of such a value, and also writes a note. When a read of a document of Palette gives
a value that this release does not know, strata writes one note on standard error:

```text
{{#include ../output/unknown-value-note.txt}}
```

A run writes the note one time for each set and value. The set is the name that `strata schema`
gives to the definition of the set. These reads can give such a value:

- the `state` of a pack version in the pack listing
- the `providerType` of an OCI registry, and the provider type in a pack ref
- the `type` of a profile version

`--quiet` and `STRATA_QUIET` hide the note:

```text
{{#include ../output/unknown-value-quiet.txt}}
```

The note changes no output. Standard output, the exit code, and each JSON document stay the same.
A value that strata reads from a record or from a report has no note. A later release of strata
wrote it, and Palette did not give it.

#### The log of the unknown values

Strata also appends one line for each new value to the file `strata-unknown.jsonl`. The file is in
the state folder of the user: `$XDG_STATE_HOME/strata/`, or `~/.local/state/strata/` when the
variable is not set. A run adds one line for each set and value. `--quiet` hides the note and does
not stop the log.

The log needs no network. It stays on the machine, and no command sends it. A line holds only the
fields of [`strata-unknown/1`](./documents.md#strata-unknown1). It holds no name, no uid, no host,
no free text, and no value of a pack.

[`strata report`](../commands/report.md) reads the log and prints a report that you can read and
send. In a terminal, the note can end with a hint to run it. The hint shows one time in a day at
most. See [What strata can report](./report.md).

The maintainers of strata find such values in a real tenant too, with the weekly contract run. See
[What to do with a new value](../development/facts.md#what-to-do-with-a-new-value).

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

After 0.24.0, the reusable workflow `preflight.yml` has the optional inputs
`no-exec`, `rule`, `verbose`, `watch`, `every`, `limit`, and `out`. It asks for
the same permission as before, `contents: read`, so a caller that ran it before
runs it as before. The new reusable workflow `preflight-comment.yml` posts the
report as a comment, in a second job. Only that workflow asks for
`pull-requests: write`.

After 0.24.0, the input `allow` of `promote-gate.yml` is a list, separated by
spaces or new lines. A caller that gives one value runs as before.

After 0.25.0, three refusals have a kind of their own. Before, each gave
`usage`. The exit code `2` and the message stay. A script that reads
`error.kind` must read the new kinds. The fields are in
[Errors](./errors.md#the-refusals-that-a-record-or-a-cluster-causes):

- `stale-record`: `compare --repave` of a record `cluster:<c>@pending` that
  holds a profile that the cluster does not run now.
- `cluster-template-governs`: `compare --repave` of a cluster in a cluster
  template, when the change gives pack values to a profile that the cluster
  runs.
- `no-cloud-config`: `snapshot --shape` of a cluster whose document names no
  cloud config.

After 0.25.0, `compare --repave` and `compare --validate` refuse a pair that
the refs show to be wrong before any request. Strata reads a record file first,
and that read is no request. Before, strata refused after the reads of both
sides, so a run with no key gave `missing-credential`. The kind is `usage` and
the message stays. One case stays after the reads: `cluster:<a>` and
`cluster:<b>@pending` that name two clusters.

After 0.25.0, `verify` gives the error of a record that it cannot check before
it needs a key. A record of an object that a later release wrote gives `usage`.
A record with no uid, such as the record of a candidate, gives `no-uid`. Before,
a run with no key gave `missing-credential` for such a record. With several
records, the row of that record holds the error, and the other rows stay as
before.

After 0.25.0, `strata-preflight` decides the color as `strata` does. The order
is in [Colors](./formats.md#colors). An empty `NO_COLOR` no longer removes the
color: `NO_COLOR` needs a value. `CLICOLOR_FORCE` forces the color, and
`TERM=dumb` removes it under `--color auto`. The report on standard output and
the progress lines on standard error each get their own decision. `--no-color`
and `--color never` work as before.

After 0.25.0, the line `images` of `compare` writes `no charts`, not `0 charts`,
and `no images`, not `0 images`. The markdown form of the line changes in the
same way.

After 0.25.0, the markdown table of `strata` writes a line break in a cell as a
space. A line break, such as one in an error message, ended the row. The markdown
report of `strata-preflight` did this already. That report and the HTML report
now write `Palette release 4.10`. They wrote `Palette 4.10`.

After 0.25.0, `strata-preflight` compiles each program of a rule file that you
give with `--probes`, with the variables that the program has when it runs. A
rule that reads `$item` in `each`, `expr`, `when`, an `argv`, or a `reproduce`
line gave `ERROR` or `SKIP` in a run. Now the file fails with the kind
`catalog` and exit `2`. A POST with `each` and no `body` fails in the same way.
It sent the entity one time for each item. The rules in the binary are as before.

After 0.25.0, `evidence[].kind` of a rule is one of `run`, `source`, `doc`, and
`report`. A rule file with another kind fails at load. `preflight --schema rules`
and `preflight --schema report` list the four values. The report holds the same
values as before, so `strata-preflight/1` stays.

After 0.25.0, the world that a rule reads, and `world.json` of `--out`, have the
fields `edgeHosts` and `validateBody`. See
[The world document](./preflight-rules.md#the-world-document). The six rules
that read each edge host have `each: .edgeHosts`, which `--rules --json`
shows. The `reproduce` lines of the rules that call the validate route read
`.validateBody`. A rule or a custom probe can read both fields.

After 0.25.0, `pack-dependency` and `ecr-oci-images` send the body of
`palette-validate`. They sent the entity with its own name. For `cluster:UID`,
the route refuses the name of a cluster that Palette holds, and these two rules
read the refusal as no fault. Now the name has the suffix `-preflight`, and they
find the fault.

After 0.25.0, a run sends each request one time. The backend keeps the answer of
each request, by its method, its path, and its body. The four rules that ask the
validate route send one request. The six rules that read each edge host send one
request for each host. `template-outside` reads the profile that the world read.
A 5xx that is not a refusal is not kept, and the next rule asks again.

After 0.25.0, `strata-preflight` refuses a `--release` that is not a version, such
as `4.11.x`. The error kind is `usage`, the exit code is `2`, and the command
sends no request. Release 0.25.0 read the numbers at the start of the text, so
`4.11.x` was `4.11`. Give `4.11` now. [The order of
versions](#the-order-of-versions) says which text is a version.

After 0.25.0, the three forms of the report of `strata-preflight` show the fix in
one way. The fix shows for a check that failed, and for no other check. The
terminal form did this already. The markdown and the HTML form also showed the fix
of a check that was not checked or gave an error. The markdown form showed a fix
only for a check with `reproduce` lines.

Each form writes the fix as `fix: <text> (<owner>)`, with `Fix:` in the markdown.
Before, the markdown and the HTML form wrote `Fix (<owner>): <text>`. The markdown
section for the lines to run by hand is now `Reproduce by hand and fix`. It lists
each check that shows a `reproduce` line or a fix.

After 0.25.0, the three forms write the confidence as `catch <rate>, clean <rate>`.
The terminal form wrote no comma, and the HTML form put the two rates on two
lines. A check with no confidence has `no runs` in a table, and no text in the
terminal. The JSON report does not change.

After 0.25.0, `strata-preflight --records` gives the error kind `pattern` for a
pattern that no file matches or that is not valid, as `strata verify` does. It
gave `usage`, and the exit code `2` stays. The message is
`'<pattern>' matched no files`, and the error has the field `pattern`.
`strata-preflight` also finds the uid of `PALETTE_PROJECT` as `strata` does. It
warns when `PALETTE_PROJECT_UID` is set and the two name two projects. A quiet
run does neither.

After 0.26.0, `strata-preflight` counts only the runs of the release that you
name with `--release`. This changes the level of a rule on that release:

- `strata-preflight --promotion --release 4.11` counts the runs of `4.11` alone.
  A rule that no run proved on `4.11` is a probe. When the rule file says
  `level: preflight`, the advice is `demote`, and the exit code is `1`. After a
  run on `4.11` is in the ledger, the rule is a preflight check again.
- A run with no release counts for no release. Release 0.26.0 counted it for each
  release. The ledger of the lab has no release in its runs, so with `--release`
  the lab proves no rule. The rule `gke-no-release-channel` is `demote` for each
  release until the ledger holds a run of the lab with a release.
- The report of a check shows the same counts. A rule with runs of older releases
  only shows `no runs, older runs <n>`. Release 0.26.0 showed the counts of those
  runs.
- A rule whose `releases` range does not include the release has no entry in the
  promotion report.
- With no `--release`, each run counts, as in 0.26.0. The level that a rule file
  states, the exit code of a check, and `--strict` do not change.

Do these steps in a gate of CI. Write the Palette release in `release:` of each
run that you add to a ledger of your own. Add `--release` to `--promotion`, and
run it after each upgrade of Palette. [The runs of one
release](../commands/preflight/promotion.md#the-runs-of-one-release) shows the
steps.

The terminal text and the markdown can change in any release. The candidate
file rejects fields that it does not know. So a file with a field from a later
release fails on an older strata with `candidate-invalid`. A new value of a
closed set, such as a new `type` of a layer, fails in the same way. A person
writes the file, so strata refuses the value and does not guess.

### The order of versions

After 0.25.0, strata orders two versions as Palette does, by Semantic Versioning
2.0.0, when both parse. A version parses when it has one to three numbers. It can
have a `v` before them, a pre-release after `-`, and build data after `+`. A
pre-release is below its version: `1.0.1-rc.1`, `1.0.1-mr12.345`, and
`1.0.1-g0123456789ab` are below `1.0.1`. Build data has no precedence, so `1.0.1+2`
equals `1.0.1`. Before, strata put a pre-release above its version.

A string that does not parse keeps the natural order of its numbers and its text.
`22.04`, `1.30.6`, and `LTS__24.x` keep their order. A string that does not parse
is above each version that parses, and the empty string is the lowest. Three
results change:

- **`behind` names no pre-release as a newer version of a pin that is not a
  pre-release.** `1.0.1-g0123456789ab` is no newer version of the pin `1.0.1`.
  The option `--pre-release` names them. A pin that is a pre-release has its final
  version and each later version as newer versions. Before, a registry with such a
  pre-release made `behind` exit `1`. Now it exits `0` when nothing else is newer.
- **`<name>@latest` is the highest version that is not a pre-release.** A name
  that has pre-releases only gives its highest pre-release. This holds for a
  profile, an app profile, and `pack:<name>@latest`. An exact version still names
  a pre-release.
- **`strata packs` lists a pre-release below its version.** `latest` marks the
  highest version by the new order. A live check compares it with the version
  that Palette marks with `latestVersion`. `paletteLatest` stays in the document.
  `--latest` adds the version of Palette only when it is not the highest. This
  can happen for a version that does not parse.

`strata-diff/1` has the new field `versionMove`, in the document and in each
entry of `packs[]`. It is `forward` when side B has the higher version, and `back`
when side B has the lower. It is absent when a side has no version, and when the
two versions have the same precedence. The id stays, because a reader ignores a
field that it does not know. `scripts/ci/merge-gate.sh` reads `versionMove`, and
it does not sort the versions as text now. A move between a version and its
pre-release gives the other verdict: see
[Release 0.26.0](#release-0260-changes-that-need-action).

## Release 0.27.0: changes that need action

Release 0.27.0 is a minor release, so it can change the API of the library. This
section lists each change that can break code that uses a library. A change that
only adds a field, a value, an input, or an option is in the sections above.

### For a user of the library of strata

- `render::Renderer` has the new required method `render_report`. It renders
  the values that `strata report` shows, a `closed_set::unknown::Report`. Code
  outside the crate with an `impl Renderer` does not build until it has the
  method. Add `fn render_report(&self, r: &Report) -> Result<String>` to each
  such `impl`. To keep the output of strata, call `render_report` of
  `TerminalRenderer`, `MarkdownRenderer`, or `JsonRenderer`. A program that only
  calls a renderer does not change.
- `error::Error` has the new variant `LogInvalid`, with the fields `path`,
  `line`, and `message`. `ops::unknown_report` gives it for a line of the log of
  the unknown values that is not a line of `strata-unknown/1`. `Error` is not
  `#[non_exhaustive]`, so a `match` on `Error` with no wildcard arm does not
  build. Add a `_` arm, or name `Error::LogInvalid { .. }`. `Error::kind` gives
  `log-invalid` for it, and `Error::to_json` gives the same kind.

## Release 0.26.0: changes that need action

Release 0.26.0 is a minor release, so it can change the API of the library. This
section lists each change that can break a script, a program that reads a
document, a CI pipeline, or code that uses a library. A change that only adds a
field, a value, an input, or an option is in the sections above.

### First: a saved directory of a cluster group can hold a password in clear

- **Save each directory of a cluster group again, and search each repository
  that holds one.** Palette gives the group values with no mask.
  `RawCluster::save_dir` writes `********` over each password in them. The
  values are in `spec.clustersConfig.values` of `cluster.json`.
  - Releases 0.20.0 to 0.25.0 did not mask a password in values with no
    canonical form. An example is a template with no quotes,
    `host: {{ .spectro.var.host }}`. A key that is a list is another example.
  - Releases 0.11.0 to 0.19.x masked no password of the group values.
  - Only a program that calls the library saves such a directory.
    `strata snapshot --raw-dir` refuses a cluster group in each release.
- **Do these steps for each such directory:**
  1. Save the directory again with 0.26.0, or delete it.
  2. If the directory left the host, change each password that it held.
  3. If a repository holds the directory, search its history. This command
     lists each commit and file that holds the document of a cluster group:
     `git grep -c clustersConfig $(git rev-list --all -- '*cluster.json') -- '*cluster.json'`.
  4. Read the values of each file that the command lists. Change each password
     that they hold in clear.
- **`save_dir` can give `serialize` now.** Strata cannot mask some values, for
  example a password in a block scalar beside a key that is a map. Then
  `save_dir` writes no file. Release 0.25.0 saved such values with the password
  in clear. To save such values, put quotes around the template in the values
  of the group.

### For a user of the command line or of a document

- **Strata orders versions as Palette does.** It uses Semantic Versioning 2.0.0
  for two versions that parse. A pre-release is below its version, so
  `1.0.1-rc.1` is below `1.0.1`. Release 0.25.0 put it above. See
  [The order of versions](#the-order-of-versions).
  - `behind` names no pre-release as a newer version of a pin that is not a
    pre-release. So a run that gave exit `1` for such a version alone now gives
    `0`. Add `--pre-release` to name them.
  - `<name>@latest` is the highest version that is not a pre-release.
  - `strata packs` and `strata list` put a pre-release below its version.
  - Two versions are equal when they differ only by a `v`, by a missing `.0`,
    or by build data after `+`. So `1.2` equals `1.2.0`. If a profile has both
    versions, `@latest` gives `profile-name-ambiguous` and exit `2`. Give the
    uid.
  - A version that does not parse is above each version that parses. Examples
    are `1.2.3.4` and `LTS__24.x`. So `@latest` can give `1.2.3.4` in place of
    `2.0.0`, and `behind` does not name `2.0.0` as newer than the pin `1.2.3.4`.
  - A script that sorts versions as text must read `versionMove` of
    `compare --format json`. It is `forward` or `back`, in the document and in
    each entry of `packs[]`.
- **The root of a record changes when its values hold some characters.** The
  canonical text writes `\uXXXX` for each of them. So each hash that covers such
  a character changes: `valuesHash`, `contentHash`, `variablesHash`, and the
  `root`. NUL, a tab, and each other character keep their hashes.
  - The characters are DEL, U+0080 to U+009F, U+2028, U+2029, U+FFFE, and
    U+FFFF.
  - `verify` of an older record that holds such a character shows a change.
  - `compare` of an older record and a new record of one release shows
    `AUTHORED` and "the version was edited in place". It exits `1`.
  - `same-release` of the two gives `NOT THE SAME RELEASE` and exit `1`.
  - Take each such record again with 0.26.0. Take the two records of a pair
    with one release.
  - To find such a record, run this with GNU `grep` in a UTF-8 locale:
    `jq -r '.. | strings' RECORD | grep -c -P '[\x{7f}-\x{9f}\x{2028}\x{2029}\x{fffe}\x{ffff}]'`.
    A number above `0` shows such a character. A record holds no manifest
    text, so also check each manifest where you keep it.
- **A record holds no `packs[].masked`.** Release 0.25.0 and before wrote it
  for a pack whose values hold a mask. `compare`, `verify`, and `same-release`
  give the same paths as before, and no root changes. A script that reads
  `packs[].masked` of a record must read `valuesCanon`: each value that is the
  text `********`. `strata schema snapshot` no longer lists `masked`. A record
  of 0.25.0 still validates. `packs[].masked` of `strata-diff/1` stays.
- **Three refusals have a kind of their own.** Each gave `usage` before:
  - `stale-record`: `compare --repave` of a record `cluster:<c>@pending` that
    holds a profile that the cluster does not run now.
  - `cluster-template-governs`: `compare --repave` of a cluster in a cluster
    template, when the change gives pack values to a profile that the cluster
    runs.
  - `no-cloud-config`: `snapshot --shape` of a cluster whose document names no
    cloud config.

  The exit code `2` and the message stay. A script that reads `error.kind` must
  read the new kinds.
- **Some refusals come before any request.** `compare --repave` and
  `compare --validate` refuse a pair that the refs show to be wrong. `verify`
  gives `usage` or `no-uid` for a record that it cannot check. Before, a run
  with no key gave `missing-credential` for these cases. Now it gives `usage` or
  `no-uid`, with exit `2`.
- **The message of `snapshot-schema` changes.** It names the field that does
  not match, such as `variables[0].name`. It ends with "Take the record again."
  A script that compares the whole message must change.
- **`strata-preflight` decides the color as `strata` does.** An empty
  `NO_COLOR` no longer removes the color. Give `NO_COLOR` a value, such as
  `NO_COLOR=1`, or give `--no-color`.
- **`strata-preflight --probes` checks each program of the file at load.** A
  program that cannot run now gives the kind `catalog` and exit `2`. An example is an `expr` that reads `$item`: it runs over the responses,
  with no item. Another example is a program that nests more than 64 levels.
  - Release 0.25.0 ran a rule that reads `$item` where the check has no item.
    The rule gave `ERROR` or `SKIP`.
  - A POST with `each` and no `body` fails in the same way.
  - `evidence[].kind` must be `run`, `source`, `doc`, or `report`.
- **A `releases` bound must start with a number that parses.** A rule with `>=4.x` now applies to each release, also to `3.9`. A
  release with three numbers follows Semantic Versioning: `1.0.1-g0123456789ab`
  is below `1.0.1`, so it is not in `>=1.0.1`.
- **`strata-preflight --release` refuses a text that is not a version.** An
  example is `4.11.x`. The error kind is `usage`, the exit code is `2`, and the
  command sends no request. Release 0.25.0 read the numbers at the start of the
  text, so `4.11.x` was `4.11`. Give `4.11`.
- **`strata-preflight --records` gives the kind `pattern` for a pattern that
  matches no file or is not valid.** It gave `usage`. The exit code `2` stays.
  The error has the field `pattern`. A script that reads `error.kind` must read
  the new kind.
- **Two rules of `strata-preflight` can find a fault where they found none.**
  `pack-dependency` and `ecr-oci-images` send the body of `palette-validate`.
  The name has the suffix `-preflight`. For `cluster:UID`, the route refused the
  name of the cluster before, and the rules read that as no fault. So
  `--strict` can give exit `1` where it gave `0`.
- **The world of `strata-preflight` holds no `packs[].masked`.** A custom probe that reads `.profiles[].packs[].masked`, and
  a reader of `world.json` of `--out`, must read `valuesCanon`.
- **`strata-preflight cluster:<c>@pending/` gives `usage`.** Strata reads the
  ref as `cluster:<c>@pending`, as `strata` does now. Release 0.25.0 read the
  uid `<c>@pending`.

### For a user of the CI drop-ins

- **`merge-gate.sh` orders the versions as Palette does.** It reads
  `versionMove` of the comparison, and it needs no `sort -V` now.
  - A merge request that moves a pin to a pre-release of the same version, such
    as `1.21.1` to `1.21.1-rc.1`, now fails the gate with `BEHIND`. Add the
    line `Rollback: <record> to <name>@<version>` to a commit, or pin the final
    version.
  - A move from a pre-release to its final version, such as `1.21.1-rc.1` to
    `1.21.1`, passes now. Before, it needed a rollback line.
  - A move from a version that does not parse to one that parses, such as
    `1.2.3.4` to `1.3.0`, fails the gate now.
- **Run `merge-gate.sh` with the strata of the same release.** Strata 0.25.0
  writes no `versionMove`, so the script of 0.26.0 finds no backward pin with
  it. The reusable workflow and the GitLab job install both from one tag.
- **A gate can fail for records of two releases.** The gates are `comment.yml`,
  `merge-gate.yml`, and `promote-gate.yml`. Two records of one release have two
  roots when their values hold a character of the new canonical text, and two
  releases took them. See the change of the root above. Take both records with
  one release.
- **The GitLab job `strata-upstream-save` keeps the report as an artifact.** It
  runs in place of `strata-upstream` when `STRATA_SAVE` names a file. Release
  0.25.0 kept the artifact in `strata-upstream`. A `needs:` entry or a download
  that names the job for the artifact must name `strata-upstream-save`.
- **`install-strata.sh` needs cosign 3 or later to check the signature.** With
  an older cosign on the `PATH`, the installer skips the check. It says so. With `--require-signature`, it stops with exit `2` before any
  download. Install cosign 3 to keep the check. The reusable workflows install
  cosign 3, so they do not change.

### For a user of the library of strata

- `RawCluster::save_dir` writes `********` over each password of the values of
  a cluster group, also in values with no canonical form. It can give
  `Error::Serialize` and write no file. See the first part of this section.
- The types of Palette are in the module `model`. Each old path stays as a
  `pub use` until release 0.27.0. The compiler gives no warning for an old
  path, so change each path now. See [The Rust library](#the-rust-library) for
  the list, such as `strata::cloud` to `strata::model::cloud` and
  `strata::content::layer` to `strata::model::layer`.
- Seven items are no longer public. Use the public function that reaches each
  one:
  - `candidate::pack_snapshot`: `ops::snapshot_for` reads a `pack:` ref.
  - `candidate::warn_preset_options`: `ops::snapshot_from_candidate` and
    `ops::snapshot_from_loaded` give the warning.
  - `ops::app_consumers`: `ops::consumers_with` reads an app profile.
  - `api::codes::REFUSAL_CODES`: `Error::class` applies the codes.
  - `api::Palette::virtual_clusters`, `api::Palette::cluster_search_with_virtual`,
    and `api::SearchedClusters`: `api::Palette::virtual_cluster_search` reads
    the virtual clusters.
- `error::Error` has the variants `StaleRecord`, `ClusterTemplateGoverns`, and
  `NoCloudConfig`. A `match` with no wildcard arm must name them.
  `ops::palette_repave` and `api::Palette::cloud_config` gave `Error::Usage` for
  these cases. A caller that matched `Usage` must match the new variants.
- `Error::SnapshotSchema` has the field `field: String`. It is empty when the
  whole document does not match. A pattern with no `..` must name it.
- The field `state` of `Error::PackUnavailable` is a
  `model::pack_state::PackState`, in place of `String`. `ClosedSet::as_str`
  gives the text. The message and the JSON stay.
- `snapshot::Pack` has no field `masked`. The method `masked_paths()` gives the
  same list.
- `reference::version::compare` follows Semantic Versioning 2.0.0 for two
  versions that parse, as the command line does above. Code that sorts with it
  gets the new order.
- `ops::BehindOptions` has the field `pre_release`. `diff::Diff` and
  `diff::PackDiff` have the field `version_move: Option<diff::VersionMove>`. A
  struct literal must set each new field, or use `..Default::default()`. A
  pattern with no `..` must name it.
- `api::codes::ValidateRoute::refuses_answer` takes the body of the error as
  `&serde_json::Value`, in place of text. Parse the text one time, and give the
  value.
- `ops::ListedProfile::profile_type` is an `Option<Open<ProfileType>>`, in
  place of `Option<String>`. `Open::as_str` gives the text.
- `term::color::Paint::resolve` has a second argument, `term::color::Stream`.
  Give `Stream::Stdout` to get the behavior of 0.25.0.
- `ops::RUNNING_STATE`, `ops::ERROR_STATE`, and `ops::DELETED_STATE` are
  deprecated. They stay until release 0.27.0.
  `ClusterState::Running.is(text)` replaces `text == RUNNING_STATE`.
- `Ref::parse` reads `cluster:<uid>@pending/` as `cluster:<uid>@pending`. It
  reads `tag:+`, and each tag text with no label, as a uid.
- `content::canonical_json` is public. Its text can change in a release, as it
  does in this one. Do not compare it between releases. Compare the hashes of a
  record.
- A serde read of a `Snapshot` moves the error fields of a record of 0.24.0
  into `errors`, as `Snapshot::load` does. Code that moved them itself can
  stop.

### For a user of the library of strata-preflight

- `world::World::to_value` gives a `Result<Value>`, in place of a `Value`. It
  gives `Error::Input` for a world with a `clusterUid` whose `entity`, or whose
  `metadata` of the `entity`, is neither an object nor `null`. Release 0.25.0
  gave the value. `run::run` and `run::watch` give the same error. Release
  0.25.0 ran the checks.
- `fixture::load` gives `Error::Input` for a fixture, a base, or a `_patch` that
  is not a JSON object. It also gives it for the world above. Release 0.25.0
  gave a value in these cases. One case panicked: a base or a `_patch` that is
  not an object, in a file with a key such as `_note`.
- `run::RunInputs` and `run::WatchInputs` have the field `paint: Paint`, in
  place of `color: bool`. `Paint::off()` replaces `false`. `report::render` and
  `report::terminal` take `&Paint`, in place of `bool`.
- `probe::LiveBackend` has private fields. `LiveBackend::new(palette, no_exec)`
  makes it.
- `report::label_for` is gone. `CheckResult::label` gives the word.
- `rule::Tier::name`, `Level::name`, `Severity::name`, and `Status::name` are
  gone. `ClosedSet::as_str` gives the same text. `Tier::ALL` is now the constant
  of the trait. Both need `use strata::closed_set::ClosedSet`. `Tier::parse` is
  `Tier::from_input`.
- `rule::Check::kind` gives a `rule::CheckKind`, in place of text.
  `rule::Evidence::kind` is a `rule::EvidenceKind`, in place of `String`.
- `rule::Check::Palette` holds a `rule::PaletteCheck`. `Check::Palette { .. }`
  becomes `Check::Palette(PaletteCheck { .. })` in a literal and in a pattern.
- `world::CLOUDS` is gone. `world::clouds()` gives the same texts in the same
  order.
- `rule::Catalog::parse_section` compiles each program, and gives `Err` for a
  program that cannot run.
- `rule::release_in_range` orders a release with three numbers by
  `reference::version::compare`. So `1.0.1-g0123456789ab` is not in `>=1.0.1`.
  A bound that does not parse, such as `>=4.x`, is no bound: each release is in
  it. A release that does not parse, such as `4.11.x`, is in each range. Call
  `rule::require_release` first.
- `world::World::with_records` gives each record with no `packs[].masked`. A
  custom rule that reads it must read `valuesCanon`.

## Release 0.25.0: changes that need action

Release 0.25.0 is a minor release, so it can change the API of the library. This
section lists each change that can break a script, a program that reads a
document, a CI pipeline, or code that uses a library. A change that only adds a
field, a value, an input, or an option is in the sections above.

### For a user of the command line or of a document

- **A cluster record holds its read errors in `source.cluster.errors`.** The
  keys are `updatesPending` and `pendingRepave`, and each value is the HTTP
  error of Palette. The record is `strata-snapshot/1`, and its id stays.
  Releases 0.8.0 to 0.24.0 wrote `updatesPendingError`, and releases 0.19.0 to
  0.24.0 wrote `pendingRepaveError`. Strata reads an older record into `errors`,
  so `compare`, `verify`, and the notes give the same answer. No root changes.
  A script that reads `updatesPendingError` or `pendingRepaveError` must read
  `errors.updatesPending` and `errors.pendingRepave`.
- **`strata-diff/1` and `strata-verify-summary/1` carry the same rename, and
  their ids stay.** `compare --format json` has the errors in `a.cluster.errors`
  and `b.cluster.errors`. `verify --summary --format json` has them in
  `diff.a.cluster.errors` and `diff.b.cluster.errors` of a row of `records[]`.
  Strata only writes these two documents. A script that reads an older field
  there gets no value and no error.
- **Strata does not read a record with a variable that has `name: null`.**
  Palette requires a name. So only a raw directory or a profile export that a
  person made gives such a variable. Strata now gives that variable an empty
  `name`, so its record has another `variablesHash`, `root`, and
  `contentDigest`. `compare`, `verify`, `behind`, `same-release`, and the gates
  stop on an older record with `snapshot-schema` and exit `2`.
  - Take the record again.
  - Do not change `null` to `""` in the file. Its `variablesHash` and `root`
    stay as they are, so they no longer match the variables.
- **A variable of a candidate file needs a `name`, and each field needs its
  type.** `required`, `hidden`, `immutable`, and `isSensitive` are `true` or
  `false`. `options[]` is a list. A field of text is text, a number, or a flag,
  but not a list or a map.
  - Before, strata read a field of another type as its default. Now the file
    gives `candidate-invalid` and exit `2`.
  - The message names the field, such as
    `variables[0].required: invalid type: string "true", expected a boolean`.
  - A YAML `yes` or `no` is text. Write `true` or `false`. See
    [Checks before any request](./candidate-file.md#checks-before-any-request).
- **A number or a flag with no quotes in a text field of a candidate file keeps
  its text.** `defaultValue: 8080` gives the default `8080`.
  Release 0.24.0 gave empty text, and sent no default to Palette. So the record
  of such a candidate has another `variablesHash` and root, and `--create` and
  `--validate` send the default. To get the record of 0.24.0, write
  `defaultValue: ""`. A `null`, a `~`, or no value gives the default of the
  field, as before. In `format` and `inputType`, a number or a flag gives the
  default of the field.
- **A param of a layer of a candidate file must be text.** `installOrder` is the
  one exception: it must be an integer. Another value, such as `replicas: 3`,
  gives `candidate-invalid` and exit `2`. The message names the layer and the
  param. Before, the candidate took any value. Quote the number: `replicas: "3"`.
- **The schemas give the shape of each open field.** A validator that reads
  `strata schema <kind>` now refuses these:
  - a variable with no `name`
  - an `error` with no `kind` or no `message`
  - a segment that is not a key, an index, or an object with a `name`
  - a param that is not text, but for `installOrder`, which is an integer
  - a time that is not RFC 3339, when the validator checks `format`

  One time can be empty text: `recordedAt` of a row of `verify --summary` for a
  file that strata cannot read. Each document that strata writes from Palette
  has this shape. A record of an older release validates, but for a variable
  with `name: null`.
- **`compare --validate` and `behind` ask the route of the profile version for a
  profile that the cluster runs.** The cluster route accepted each move. It
  checks no sent tag for such a profile. Release 0.24.0 sent
  `POST /v1/spectroclusters/{uid}/validate/packs` for these cases. Now strata
  sends `POST /v1/clusterprofiles/{uid}/validate/packs`.
  - `compare --validate` sends it for a profile version that the cluster runs.
  - `behind` sends it for each record of a cluster, one time for each profile
    whose template holds the pin.
  - So `paletteValidation` can hold a refusal where it held `accepts`. The
    errors cover each cluster that runs the version, and the message names them.
    The exit code does not change. See [Ask Palette if it accepts the
    packs](../commands/compare/validate.md) and
    [Palette accepts the move, or refuses it](../commands/behind.md#palette-accepts-the-move-or-refuses-it).

### For a user of the CI drop-ins

- **Each reusable workflow that installs strata checks the signature of
  `SHA256SUMS`.** These are `comment.yml`, `merge-gate.yml`, `preflight.yml`,
  `promote-gate.yml`, `retire-gate.yml`, `upstream.yml`, and `verify.yml`. Each
  one installs `cosign` with the action `sigstore/cosign-installer`. Then it runs
  the `install-strata.sh` of the tag in `strata-version`, with
  `--require-signature`. The installer runs `cosign verify-blob` with the
  identity of the release workflow at that tag. If the signature does not
  verify, the job fails and installs nothing. Release 0.24.0 checked the binary
  against `SHA256SUMS` only.
- **An organization that permits only some actions must permit
  `sigstore/cosign-installer`.** `retire-gate.yml` also runs `actions/checkout`
  now. Release 0.24.0 used no action in that workflow, so permit
  `actions/checkout` too. A self-hosted runner must reach github.com for the
  cosign release, and the trust root of Sigstore. The installer needs `curl`
  and `jq` on the runner. The permissions of the workflows do not change. See
  [The reusable workflows](../install.md#the-reusable-workflows).
- **`strata-version` must be v0.25.0 or later.** A workflow runs the installer of
  that tag with `--require-signature`. The installer of v0.24.0 and before has no
  such option, so the job stops with `unknown option`. This happens when the
  workflow comes from a newer tag or from `main`, and `strata-version` names an
  older release. Give the same tag to both.
- **`install-strata.sh` stops when `cosign` is on the `PATH` and the signature
  does not verify.** It exits `2` and installs nothing. Release 0.24.0 did not
  check the signature. Without `cosign`, the installer installs as before and
  says that it did not check the signature. `--require-signature` makes that an
  error.
- **The GitLab job `strata-preflight` runs `preflight.sh`.** The release carries
  that script from 0.25.0. Set `STRATA_VERSION` to v0.25.0 or later.

### For a user of the library of strata

- `snapshot::ClusterSource` has the field `errors:
  BTreeMap<Open<ClusterRead>, String>` in place of `updates_pending_error:
  Option<String>` and `pending_repave_error: Option<String>`. `ClusterRead` is a
  new enum with the values `UpdatesPending` and `PendingRepave`, and
  `closed_set::Open` keeps a key of a later release. The methods
  `updates_pending_error()` and `pending_repave_error()` give the same text as
  `Option<&str>`. A struct literal must set `errors`, or use
  `..Default::default()`. A pattern with no `..` must name `errors`.
- In release 0.25.0, only `Snapshot::load` moves the fields of an older record
  into `errors`. A program that reads a `Snapshot` or a `ClusterSource` with
  serde itself gets an empty `errors` for such a record. Use `Snapshot::load`.
  Later releases move them in a serde read of a `Snapshot` too.
- `snapshot::Snapshot::variables` and `candidate::Candidate::variables` are
  `Vec<snapshot::Variable>` in place of `Vec<serde_json::Value>`. A read such as
  `v["isSensitive"]` becomes `v.is_sensitive`. A JSON candidate in memory follows
  the rules of the candidate file above.
- `diff::Leaf::segments` is `Vec<diff::Segment>` in place of `Vec<Value>`.
  `Segment::Key(String)`, `Segment::Index(usize)`, and `Segment::Named { name }`
  give the same JSON as before.
- `diff::group_variables` takes `&[Variable]` in place of `&[Value]`.

### For a user of the library of strata-preflight

- The API does not change. `world::World::with_records` gives each record in the
  shape of 0.25.0, so a cluster record has `source.cluster.errors`. A custom rule
  that reads `updatesPendingError` must read `errors.updatesPending`.

## Release 0.24.0: changes that need action

Release 0.24.0 is a minor release, so it can change the API of the library. This
section lists each change that can break a script, a program that reads a
document, or code that uses a library. A change that only adds a field or a
value is in the sections above.

### For a user of the command line or of a document

- **`strata-preflight`: two rules left the catalog.** `gke-pool-multiple-3` and
  `profile-cloud-mismatch` are gone. The validate route of Palette makes these
  two checks, and the rule `palette-validate` shows its answer.
  - `palette-validate` is a probe of the `palette` tier. It needs the key, and
    it changes the exit code only with `--strict`. So a GKE pool with a size
    that is not a multiple of 3 now gives exit `0`. It gave exit `1`. With
    `--offline`, no rule checks the size of the pool.
  - To stop a build for this fault, run the `palette` tier with the key and
    `--strict`.
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
  sends the key pair to the property route, which refuses a key pair that the
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
  more request, `GET /v1/spectroclusters/{uid}/variables`.
  - Each profile of the entity has the variables of the cluster. A sensitive
    value has the mask that Palette gives. `entity.json` of `--out` has the
    variables.
  - An error of this request stops the run with exit `2`. An answer that the
    route is not there gives no variables.
  - The key must have permission to read the variables of the cluster.
- **A change file merges `variables` by name.** A variable of the file replaces
  the variable with that name. A new name adds a variable. Before, the list of
  the file replaced the list. A change file cannot remove a variable now.
- **`snapshot --from-candidate --validate`, with a disabled pack.** The import
  validate route refuses the candidate with HTTP `500` and
  `DisabledPacksForbidden`. Strata now reads this as a refusal of Palette. The
  error kind is `palette-refuses`, with the code, in place of `api`. The kind
  names stay, but this case gives another kind. The exit code stays `2`, and
  strata writes no record. A program that reads `error.kind` must read
  `palette-refuses` for this case.
- **`behind` asks Palette about each pin that is behind.** This applies to a run
  with no `--no-impact`. Strata sends one request for each record that pins the
  pack, in the project of the pin. It also reads the document of each cluster
  record.
  - So a run sends more requests and takes more time. The exit code does not
    change.
  - The key must have permission to send `POST .../validate/packs`. If it does
    not, the pin has `paletteValidationError`, and the report shows
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
