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

Five sections list the changes that need action:
[Release 0.28.0](#release-0280-changes-that-need-action),
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
are gone: release 0.26.0 deprecated them, and release 0.27.0 removed them.
`model::cluster_state::ClusterState` names each state, and
`ClusterState::Running.is(text)` replaces `text == RUNNING_STATE`.
`ClosedSet::parse` reads the text of a state.
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
crate holds no module of them. Each old path stayed as a `pub use` for one minor
release, and release 0.27.0 removed the old paths. Use the new path:

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
  of the trait. Both need `use strata::closed_set::ClosedSet`, because
  `strata-preflight` does not export the trait. `Tier::parse` is `Tier::from_input`.
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

After 0.26.0, the library of `strata` changes in these ways.
[Release 0.27.0](#release-0270-changes-that-need-action) gives the action for each one:

- `closed_set::unknown` is new. It holds the log of the values that strata does not know, the report
  of those values, and the send. Its `Report` and `report::Entry` refuse a field that they do not
  list.
- `ops::unknown_report`, `ops::send_unknown_report`, and `term::ask::yes_or_no` are new.
- `render::Renderer` has the new required method `render_report`.
- `error::Error` has the new variants `LogInvalid` and `ReportSend`.
- `api::codes::is_refusal_code` is new. `Error::class` gives `Terminal` to a 5xx with
  `AwsKeyPairNotFound` or `FeatureNotSupported`, where 0.26.0 gave `Transient`.
- The old paths of the types of Palette are gone: use `strata::model::<module>`. The constants
  `ops::RUNNING_STATE`, `ops::ERROR_STATE`, and `ops::DELETED_STATE` are gone too.
- The `Display` of `Error::Api` for a 403 with the code `ProjectUidIsNotEmpty` has new words. The
  kind and the exit code stay.

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
- `rule::Check::Exec` holds a `rule::ExecCheck` in place of the four fields. A pattern that names
  `argv`, `needs`, `timeout`, or `expr` must read them from the `ExecCheck`. `ExecCheck` is new. It
  has the field `palette: Option<rule::PaletteRead>`, the `GET` request that the check sends before
  its command. A struct literal must set it to `None`.
- `probe::Ran::Exec` has the new field `request: Option<probe::Call>`. A struct literal must set it
  to `None`, and a pattern with no `..` must name it. `probe::Ran::requests` gives the one request
  for a command that has a read of Palette.
- `rule::PaletteRead` is a new type. A rule file takes `palette: { path: ... }` in the check of kind
  `exec`. The program of the check reads the answer as `.response`.
- `rule::ExecCheck` has the field `also: Vec<String>`, the second command of the check. A
  struct literal must set it to `Vec::new()`. A rule file takes `also: [...]` in the check of kind
  `exec`. The program of the check reads the output as `.also`.
- `probe::Ran::Exec` has the new field `also: Option<String>`, the line of the second command. A
  struct literal must set it to `None`, and a pattern with no `..` must name it. `probe::Ran::text`
  gives both lines, with `; ` between them. `probe::Ran::answer` is still the exit code of the first
  command.
- `rule::PaletteCheck` has the new field `refusal_codes: Vec<String>`, the codes of a 5xx that the
  check reads as an answer. A struct literal must set it to `Vec::new()`, and a pattern with no
  `..` must name it. A rule file takes `refusal_codes: [...]` in the check of kind `palette`. The
  JSON Schema of `preflight --schema rules` has the new key.
- `probe::Backend::palette` has a fourth argument, `refusal_codes: &[String]`, and an `impl Backend`
  must take it. A 5xx whose `code` is in the list is a `Response` with `ok: false`. So is a refusal
  of a validate route, such as `POST /v1/spectroclusters/{cloud}/validate`, as in 0.26.0. Each other
  5xx is an error. For `LiveBackend`, `&[]` gives the result of 0.26.0. For `FixtureBackend`, an
  entry with another 5xx is now an error, where 0.26.0 gave a `Response`: see
  [Release 0.27.0](#release-0270-changes-that-need-action).

After 0.27.0, the library of `strata` changes in these ways:

- `model::cloud::supports` and `model::pack_state::PackState::is_retired` are public.
  `strata-preflight` calls them. Their results do not change.
- `model::cluster_state::ClusterState::is_gone` is new. It is true for a document with the
  state `Deleted` or with `metadata.isDeleted`. `ops` reads a record with the same rule.
- `render::count_of` is public. It gives `1 probe`, `2 probes`, or `no probes`.
- `render::paint_state` is new. The terminal form of `consumers` and of `list` paints a state with
  it. `consumers` painted `Running` green and `unknown` yellow. `list` painted `Running` green.
  Now both paint a building state yellow, and `Error`, `Deleting`, and `Deleted` red. A text
  without color does not change.

After 0.27.0, the library of `strata-preflight` changes in these ways:

- `probe::is_answer` has a second argument, `probe::Reader`. `Reader::Check` gives the result of
  0.27.0. `world::unreadable(status)` is gone: `is_answer(status, Reader::World)` gives its result.
  A 403 is an error for `Reader::Check`, and the world holds `null` for it.
- `probe::palette_error_line` is gone. No code called it.
  `strata::model::validation::PaletteRefusal::from_error_body` reads the same body.
- `catalog::LEDGER` is private. `catalog::builtin_ledger()` gives the ledger of the binary as a
  `Result<confidence::Ledger>`.
- `rule::Fix::owner` is a `rule::Owner`, not `String`. A rule file with an owner that is not
  `palette`, `provider`, `lab`, `strata`, `operator`, or `none` fails at load, with the
  error `catalog`. The text of the report does not change.
- `confidence::RunResult` implements `ClosedSet`. `report::Format` derives `clap::ValueEnum`,
  and the command line reads it. The values and their texts do not change.
- `rule::Rule::bar` is new. `run::DEFAULT_EVERY_SECS` and `run::DEFAULT_LIMIT_SECS` are new. They
  hold the defaults of `--every` and `--limit`.
- `report::Summary::applies` is new. It counts the checks that ran and the checks that did not run.
- `world::World::to_value` and `fixture::load` add the field `packFacts` to the world. See
  [The world document](./preflight-rules.md#the-world-document). A rule or a custom probe can
  read it.
- `run::watch` ends when the document of the cluster has `metadata.isDeleted`, also when its state
  is not `Deleted`. Before, it read until the limit, and the report held an error for each rule
  that passed.

After 0.27.0, `timestamp` is a public module of `strata`. `timestamp::now_utc` gives the time
now as RFC 3339 in UTC, with whole seconds. `timestamp::utc_text` writes a number of seconds
since the Unix epoch in the same way. `strata`, `strata-preflight`, and the adapter of knurl
call them, so the three write a time in one way.

After 0.27.0, `render::Format` has `ALL`, `as_str`, and `from_text`, from one table. In
`strata-preflight`, the jq programs of a rule can call the new filter `semver_numbers`. It reads
a text as a version, as `reference::version::numbers` does. They can call `count_of` too: it
writes a count with its noun, as `render::count_of` does.

After 0.27.0, two items are new, and both only add to the API. `reference::version::line` gives the
release line of a version: its first numbers, joined by dots, such as `4.10` for `4.10.3-rc.1`.
`probe::FixtureBackend::outputs` in `strata-preflight` gives the output of each command that a
fixture holds under `_exec`, as `exec` answers them.

After 0.29.0, four types of the library of `strata` have a new field. A struct literal must set
each new field, and a pattern with no `..` must name it:

- `ops::BehindOptions` has the field `pin: Option<String>`, the key `<layer>/<name>` of the one
  pack to report. Give `None` to get the result of 0.29.0, or use `..Default::default()`.
- `ops::VerifyOptions` has the field `full: bool`. `ops::verify_many` reads it. Give `false` to
  get the result of 0.29.0, or use `..Default::default()`.
- `diff::ValuesDiff` has the field `unified: Option<String>`. Give `None`, or use
  `..Default::default()`.
- `diff::ManifestChange` has the field `unified: Option<String>`. The type has no `Default`, so a
  struct literal must set the field to `None`.

After 0.29.0, the JSON renderer reads the argument `full` of `render::Renderer::render`. With
`true`, it adds the field `unified` to the values and to the manifest changes of each changed pack.
It ignored `full` before. `ops::behind` gives `Error::Usage` for a `pin` that no record pins from a
registry. It gives the error before any request.

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

`strata-unknown/1` and `strata-report/1` refuse each field that they do not list, because the log and
the report must hold nothing else. A new field of either takes a new id.

Two values changed after 0.21.0 with no new id. In `strata-packs/1`, a version
that the listing gives no type has the `type` `spectro`, where 0.21.0 wrote
empty text. The schema now gives the field the pack types. A record of a
cluster omits a label whose value is not text, where 0.21.0 kept it with
empty text. The labels are outside the root, so no root changes.

After 0.29.0, `strata-diff/1` has the new field `unified`. It is in `packs[].values` and in each
entry of `packs[].manifests[]`, and only with `--full`. The id stays. A document without `--full`
is as before. With `verify --summary --full --json`, the `diff` of each drifted row of
`strata-verify-summary/1` has the field. [The field unified](./documents.md#the-field-unified)
says what the text holds.

Four values change after 0.27.0 with no new id. A new id breaks each reader of the document
for a case that is rare. [Release 0.28.0](#release-0280-changes-that-need-action) gives the
action for each one.

- In `strata-packs/1`, `latest` marks the version that `pack:<name>@latest` records. Before, it
  marked the highest version. The two differ only for a pre-release above the release, and for
  `--cloud`. The field stays a flag, and `paletteLatest` keeps the mark of Palette.
- In `strata-preflight/1`, `answer` of the check `iam-role-policy-quota` has the exit code of
  each command, such as `exit 0; exit 254`. It was `exit 0`. Only this rule has a second
  command, and `summary.exitCode` is the field that a script must read.
- In `strata-snapshot/1`, `valuesHash` of a cluster group with a password in values of no
  canonical form covers the masked text. Other values keep their hash. The tier is not part of
  the root, so no root changes.
- In `strata-unknown/1` and `strata-report/1`, the fields `palette`, `status`, and `errorCode`
  are gone. No release wrote them, so no reader of a document of a command sees the change.

After 0.26.0, three documents have new fields, and each keeps its id.
`confidence` of a check in `strata-preflight/1` has `olderRuns`, the number of runs
of older releases that the counts omit. `strata-preflight-promotion/1` has
`release`, and `olderRuns` in each entry of `confidence`. `olderRuns` is `0` when
the run names no `--release`. `strata-preflight-rules/1` has the new keys `refusal_codes` in a
check of kind `palette`, and `palette` and `also` in a check of kind `exec`. A reader that ignores
unknown fields reads all three.

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
absent is a value. Strata omits a field that is false, empty, or zero, and each
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
Each hash that covers such a character changes. These are `valuesHash`,
`contentHash`, `variablesHash`, `hash.authored` and `hash.node` of a pack,
`contentDigest`, and the `hash` of a tier. So the `root` changes.

The policy tier of a cluster sorts its role bindings, their subjects, and its
namespaces by the same text, so their order can change too. `verify` of an older
record that holds such a character shows a change. A record with none of them
keeps each hash.

`compare`, `same-release`, and the gates show the change for two records of one
release that two releases took. `same-release` of such a pair gives `different`
and exit `1`. [Release 0.26.0](#release-0260-changes-that-need-action)
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

The log needs no network. It stays on the machine, and no command sends it. Only the setting
`STRATA_REPORT=1`, or your answer of yes, lets a run send the report of the log: see
[Send the report by itself](./report.md#send-the-report-by-itself). A line holds only the
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
The backend keeps no 5xx that is not a refusal, and the next rule asks again.

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

- `strata-preflight --promotion --release 4.10` counts the runs of `4.10` alone.
  A rule that no run proved on `4.10` is a probe. When the rule file says
  `level: preflight`, the advice is `demote`, and the exit code is `1`. After a
  run on `4.10` is in the ledger, the rule is a preflight check again.
- A run with no release counts for no release. Release 0.26.0 counted it for each
  release. The ledger of the lab has no release in its runs, so with `--release`
  the lab proves no rule. The rule `gke-no-release-channel` is `demote` for each
  release below 4.11, the range of the rule. It stays so until the ledger holds a
  run of the lab with a release.
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

After 0.26.0, `strata-preflight` asks Palette first for five rules that ran the tool of a cloud.
Palette lists the fields that these rules read, or a part of them:

- **`static-subnets` and `sg-name-collision` are in the `palette` tier.** They read the VPC route and
  the security group route of Palette. Before, they were in the `cloud` tier and ran `aws`.
  - A run with `--tier cloud` does not run them now. A run with `--no-exec` and the key runs them.
  - They need the field `spec.cloudAccountUid` in the entity. A run without it skips them.
  - `static-subnets` fails a subnet that no VPC of the account lists. It fails a subnet in another
    VPC, and a subnet in another zone than the pool says.
  - The messages stay the same, and the rules need no tool.
- **`aws-quotas`, `gcp-quotas`, and `azure-vcpu-quota` read the vCPUs of a pool from Palette.**
  - The rules stay in the `cloud` tier, because no route of Palette lists a quota.
  - Each rule reads the instance type route first, and then runs the tool of the cloud for the
    quota. The report shows the command, and the request in `requests[]`.
  - Before, a rule counted the vCPUs from the name of the type: 4 for an `xlarge` type, and 2 for a
    smaller one. Now it counts the `cpu` that the route lists, so the sum can differ from 0.26.0.
  - The three rules need the key and the field `spec.cloudAccountUid`. A run with no key shows
    `NOT RUN` for them. Before, a run with the tool and no key checked the quota.
  - Each rule sends the query `cpuGtEq=0&memoryGtEq=0` with `cloudAccountUid`. When the query says
    nothing, the route lists only the types with 2 vCPUs or more and 4 GiB of memory or more. The
    query lifts that filter, so a pool of `t3.small` counts.
  - A pool whose type the route still does not list, such as an old family, fails the rule. The
    message names the pool and the type. Before, the rule counted the vCPUs of that pool from the
    name of the type. The message of a short quota counts the listed pools alone.
  - When Palette answers with an error, the rule fails and gives the code of Palette.

After 0.26.0, `instance-offering` sends the same query: `cpuGtEq=0&memoryGtEq=0`. The list holds the
small types now: the types with fewer than 2 vCPUs or less than 4 GiB of memory. Before, the rule
reported such a type as not in the list of the region.

After 0.26.0, the message of `api` for a 403 with the code `ProjectUidIsNotEmpty` says that the
route is of the tenant scope. The kind `api` and the exit code `2` stay.

After 0.27.0, the text of `strata-preflight` agrees each noun with its count. The report said
`1 probe(s) failed`, and now it says `1 probe failed`, `2 probes failed`, or `no probes failed`. The
same holds for the warnings, the notes, the errors, the rules, the checks, and the readings of a
watch. It holds for the events of `--verbose` and for the message of each rule too. A message that
wrote `2 key pair(s)` now writes `2 key pairs`. A run of one rule says `1 rule applies` and
`0 of 1 check passed`.

`summary.verdict` in the JSON changes in the same way, for example `FAIL: 2 checks failed: a, b`.
`--promotion` says `1 level disagrees with the ledger`, `2 levels disagree with the ledger`, or
`no levels disagree with the ledger`.
A script must read `summary.exitCode`, and not the text. These words change no exit code.

After 0.27.0, a watch of `strata-preflight` ends when the document of the cluster has
`metadata.isDeleted`, also when `status.state` is not `Deleted`. Before, it read until the limit,
and each rule that passed became an error with exit `2`. Now the exit code is `0` when no counted
rule failed, as it is for the state `Deleted`. The verdict says that the cluster is gone: see
[the watch](../commands/preflight/watch.md).
The terminal form of `strata consumers` and `strata list` colors a cluster state as
`strata-preflight --verbose` does. Green is `Running`, yellow is a build, and red is `Error`,
`Deleting`, or `Deleted`.

After 0.29.0, `compare --json --full` writes the field `unified`. `--json` ignored `--full`
before, so the document was the same with and without it. The text is the text that `--full`
prints. A secret that Palette masks stays `********` in it, and no value of a cluster variable is
in it. See [The field unified](./documents.md#the-field-unified).

After 0.29.0, two options are new, and both only add:

- `verify --full` gives the unified diffs of a record that drifted, as `compare --full` does. For
  one record, the report prints them, and `--json` writes `unified` in `packs[]`. With many
  records or `--summary`, it changes the JSON alone.
- `behind --pin <layer>/<name>` reports the pins of one pack, such as `addon/cert-manager`. It
  compares the move and asks Palette about it, and it reads no other pack. A key that no record
  pins from a registry gives the error `usage` and exit `2`, before any request. `--pin` with
  `--no-impact` gives a usage error of the command line and exit `2`.

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
  can happen for a version that does not parse. Release 0.28.0 changes `latest`:
  see [the section of release 0.28.0](#release-0280-changes-that-need-action).

Three more results follow from the order:

- **Two versions that differ only by a `v`, a missing `.0`, or build data are one
  version.** `1.2` equals `1.2.0`, and `v1.2.3` equals `1.2.3`. When a profile
  holds both, `<name>@latest` gives `profile-name-ambiguous`. `versionMove` is absent
  for such a pair.
- **A version that does not parse is above each version that parses.** Examples
  are `1.2.3.4` and `1.0.0-a_b`. So `<name>@latest` gives `1.2.3.4` in place of
  `2.0.0`, and `strata packs` lists it first. `behind` names no three-number
  version as newer than the pin `1.2.3.4`.
- **`strata list` orders the profiles by the same order.**

The order of a version that does not parse is the order of strata. No live run
shows how Palette orders such a version.

`strata-diff/1` has the new field `versionMove`, in the document and in each
entry of `packs[]`. It is `forward` when side B has the higher version, and `back`
when side B has the lower. It is absent when a side has no version, and when the
two versions have the same precedence. The id stays, because a reader ignores a
field that it does not know. `scripts/ci/merge-gate.sh` reads `versionMove`, and
it does not sort the versions as text now. A move between a version and its
pre-release gives the other verdict: see
[Release 0.26.0](#release-0260-changes-that-need-action).

## Release 0.28.0: changes that need action

Release 0.28.0 is a minor release, so it can change the API of the library. This
section lists each change that can break a script, a program that reads a
document, a CI pipeline, or code that uses a library. A change that only adds a
field, a value, an input, or an option is in the sections above.

### For a user of the command line or of a document

- **`strata packs`: `latest` marks the version that `<name>@latest` records.** Before,
  it marked the highest version, also when that version was a pre-release.
  - The two differ when the highest version is a pre-release and the registry has a version that
    is not a pre-release. An example is `1.31.0-rc.1` above `1.29.0`. `latest` marks `1.29.0` now.
  - `latest` marks one of the versions that the listing keeps. With `--cloud`, that is not
    always the version that the ref records, because the ref ignores `--cloud`. With `--cloud`,
    the listing keeps the versions that support that cloud. An example is `2.0.0` for `aws` and
    `1.0.0` for `gcp`. `strata packs <name> --cloud gcp` marks `1.0.0`, and
    `pack:<name>@latest` records `2.0.0`.
  - `paletteLatest` stays on the version that Palette marks. `--latest` keeps that version
    too, and the terminal output marks it `latest in Palette`.
  - A script that reads the highest version must read the first entry of `versions[]`. The
    list is in the order of Semantic Versioning, from the highest.
- **`pack:<name>@latest` picks from the versions that the registry serves.** Release 0.27.0
  picked from each version of the pack listing. `strata packs` omits a version with no tag in the
  answer of the registry, unless Spectro Cloud retired it. Palette does not serve such a version.
  Now the ref and the listing use that one list.
  - If `1.30.0` has no tag and `1.29.0` has one, the ref records `1.29.0`. Release 0.27.0
    recorded `1.30.0`. `latest` marks `1.29.0` in both releases.
  - A FIPS tenant gives no tag to a version that is not FIPS compliant.
  - An exact version, such as `pack:<name>@1.30.0`, reads the whole listing as before.
- **The message of `snapshot-schema` names the field inside the cluster of a record.** It
  named `source.cluster` for each such field. Now it names `source.cluster.profiles[0].version`,
  for example. The error document has the new key `field`, which holds the same path. A script
  that matched `source.cluster` in the text of the message must match the full path.
- **`stale-record`: `cluster` holds the uid of the cluster.** It held the name, while the
  other errors of a cluster held the uid. The new key `name` holds the name. A script that read
  the name from `error.cluster` must read `error.name`. The message stays.
- **The policy `valuesHash` of a group with a password in values of no canonical form changes.**
  - A key of such values is a map or a list. An example is a template with no quotes. See
    [The config that a cluster group gives](../commands/snapshot/other-objects.md#the-config-that-a-cluster-group-gives).
  - The hash covered the text with each password in clear. It covers the text with `********`
    now. A new password changes no hash, and the record of a saved directory has the hash of
    the record of the fetch.
  - Other values keep their hash. That includes values of no canonical form that hold no
    password. A record of 0.27.0 of a group with such values has the same `policy` tier as a
    new record.
  - A record of 0.27.0 or before of a group with a password differs in the `policy` tier from a
    new record of the same group. `compare` and `verify` show that difference. The root does not
    change. Take the record again.
  - A snapshot, `verify`, and `compare` of such a group give the error `serialize` and exit `2`
    when strata cannot mask a password in the values. A snapshot writes no record. `save_dir`
    gave that error before. In 0.27.0, `verify` and `compare` gave exit `0` for such a group.
  - Taking the record again gives the same error. Quote the template, or change the block scalar
    in the values of the group.
- **`verify-report.sh` and `upstream-report.sh` run when an open issue has no text in its
  body.** GitHub gives `null` as the body of such an issue.
  - `upstream-report.sh` stopped with a jq error on each version of jq. `verify-report.sh`
    stopped on the jq of `ubuntu-latest`.
  - Both scripts read the first line of such a body as an empty line now. They skip the issue
    and do their usual work. No script of yours needs a change.
- **`pr-comment.sh` skips a comment with no text.** The jq filter stopped with an error on a
  body that is `null`.
  - The script skips such a comment and continues. This is the rule of `verify-report.sh`. No
    script of yours needs a change.
  - The call that creates the label of the impact is quiet now, as in the report scripts. It
    prints what the host says only when the call fails.
- **`install-strata.sh` prints its notes when `STRATA_QUIET` is set.** Release 0.27.0 hid them.
  That included the note that says that the signature is not checked.
  - `STRATA_QUIET` stops the progress messages of the `strata` binary. No script in `scripts/ci/`
    reads it. See [Settings](./settings.md).
  - The notes go to standard error. The path of the binary on standard output does not change.
- **`strata-preflight`: seven rules read a version as `strata` does.** The rules are
  `capa-stack-release-behind`, `ccm-version`, `edge-agent-version`, `gke-version-not-offered`,
  `eks-version-not-offered`, `eks-addon-version`, and `maas-os-image`.
  - `capa-stack-release-behind` stopped with an error for `--release 4.11.0-rc.1`. It compares
    the numbers of the release now. A `--release` that is no version is still an error, and the
    message names it.
  - `capa-stack-release-behind` compared a tag with four numbers, such as `1.26.15.1`. It stops
    with an error now, and the message names the tag. A tag such as `1.26.x` was an error before
    too.
  - `ccm-version` and `edge-agent-version` read a `v` before a version, and build data after
    it. A version with one of them can give another result than before.
  - The last four rules cut the version of a pack at the dots. The text `v1.32.1` gave the line
    `v1.32`, and `1.32+build.5` gave `1.32+build`. No cloud offers such a line, so the rule
    failed. The four rules read the numbers now. Both texts give the line `1.32`, as `1.32.1` did.
  - `gke-version-not-offered` matches an exact version when the pack writes three numbers. It
    matches `1.34.11` now for `v1.34.11`, `1.34.11-rc.1`, and `1.34.11+build.5`. Before, it
    matched the whole text, so these texts failed.
  - `maas-os-image` reads the numbers 22 and 4 as Ubuntu 22.04, so the text `22.4` passes now.
    Before, only the text `22.04` passed. Its message names the version as the pack writes it.
    It named the first two parts of the text.
  - A text that is no version reads as it did in 0.27.0 in the five rules that cut it. Examples
    are `1.26.x` and `4.10.x`. These rules read the first two parts between the dots.
  - `maas-os-image` reads the first two parts of such a text as numbers. So `22.4.x` and
    `22.4.1.1` pass now, and they failed before.
- **`strata-unknown/1` and `strata-report/1` lose the fields `palette`, `status`, and `errorCode`.**
  No release of strata wrote them. A log or a report of a command of 0.27.0 never held them.
  - A line or an entry with one of these fields is not valid now. `strata report` stops with the
    error `log-invalid`, and the endpoint answers 422. Only a log that a person edited, or the log
    of a program that called `Watch::palette_release`, has one.
  - The table of `strata report --format markdown` has the columns `set`, `value`, `route`,
    `count`, and `strata`. It had `Palette`, `status`, and `error code` too.
  - The issue of the endpoint has no row `Releases of Palette` and no row `Refusals`.
- **A value of the log and of the report is one printable ASCII word.** Release 0.27.0 kept
  each character that is not a control character.
  - Strata writes a `?` for each space, and for each character that is not printable ASCII.
    Examples are a zero width space and a letter that is not ASCII. No value of a closed set of
    strata has a space.
  - Markdown drops one space at each end of a code span. So the values `pending` and ` pending `
    gave two issues that look equal.
  - A value of more than 64 characters ends with `...`. It ended with `…`.
  - A line of an older log with a space, or with such a character, is not valid now.
    `strata report` stops with the error `log-invalid`. Delete the line, or delete the file
    `strata-unknown.jsonl`.
  - The endpoint of 0.27.0 took a report with such a value. The endpoint of 0.28.0 answers 422 for
    the whole report. A client of 0.27.0 then marks no line, so it sends the same report at each
    run. No other value of that log reaches the endpoint.
  - After the update, the send of a run skips such a line. It sends the other lines of the log.
    The run writes a warning that gives the count of the skipped lines and the number of the
    first one. `--quiet` hides the warning, and the exit code stays `0`.
  - `strata report` still stops with the error `log-invalid` on such a line. Delete the line to
    stop the warning.
  - The mark counts each line of the log that is not blank, valid or not. This is the meaning in
    0.27.0, so a mark of 0.27.0 and a mark of this release mean the same. The update moves no mark.
  - A line that you delete before the mark moves the mark. The next line moves up under it, and the
    send skips that line. For each line that you delete before the mark, lower the number in the
    file `strata-unknown.sent` by 1.
- **`strata-preflight`: `answer` of a check with a second command has the exit code of each command.**
  Only `iam-role-policy-quota` has a second command.
  - `answer` was `exit 0`, the exit code of the first command alone. A failed read of the quota
    also showed `exit 0`. It is `exit 0; exit 254` now, in the order of `ran`.
  - A script that compares `answer` of this rule with `exit 0` must read both codes.
  - The lines of `reproduce` for `.exec` and `.also` hold the exit codes that the commands gave.
    They held `exit: 0` for both.
- **`strata-preflight`: the ledger of the binary counts the 13 clean runs of `iam-ten-policies` for `iam-role-policy-quota`.**
  - The runs named the old id, so they counted for no rule. A clean build that passes the quota of
    10 also passes the quota of 20. The confidence of the rule in the report counts them now.
    `--promotion` counts them on `aws` and on `eks`.
  - The ledger no longer holds runs of `eks-ami-type`, `profile-cloud-mismatch`, and
    `gke-pool-multiple-3`. These rules left the catalog, so the runs counted for no rule.
  - A ledger that you give with `--ledger` keeps its own ids. Rename the old ids as the
    [release 0.27.0](#release-0270-changes-that-need-action) item of the rule says.
- **The root item of release 0.26.0 names four hashes. Four more hashes change with it.**
  They are `contentDigest`, `hash.authored` and `hash.node` of a pack, and the `hash` of a
  tier. The characters are those of that item.
  - `same-release` of a record of 0.25.0 and a record of 0.26.0 or later for one raw
    directory gives `different` and exit `1`. It names the pack and says that the authored
    hash differs.
  - The action stays the same: take both records with one release. See
    [Release 0.26.0](#release-0260-changes-that-need-action).
- **`merge-gate.sh` stops with exit `2` when strata is older than the script.** The strata of
  0.25.0 writes no `versionMove`. With it, the script of 0.26.0 or later found no backward pin and
  the gate was green.
  - The script reads the schema of the comparison, `strata schema diff`, and looks for the field.
    It compares no version numbers.
  - The reusable workflows and the GitLab job install the script and the binary from one tag, so
    they do not change. A local run or a custom job must use the strata of the same release.
- **The legend text of the status `registry-move` of `same-release` changes.** `legend.status`
  says "moved to another registry" in place of "pinned to another registry". The text is for a
  reader, and no script needs a change.
- **Some descriptions in `strata schema` and in `strata-preflight --schema` have new words.** The
  doc comments of public items feed them. No key or id changes. One type changes: see the item
  for `fix.owner` below. No other change needs a change in a script.
  - `strata schema behind`: the description of `newer` in a pin.
  - `strata schema consumers`: the description of the definition `ClustersRead`.
  - `strata schema diff` and `strata schema verify-summary`: the description of `tagRenamed` in
    a pack.
  - `strata schema packs`: the descriptions of `latest` and `paletteLatest` in a pack build.
  - `strata-preflight --schema report`: the descriptions of the verdicts `not-run` and
    `skipped`, and of `answer` and `requests` in a check.
  - `strata-preflight --schema rules`: the descriptions of the checks `palette`, `exec`, and
    `watch`, and of `check` and `when` in a rule.
  - `strata-preflight --schema report` and `--schema rules`: the description of `owner` in a fix.
- **`strata-preflight`: the `owner` of a fix is one of six values.** The values are `palette`,
  `provider`, `lab`, `strata`, `operator`, and `none`. In 0.27.0 the owner was free text.
  - A rule file of `--probes` with another owner stops the run with the error `catalog` and
    exit `2`. A file with `owner: team-network` ran in 0.27.0. Use `operator`, and name the team
    in the text of the fix.
  - `fix.owner` in `strata-preflight/1` and in `strata-preflight-rules/1` is a list of six
    values in the schema, and no longer a `string`. The ids stay, because the catalog writes
    only the six values.
  - `report::Report` refuses a report of 0.27.0 with another owner. Only a custom probe writes
    such an owner. Change it in the file of the report before you read the file again.
- **`strata-preflight --watch`: a deleted cluster ends the watch.** In 0.27.0, a cluster with
  `metadata.isDeleted` and the state `Deleting` read until the limit. Then each rule that passed
  became an error, and the exit code was `2`.
  - The watch ends at the first reading now. The exit code is `0` when no counted rule failed,
    as it was for the state `Deleted`.
  - A pipeline that gated on exit `2` for such a cluster passes now.
  - The verdict of a watch that ended at a cluster that is gone says so. It was
    `OK: each counted check passed`, and it is `OK: the cluster is gone; no counted check failed`
    now. A watch with no check that ran says `OK: the cluster is gone; no check ran`.
  - The new words are in `summary.verdict` of the JSON, and in each form of the report. A script
    must read `summary.exitCode`. The exit code stays `0`.
  - A watch of a cluster that is not gone keeps its words. A failed or an error verdict keeps its
    words too, also for a cluster that is gone.
- **`strata-preflight`: the words of a count and of the messages of the rules change.** A script
  must read `summary.exitCode`, and not the text.
  - `summary.verdict`, `subject.source` of a watch, and the last line of `--promotion` agree each
    noun with its count. An example is `4 readings` in place of `4 reading(s)`.
  - The terminal form and the markdown of a report change in the same way. The summary says
    `1 rule applies` and `0 of 1 check passed`.
  - The messages of 17 rules wrote `(s)` after a noun. Each one agrees the noun with its count
    now: `1 key pair`, `2 key pairs`, or `no key pairs`. A count of zero reads `no`, as the
    summary does. A test of the catalog refuses `(s)` in a rule.
  - The rules are `aws-region-not-enabled`, `eks-fargate-private`, `dedicated-hosts`,
    `aks-system-pool`, `edge-two-node`, `edge-host-in-use`, `edge-host-resources`,
    `edge-host-duplicate`, `vsphere-instance-floor`, `ssh-key-pair`, `gke-pool-roll`,
    `macro-silent`, `eks-one-zone`, `variables-in-use`, `pvc-lb-leftover`,
    `leftovers-before-build`, and the `reproduce` line of `eks-quotas`.
  - Six messages have more than new nouns:
    - `eks-fargate-private` says `the route table of 2 subnets (a, b) sends 0.0.0.0/0 to igw-1`.
      It said `subnet(s) a, b route 0.0.0.0/0 to igw-1`.
    - `edge-host-in-use` says `part of 1 cluster (other-cluster)`, and `macro-silent` says
      `reads 1 macro (region)`. Each one puts the names in brackets after the count.
    - `edge-host-duplicate` says `is in the pools a, b`.
    - `pvc-lb-leftover` says `1 EBS volume ... is available` or `2 EBS volumes ... are available`.
      `leftovers-before-build` says `1 instance ... is still in` and `left it`, or
      `2 instances ... are still in` and `left them`.
  - The messages of `aws-quotas`, `gcp-quotas`, and `azure-vcpu-quota` have new words. A long
    sentence of each is now two short sentences or more.

### For a user of the library of strata

- `snapshot::cluster_tiers` gives a `Result`. A call must handle the error, or use `?`. It
  gives `Error::Serialize` for a cluster group whose values have a password that strata
  cannot mask. `Snapshot::from_cluster` gives the same error.
- `error::Error::StaleRecord` has the new field `name: String`. A literal must set it, and a
  pattern with no `..` must name it. The field `cluster` holds the uid.
- `ops::PackBuild::latest` is true for the version that `pack:<name>@latest` records, of the
  versions that the listing keeps. It was true for the highest version. With `--cloud`, the
  ref can record another version. See the first item of the previous list.
- `timestamp` and `render::Format::ALL` are new: see [The Rust library](#the-rust-library).
- `closed_set::unknown::log::Line` and `closed_set::unknown::report::Entry` hold one
  `closed_set::unknown::sighting::Sighting`. It has the fields `strata`, `set`, `value`, and
  `route`. The JSON stays flat. Read `line.sighting.value` where you read `line.value`.
  - The fields `palette`, `status`, and `error_code` are gone from both, and
    `Watch::palette_release` is gone. No code of strata wrote them.
  - `log::FIELDS` has five items. `Sighting::new` is not public, as `Line::new` was not.
- `closed_set::unknown::Hint`, `closed_set::unknown::Line`, and `closed_set::unknown::Report` are
  gone. Each type has one path: `closed_set::unknown::hint::Hint`,
  `closed_set::unknown::log::Line`, and `closed_set::unknown::report::Report`.
- `closed_set::unknown::send::Step` and `closed_set::unknown::send::Choice` are `#[non_exhaustive]`.
  A `match` on either needs a `_` arm.
- `config::state_dir` is not public. The functions `closed_set::unknown::log::path`,
  `closed_set::unknown::hint::path`, and `closed_set::unknown::send::answer_path` give the files.
- `text::code_span` and `text::code_cell` are new. `code_span` writes a text as a code span of
  markdown, with a fence that the text cannot end. It escapes nothing, for a title and a sentence.
  `code_cell` writes the span for a cell of a table, and it also escapes a `|` and a line break.
- `closed_set::unknown::send::post` gives `Result<Vec<usize>>`. The list holds the numbers of the
  entries, from 1, that the endpoint holds back for the hour: the list `waiting` of its answer.
  The list is empty when the endpoint took the whole report. The function gave `Result<()>`.
- `ops::send_unknown_report` gives `Result<ops::Sent>`. It gave `Result<()>`. The new type has the
  fields `marked`, `waiting`, and `skipped`.
  - A line of the log that is not valid does not stop the send now. The function skips it, and it
    never gives `Error::LogInvalid`. The field `skipped` holds the numbers of such lines.
  - The mark of the sent lines counts each line that is not blank, valid or not, as in 0.27.0.
    The send skips a line that is not valid, and the mark still counts it.
  - When the endpoint holds back some values, the function marks the lines before the first line
    of such a value, and stops. It does not mark the other lines. The field `waiting` counts the
    lines that it left.
- **A correction to the section of release 0.27.0.** The module `closed_set::unknown` is new in
  0.27.0, and so are `ops::unknown_report`, `ops::ReportOptions`, and `term::ask`. Release 0.26.0
  has none of them.
  - That section says that `Report` and `report::Entry` "now implement `Deserialize`". They are
    new, and they refuse a field that they do not list. The document `strata-report/1` is new too.
  - It says that `ops::unknown_report` "only reads" the log, that `send::post` "has the third
    argument", and that `ReportOptions` "has the field `all` alone". These words compare with a
    state inside the wave. A program that moves from 0.26.0 has no earlier form of them.

### For a user of the library of strata-preflight

- `world::Engine`, `world::ENGINES`, and `run::RULES_SCHEMA` are not public. No program of
  this repository used them. `world::clouds()` and `world::cloud_list()` give the texts of the
  engines, and the document of `preflight --rules --json` holds its schema id in `schema`.
- A program in a rule can call the new jq filter `semver_numbers`. A rule file that you give
  with `--probes` can use it. It runs in `strata-preflight` only, and not in the `jq` tool.
  - The report writes a `reproduce` line for such a program with a definition of the filter in
    front. So the line runs in the `jq` tool.
- A program in a rule can call the new jq filter `count_of` too. `2 | count_of("key pair")` gives
  `2 key pairs`, `1` gives `1 key pair`, and `0` gives `no key pairs`. It gives the words of
  `render::count_of`. The `reproduce` line of such a program holds a definition of it.
  - A number that is not a whole number of zero or more gives the number and the plural noun.
    `-1 | count_of("CPU")` gives `-1 CPUs`, and `0.5` gives `0.5 CPUs`. A value that is no number
    is an error of the program.
  - So a rule can name a count that Palette gives and that is under 0. The message of
    `vsphere-instance-floor` for `numCPUs` of `-1` is
    `pool cp-pool has -1 CPUs`. The rule gives that message, and no error.
- `report::Assembly` has the new field `cluster_gone: bool`. A literal must set it. `false` gives
  the verdicts of 0.27.0. `true` is for a watch that ended at a cluster that is gone: a verdict
  of `OK` then says so.
- `probe::is_answer` has a second argument. Change `is_answer(status)` to
  `is_answer(status, Reader::Check)`.
- `world::unreadable` is gone. Change `unreadable(status)` to `is_answer(status, Reader::World)`.
- `probe::palette_error_line` is gone. Call
  `strata::model::validation::PaletteRefusal::from_error_body` in its place.
- `catalog::LEDGER` is private. Call `catalog::builtin_ledger()`, which gives a
  `Result<confidence::Ledger>`. The library example of 0.27.0 called
  `Ledger::parse(catalog::LEDGER, ...)`, and a program that copied it must change that call.
- `rule::Fix::owner` is a `rule::Owner`, and no longer a `String`. `Owner::as_str` gives the
  text. A literal with `owner: "operator".into()` becomes `owner: Owner::Operator`. A rule file
  with another owner fails at load: see the item for `fix.owner` above.
- `probe::Ran::Exec` has the fields `first: probe::Invocation`, `also: Option<probe::Invocation>`,
  and `request: Option<probe::Call>`. It had `line`, `answer`, and `also: Option<String>`.
  - `probe::Invocation` is new. It holds the `line` of a command and its `exit: Option<i32>`.
    A literal with `line: l` and `answer: Some("exit 0".into())` becomes
    `first: Invocation::ended(l, 0)`. A literal with `answer: None` becomes
    `first: Invocation::not_run(l)`. The same two calls build the value of `also`.
  - `probe::Ran::answer` gives an `Option<String>` in place of an `Option<&str>`. For two
    commands it holds the exit code of each, such as `exit 0; exit 254`.

## Release 0.27.0: changes that need action

Release 0.27.0 is a minor release, so it can change the API of the library. This
section lists each change that can break a script, a program that reads a
document, a CI pipeline, or code that uses a library. A change that only adds a
field, a value, an input, or an option is in the sections above.

### For a user of the command line or of a document

- **`strata-preflight`: the rule `iam-ten-policies` is now `iam-role-policy-quota`.**
  The old id held a number, and AWS changed the number. The default quota of managed
  policies for a role is 20 now. It was 10.
  - The rule reads the quota of the account from Service Quotas. It fails a role that
    is over that quota. A role with 11 to 20 policies failed the old rule, and passes now.
  - When the rule cannot read the quota, it uses the default 20. The message of a failure
    says so.
  - A script that reads `iam-ten-policies` in a report must read `iam-role-policy-quota`.
    `--rule iam-ten-policies` is a `usage` error with exit `2`, because the id is not in the
    catalog.
  - The entry of the rule in `checks[]` holds both commands in `ran`, with `; ` between them.
    Before, `ran` held the command that lists the policies of the role alone. `answer` is
    still the exit code of that first command.
  - A run of your ledger that names `iam-ten-policies` counts for no rule. Rename it in `rule:`.
- **`strata-preflight`: `iaas-pod-identity` is `FAILED` for a key pair that the region does not
  hold, and for a Pod Identity account. It was `ERROR`.**
  - Palette answers HTTP 500 with the code `AwsKeyPairNotFound` for that key pair, and with the
    code `FeatureNotSupported` for a Pod Identity account. A check read each 5xx as an error. The
    rule now names both codes in the new key `refusal_codes`, so the program reads each 500 as
    the answer of the route.
  - A script that reads the verdict `error` of `iaas-pod-identity` for these faults must read
    `failed`. The rule is a probe, so a `FAILED` result changes the exit code only with
    `--strict`. The `ERROR` result gave exit `2`.
  - A rule file that you give with `--probes` takes `refusal_codes` in a check of kind
    `palette`. A 5xx with a code that the check does not name is still `ERROR`, except a refusal
    of a validate route such as `POST /v1/spectroclusters/{cloud}/validate`.
  - The loader refuses a code in `refusal_codes` that the library of strata does not list as a
    refusal at a 5xx. The library sends a request again after any other 5xx, so a check reads
    such a code only after the retries. The error is `catalog` and names the code.
- **`strata-preflight`: the rule `system-macros-hidden` has no check.**
  - A check reads a 403 of Palette as an error of the key, and never as the result of a rule.
    So the old check could not report the 403 that it named. A run gave `ERROR` and exit `2` for
    a key that cannot read the route.
  - The report shows `not-checked` for the rule now, and the `reproduce` line says what to run
    by hand. A script that reads the verdict `passed` or `error` of `system-macros-hidden` must
    read `not-checked`.
- **`strata-preflight`: the rule `eks-ami-type` is gone.** The validate route of Palette refuses an
  AMI type that EKS does not take, so the rule repeated Palette.
  - `palette-validate` reports the refusal: `UnsupportedAmiType` for `AL2_x86_64`, and
    `IsValidValue` for a type that Palette does not know. It needs the key.
  - `--rule eks-ami-type` is a `usage` error with exit `2`, because the id is not in the catalog.
  - A run with `--offline` does not find an AL2 AMI type now. With `--strict`, such a pool gave
    exit `1`, and now it gives exit `0`.
  - An EKS entity has one passed static check fewer, so its readiness falls. A `--ready` gate can
    fail where it passed.
  - A run of your ledger that names `eks-ami-type` counts for no rule.
- **`strata-preflight --promotion --release` counts only the runs of that release.** A rule with
  `level: preflight` and no run on it is `demote`. The exit code is `1`. Write `release:` in each
  run of your ledger. See [The command line](#the-command-line).
  - `gke-no-release-channel` is `demote` for each release below 4.11. For example,
    `--promotion --release 4.10` gave exit `0` in 0.26.0, and it gives exit `1` now.
  - A run with no `release:` counts for no release.
  - With no `--release`, the result is the same as in 0.26.0.
- **`strata-preflight`: the confidence of a check with `--release` counts only that release.** A
  rule with older runs alone shows `no runs, older runs <n>`. Release 0.26.0 showed the counts of
  those runs.
- **`strata-preflight`: `static-subnets` and `sg-name-collision` are in the `palette` tier.**
  `--tier cloud` does not run them. Give `--tier palette`, or no `--tier`. Both rules need the key
  and `spec.cloudAccountUid`, and no `aws` tool.
- **`strata-preflight`: `aws-quotas`, `gcp-quotas`, and `azure-vcpu-quota` need the key and
  `spec.cloudAccountUid`.** They read the vCPUs of a pool from the instance type route of Palette.
  - A run with no key shows `NOT RUN` for them, and the readiness falls. A `--ready` gate can fail
    where it passed. A run with the tool and no key checked the quota in 0.26.0.
  - A pool whose type the route does not list fails the rule. The sum of the vCPUs comes from the
    `cpu` that the route lists, so it can differ from 0.26.0.
- **`strata-preflight`: `instance-offering` sends `cpuGtEq=0&memoryGtEq=0`.** A small type, such as
  `t3.small`, no longer fails the rule.
- **`strata report` is a new command.** A file or a ref named `report` now names the command when
  it comes first. After another argument, it gives an error with exit `2`. Write
  `strata compare report <B>` for such a ref.
- **This release names the endpoint of the report, so a run in a terminal asks one question.** The
  release sends nothing by itself. You choose the send with the setting `STRATA_REPORT=1` or with a
  yes to the question. See [Send the report by itself](./report.md#send-the-report-by-itself).
  - A run of `strata` asks at its end when it wrote a line to the log. The variable `STRATA_REPORT`
    must not be set, and you must not have answered before.
  - A run asks nothing in three cases. `CI` has a value. Standard input or standard error is not a
    terminal. Or the run has `--quiet` or `STRATA_QUIET`.
  - `STRATA_REPORT=0` stops the question and the send. Strata keeps your answer in the file
    `strata-unknown.answer` of the state folder, and asks one time.
  - `STRATA_REPORT_URL` names another address than the endpoint of the maintainers.
  - A script that runs strata in a terminal must set `STRATA_REPORT=0` to avoid the question.
- **`api`: the message for a 403 with `ProjectUidIsNotEmpty` changed.** It says that the route is of
  the tenant scope. The request named a project, and the key is right. The kind `api` and exit `2`
  stay. A script that reads the text of that message must change.

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
- `error::Error` has the new variant `ReportSend`, with the fields `endpoint` and `message`.
  `ops::send_unknown_report` gives it when the endpoint of the report did not take the report.
  The command line prints it as a warning and never as the result of a run. Add a `_` arm, or name
  `Error::ReportSend { .. }`. `Error::kind` gives `report-send` for it, and `Error::to_json` gives
  the same kind. `Error::class` gives `Unknown`.
- `closed_set::unknown::Report` and `closed_set::unknown::report::Entry` now implement
  `Deserialize`, and they refuse a field that they do not list. The JSON Schema of
  `strata-report/1` has `additionalProperties: false` for the report and for an entry. The
  document does not change. `Report::fault` is new: it checks a report that arrives from outside.
- `api::codes::is_refusal_code` is new. It says whether a code is the code of a refusal at a 5xx
  that a retry does not change. `Error::class` gives `Terminal` to a 5xx with `AwsKeyPairNotFound`
  or `FeatureNotSupported`. Release 0.26.0 gave `Transient`, so `api::Retry` sent the request
  again. It sends the request one time now.
- The new module `closed_set::unknown::send` holds the decision and the post of the send.
  `ops::send_unknown_report` posts the report of the unmarked lines, and then marks them. The new
  method `Watching::lines_written` tells how many lines a watch wrote. `term::ask::yes_or_no` asks
  a question. `text::query_value` and `BOOK_URL` are now public.
- `ops::unknown_report` only reads the log. It gives an `ops::UnsentReport`, with the `report` and
  the count `lines` of the lines that it read. `ops::mark_reported` marks those lines as sent, and
  the caller runs it after the report went out. `ops::ReportOptions` has the field `all` alone.
- `closed_set::unknown::send::post` has the third argument `position`: the number of lines of the
  log before the report. It puts the key of the send in the header `Idempotency-Key`, so the
  endpoint counts a second try of the same lines one time. A report holds 20 different values or
  fewer: see `closed_set::unknown::report::MOST_VALUES`. `Report::fault` refuses a count above
  `MOST_COUNT`.
- The old paths of the types of Palette are gone. `strata::cloud` and the nine other modules at
  the root, and `strata::content::layer`, do not build. The compiler gives `E0432` or `E0433` for
  such a path. Use `strata::model::<module>`: see [The Rust library](#the-rust-library) for the
  list. Release 0.26.0 gave no warning for an old module path.
- `ops::RUNNING_STATE`, `ops::ERROR_STATE`, and `ops::DELETED_STATE` are gone, and so are the
  same constants in `model::cluster_state`. The compiler gives `E0425` for them. Use
  `ClusterState::Running.is(text)`, and `ClosedSet::as_str` for the text of a state.
- The `Display` of `Error::Api` for a 403 with the code `ProjectUidIsNotEmpty` changed. It says
  that the route is of the tenant scope and that the request named a project. A program that reads
  the text must change. `Error::kind` and the exit code stay.
- `model::validation::PaletteRefusal::from_error_body` is new in the public API. It was private to
  the crate. It reads the `code` and the `message` of the body of a Palette error. It gives `None`
  when the body is not JSON with a `code` of text or of a number. No existing call changes.

### For a user of the library of strata-preflight

The list [After 0.26.0](#the-rust-library) gives the details.

- `confidence::promotion` has a third argument, `release: Option<&str>`. A call with two
  arguments does not build. Give `None` for the result of 0.26.0.
- `confidence::Ledger::counts_for` counts a run with no release for no release. Release 0.26.0
  counted such a run for each release. The build does not show this change.
- `confidence::Counts` has the new field `older: Vec<String>`, and `confidence::Confidence` has the
  new field `older_runs: u32`. `confidence::PromotionReport` has the new field
  `release: Option<String>`. A struct literal must set each field.
- `confidence::Confidence::text` gives `no runs` for no run. It ends with `older runs <n>` when
  `older_runs` is not `0`. A `report::CheckResult` has a `confidence` for a rule with older runs only.
- `rule::Check::Exec` holds a `rule::ExecCheck`. `Check::Exec { argv, needs, timeout, expr }`
  becomes `Check::Exec(ExecCheck { argv, needs, timeout, palette: None, also: Vec::new(), expr })`
  in a literal and in a pattern. `rule::PaletteRead` is new.
- `probe::Ran::Exec` has the new fields `also: Option<String>` and `request: Option<probe::Call>`.
  A literal must set them, and a pattern with no `..` must name them. `Ran::text` joins the two
  commands with `; `.
- `rule::PaletteCheck` has the new field `refusal_codes: Vec<String>`. A struct literal must set
  it to `Vec::new()`, and a pattern with no `..` must name it.
- `probe::Backend::palette` has a fourth argument, `refusal_codes: &[String]`. An `impl Backend`
  must take it, and a call must give it. A 5xx whose `code` is in the list is a `Response` with
  `ok: false`. So is a refusal of a validate route, such as `POST /v1/spectroclusters/{cloud}/validate`,
  as in 0.26.0. Each other 5xx is an error. For `LiveBackend`, `&[]` gives the result of 0.26.0.
- `FixtureBackend` follows the same rule for an entry of `_responses` with a status that is not
  2xx. An entry with a 401, a 403, or a 429 is an error, as on a live run. An entry with a 5xx
  status is a `Response` only for a check that names its code, or for a refusal of a validate
  route. Release 0.26.0 gave each such entry as a `Response`. A fixture of a custom rule can need
  `refusal_codes` in its check now.
- `catalog::with_probes` and `rule::Catalog::parse_section` give `Error::Catalog` for a
  `refusal_codes` entry that `api::codes::is_refusal_code` does not accept.
- The catalog has no `eks-ami-type`, and `iam-ten-policies` is `iam-role-policy-quota`.
  `Catalog::rule("iam-ten-policies")` gives `None`.

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
