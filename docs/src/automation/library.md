# The Rust library

The crate is a library. The command line is a small layer over it. A Rust
program can use the same parts. `strata-preflight` is a library too: see
[Use strata-preflight as a library](#use-strata-preflight-as-a-library).

| Item | Function |
| --- | --- |
| `api::PaletteApi` | The Palette API. `HttpApi` calls Palette. `MemoryApi` serves documents from memory, for tests. |
| `api::PaletteApi::post_json` | A `POST` that asks a question, at a route that checks the body. At `validate/repave`, Palette can save the address of the console of a pack of virtual machines. `HttpApi` and `Retry` send it, and `MemoryApi::posted` keeps each body for a test. An API that cannot send it says so. |
| `api::Palette::validate_repave`, `ops::palette_repave`, `ops::validate_body`, `diff::PaletteRepave` | The answer of Palette to whether a change of the profiles of a cluster replaces its nodes. The input is a cluster and a profile version. The answer has its reasons and says whether it agrees with the impact table. |
| `api::Palette::validate_profile_packs`, `api::Palette::validate_profile_name`, `api::Palette::validate_profile_import`, `ops::profile_validation`, `ops::profile_validate_body`, `ops::profile_import_body` | The answer of Palette, with each error. Palette says if it takes a candidate in a new profile: its name, its version, and its packs. For a profile version, it says if it takes the packs as an update of itself. |
| `ops::snapshot_from_candidate`, `ops::require_acceptance`, `model::validation::ProfileValidation` | When `validate` is `true`, `snapshot_from_candidate` puts the answer in `source.paletteValidation`. The caller decides what a refusal does. `require_acceptance` gives the error that the command gives. `ProfileValidation::accepts` is true when `ProfileValidation::errors` is empty, so no answer refuses with no error. |
| `api::Palette` | The client over a `PaletteApi`: the reads of a profile, a cluster, and a cluster summary. `audits` reads the audit log of a resource. `cluster_notifications` reads the notifications of a cluster. `resolved_values` and `profile_resolved_values` read the value that Palette gives each macro of a cluster, a cluster group, or a profile version. `api::AuditKind` names the kind of a resource in the audit log. `api::Retry` tries a transient failure again. |
| `api::Palette::pack_document`, `cloud_account`, `cluster_events`, `subject_by_id`, `api::subject_path`, `api::cluster_summary_path`, `api::cluster_events_path`, `api::cloud_account_path`, `api::cloud_config_path` | The document of a pack version by its uid, the cloud account of a cloud type, and the first 50 events of a cluster. `subject_by_id` gives a cluster, an edge host, or a cluster group by uid or by name. The five functions give the route of a request, with each segment escaped, for a program that shows or serves the request. `cluster_events` gives an empty list when Palette gives `items: null`. |
| `api::API_KEY_HEADER`, `api::PROJECT_UID_HEADER`, `config::KEY_VARS`, `config::PROJECT_VARS`, `config::URL_VARS` | The names of the two headers that each request carries. The variables hold the key, the project, and the address. A program that shows a `curl` command reads the names here. |
| `json` | The reads of a document of Palette: `str_at`, `text`, `text_or`, `opt_str`, `opt_text`, `flag`, `opt_flag`, `arr`, `strings`, `text_map`, `value_at`, `value_or_null`, and `value_or_empty_object`. For a field that is not there, each gives a fixed value: empty text, no entry, `false`, `null`, an empty object, or `None`. `text_or` gives the default that the program passes. So a program reads a document as strata does. `set_at` is the one write. It puts a value at a list of keys, and it makes an object of each value on the way that is not one. |
| `model::cloud::CLOUD_ALL`, `model::cloud::CLOUD_CUSTOM`, `model::cloud::CLOUD_EDGE_NATIVE`, `model::cloud::Cloud` | The cloud types of Palette that a program compares or sends by name. `Cloud` is a closed set: `Cloud::of` reads the text of a cloud type, and a cloud type that Palette does not know is `Cloud::Custom`. `Cloud::account` gives the cloud whose account a managed cluster uses: `eks` uses `aws`. |
| `model::cluster_state::ClusterState` | The state of a cluster in `status.state`: `Running`, `Provisioning`, `Pending`, `Importing`, `Deleting`, `Deleted`, and `Error`. `ClusterState::parse` gives `None` for a state of a later release. `ClusterState::Running.is(text)` tests the text of a document. |
| `term::color::Paint`, `term::color::ColorMode`, `term::color::Stream` | The color of the terminal text. `Paint::resolve(mode, stream)` decides it for standard output or standard error, as `strata` and `strata-preflight` do. `ColorMode::unless(true)` is `Never`, for the flag `--no-color`. |
| `text::dash`, `text::html_text`, `text::markdown_cell`, `text::code_block`, `text::cut`, `text::plural`, `text::schema_json`, `text::shell_words` | These eight functions write the texts of a report: `dash` gives `-` for empty text, and `cut` cuts a text to a length. `markdown_cell` writes a text in the cell of a markdown table, with `html_text` and a `\` before each mark of markdown. `code_block` writes a fenced code block with a fence that no line of the text ends. `plural` gives the noun or the verb that agrees with a count, and `shell_words` writes a command as a person types it in a shell. `schema_json` gives a JSON Schema as text, with its links written as plain code spans. `strata-preflight` calls them, so both tools write each of these in one way. |
| `config::Config` | Where Palette is, and the key. `Config::from_env` reads the environment and then `~/.palette.env`, as the command does. |
| `api::connect_from_env`, `api::connect_with`, `api::project_uid_named` | The client that a command line builds. `connect_from_env` reads the settings as `Config::from_env` does. It finds the uid of `PALETTE_PROJECT`, and it warns when the uid and the name name two projects. `connect_with` builds the client of a `Config`, with its retry policy. `project_uid_named` finds the uid of a project by its name, one time for each run. `strata` and `strata-preflight` connect with these functions. |
| `ops::expand_records`, `ops::is_pattern` | The record files that the arguments of a command name. A path stays as it is. Strata expands a pattern, so a quoted `releases/**/*.json` names the files at every depth. A pattern that no file matches is `Error::Pattern`. `strata-preflight` reads `--records` with them. |
| `reference::Ref` | What a ref means. A ref is a record file, `<name>@<version>`, a uid, `cluster:<name>` with `@pending` or not, `edgehost:<name>`, `clustergroup:<name>`, `appprofile:<name>@<version>`, or `pack:<name>@<version>`. A ref can also be a set of clusters, `tag:<key>=<value>` or `filter:<name>` (`Ref::ClusterSet`). |
| `ops::snapshot_for`, `ops::SnapshotOptions` | A record of what a ref names, through a `Palette` that a closure gives. The options add what the options of `snapshot` add: `macros`, `shape`, `raw_dir`, and `validate`. An option that does not fit the ref is a `usage` error before any request. The example in the crate documentation starts here. |
| `ops::SnapshotRequest`, `ops::SnapshotSource`, `ops::ObjectSource`, `ops::snapshot_object`, `ops::snapshot_for_candidate` | What `snapshot` and `candidate` read. `SnapshotRequest::source` chooses it from the ref and the flags: a set of clusters, a candidate file, saved answers, or a ref. It refuses a wrong use with `Error::Usage` or `Error::RecordNotASource`, before any request. `snapshot_object` reads the record of an object. `snapshot_for_candidate` reads the profile version or the record file that `candidate` writes from, and refuses another ref with `Error::ProfileRefRequired`. |
| `ops::snapshot_pair` | The two sides of `compare`. When one side is a cluster group, the `cluster:` ref of the other side holds only the profiles that the group pins. Its `connect` gets the `diff::Side` that it reads, so each side can read its own project. |
| `ops::SideProjects` | The project of each side of `compare` and `same-release`: `of` gives the flag of a side, else the flag of both sides. `check` refuses a flag of one side for a count of refs other than two, with `Error::Usage`, before any request. |
| `ops::PaletteQuestions` | What `compare` asks Palette about its cluster side: `repave` and `validate`. `check` refuses a pair of refs that a question cannot take, with `Error::Usage`, before any request. It reads a record file, and that read is no request. `ops::palette_repave` and `ops::palette_validation` keep the same rules for records. |
| `ops::update_checks`, `model::update::UpdateChecks`, `model::update::RequiredVariables`, `model::update::MinorSkip` | The rules that Palette checks only at the update, for a move. `ops::update_checks` checks the move of `compare --validate`, and `Bump::update_checks` holds the checks of `behind`. `MinorSkip::of` checks a move of the Kubernetes pack. See [The checks of an update](../internals/palette/profiles.md#the-checks-of-an-update). |
| `api::Palette::with_pending` | The documents of a cluster after it takes the update that waits: the profiles of `profileUpdates` in place of its templates. `Snapshot::mark_pending` marks the record. |
| `api::Palette::fetch_subject`, `edge_hosts`, `cluster_group_members` | A cluster, an edge host, or a cluster group in the shape of a cluster, by uid or by name. Also the edge hosts of the scope, and the virtual clusters of a group. `Snapshot::with_kind` and `snapshot::SubjectKind` say which object a record holds. `ops::cluster_group_snapshot` gives the record of a group with its members. `RawCluster::save_dir` saves the document of a group with `********` over each password of its values, as the record holds them. |
| `api::Palette::fetch_app_profile`, `resolve_app_profile`, `ops::app_profile_snapshot` | An app profile in the shape of a profile, with each tier as a pack in the layer `app`, and the record of it. Its `source.type` is `app`. |
| `ops::select`, `ops::ClusterQuery`, `ops::saved_filters`, `ops::TagCondition`, `ops::tenant_of` | The clusters of a query of labels. Strata uses the search of Palette. Where the tenant has none, strata uses the listing and matches the labels itself. Also the saved tag filters of the tenant. `ListOptions::query` lists them. `tenant_of` gives a client at tenant scope for the search. |
| `ops::snapshot_set`, `ops::SetReport` | A record of each cluster of a query, written to a directory, with `prune` to remove the records of the clusters that left. |
| `ops::behind`, `ops::BehindOptions`, `ops::BehindReport` | `behind` reports each pack version that records pin, the newer versions of the registry, and the impact of each bump. With a previous report, it also gives what arrived since. With `pin`, a key `<layer>/<name>`, it reports the pins of that pack and reads no other pack. It gives `Error::Usage` for a key that no record pins from a registry, before any request. `ops::is_floating` says whether a tag is a floating tag. |
| `ops::consumers_with`, `ops::ConsumersOptions` | `consumers`, with `list_only` to read the consumer list of Palette alone. Each `Consumer` has a `kind`, an `ops::ConsumerKind`: `cluster`, `edgehost`, `template`, or `app`. `ConsumersReport::edge_hosts_read` says how many edge hosts strata read. |
| `snapshot::Snapshot` | A record. |
| `snapshot::Template`, `snapshot::Tier`, `snapshot::UpstreamSource`, `snapshot::Implementation` | The closed sets of a document, each printed as the documents name it. `Source::template` is the template that a record reads, and `Source::is_draft` is true when it is the draft. The keys of `ClusterSource::tiers` and `TierDiff::tier` are tiers of a cluster record. `PackHash::upstream_source` is the field of Palette that gave the upstream digest of a pack. `Tool::implementation` is the implementation that wrote a document. |
| `diff::Side`, `diff::ChangeFlag`, `candidate::LayerKind`, `model::conjunction::Conjunction` | `RunningBehind::side` is the side of a comparison whose cluster runs behind. `PackDiff::flags` and `Bump::flags` are the change flags of a pack, and `ops::Bump::of` gives the bump from a comparison. `CandidateLayer::kind` is the type that a layer of a candidate gives. `SavedFilter::conjunction` is how the conditions of a saved filter join. |
| `api::Method`, `model::profile_type::ProfileType`, `model::pack_state::PackState`, `model::oci_provider::OciProvider` | `Error::Api` and `Error::RetryAfterTooLong` name the method of the request. `ConsumedProfile::kind` is the type `app` of an app profile. `PackBuild::state`, `PackRegistry::state`, `Pin::pinned_state`, and `Newer::state` are the state of a pack version in its registry. `Pack::registry_provider_type` is the provider type of the OCI registry of a pack. |
| `closed_set::ClosedSet`, `closed_set::Open` | The trait of each closed set. `ALL` lists the values of a set, `as_str` gives the text of a value in the documents, and `parse` reads that text. Import the trait to call them. `Open` holds a value that a later release wrote, and keeps its text. |
| `impl_closed_set!`, `closed_set::legend::Described`, `closed_set::legend::legend` | The macro that gives an enum its `ALL` and its `as_str` from one table. Also the legend of a set: what each value means, by the text of the value. `strata-preflight` uses them for its own closed sets. |
| `diff::compute` | The comparison of two snapshots. |
| `render::Renderer`, `render::Format`, `render::RowState` | The terminal, markdown, and JSON output. `Format::ALL` lists the formats, and `Format::as_str` gives the name of one. With `full`, the JSON renderer adds the field `unified` to each changed pack. With `behind_only`, `render_behind` omits each row of `behind` that is current, and the JSON keeps each one. `RowState::of_pin` and `RowState::of_profile` give the state of a row, as its status names it. |
| `timestamp::now_utc`, `timestamp::utc_text` | The time in the text of a document: RFC 3339 in UTC, with whole seconds, such as `2026-09-17T09:30:00Z`. `now_utc` reads the clock. `utc_text` writes a number of seconds since the Unix epoch, for a caller that needs the seconds too. `strata`, `strata-preflight`, and the adapter of knurl write their times with them. |
| `ops` | The operations of the commands: `snapshot_for`, `compare`, `verify`, `verify_many`, `same_release`, `consumers`, `behind`, `list`, `registry_packs`, and the rows above. Each command has a module of its own under `ops`. `fleet` and `repave` hold the queries of labels and the answer of Palette on a repave. |
| `ops::verify_with`, `ops::VerifyOptions` | `verify`, with what the options of the command add. `who` reads the audit log for a record that drifted. `allow_no_digest` accepts a pack with no upstream digest. `verify_many` takes the same options. With `full`, `verify_many` gives the diff of each row the field `unified`. `verify_with` ignores `full`. |
| `ops::check_verifiable` | Make sure from the record alone that `verify` can check it. `Error::NoUid` is the error for a record with no uid. `Error::Usage` is the error for an object of a kind that a later release wrote. `verify_with` and `verify_many` make the same check. A caller that connects first calls it before it connects. |
| `ops::same_release_with`, `ops::SameReleaseOptions` | `same_release`, with each value of `--allow`. `policy` is `Policy::RegistryMove` for `--allow registry-move`. `allow_no_digest` accepts a pair that holds a pack with no upstream digest, as `--allow no-digest` does. `allow_renumber` accepts a pack whose version alone changed, as `--allow renumber` does. With it, the function compares the archives of each renumber from the download of each profile version. It takes `connect`, which gives the client of a project, as `verify_many` does. |
| `api::Palette::profile_download`, `api::ProfileDownload` | The download of a profile version: each pack of its list, with its version and its `api::PackFile`. That is the files of its archive, no archive, or the reason of a file that is no archive. `PackArchive::changed_files` gives each file that differs between two archives. |
| `Snapshot::content_digest` | The digest of what a version is. See [The content digest](../internals/hashes.md#the-content-digest). |
| `Snapshot::pins` | Each pack that a version takes from a registry: its name, its version, and its digest as Palette lists it. [The pack-ref digest](../internals/palette/packs.md#the-pack-ref-digest) says what that digest is for each kind of registry. `Pack::registry_provider_type` names the kind of registry that a pack came from. |
| `Snapshot::each_from_cluster` | A snapshot for each profile that a cluster applies, from one read of the cluster. |
| `snapshot::cluster_tiers`, `snapshot::ClusterTier`, `api::Palette::cloud_config` | The tiers of a cluster record. Each tier has its fields, its hash, and the fields that strata could not read. The raw documents of the cluster give the tiers. These are the cluster document, the role bindings and the namespaces for `policy`, and, for `shape`, the cloud config that `cloud_config` reads. `RawCluster::cloud_config` holds it. `cluster_tiers` gives `Error::Serialize` for a cluster group whose values have a password that strata cannot mask. |
| `model::cluster_template::ClusterTemplateRef` | A cluster template. `consumers` counts each template that links a version. `PaletteRepave::cluster_template` names the template that governs a cluster. |
| `diff::TierDiff`, `diff::RunningBehind` | Each tier of two cluster records compared, and each pack that a cluster runs at another version than its profile: `Diff::cluster_tiers` and `Diff::running_behind`. |
| `snapshot::pack_content`, `Pack::images`, `Pack::charts`, `diff::ArtifactChange` | The images and the charts that the values of a pack declare. Also each one that moved between two sides: `PackDiff::image_changes` and `chart_changes`. `Summary::images_changed` and `charts_changed` count them. `Snapshot::load` fills the lists of a record written before them. |
| `Pack::annotations`, `Pack::image_id`, `diff::ImageIdChange` | The annotation `imageId` of a pack reference: the machine image that an OS pack names. `Diff::image_ids_changed` names each pack with another image on each side, and `Diff::nothing_differs` is the rule of the exit code of `compare`. |
| `Pack::invalid`, `Pack::invalid_reason`, `diff::InvalidPack` | Whether Palette marks a pack reference invalid, and why. `Verify::invalid_packs` names each such pack of the version now, and `Summary::packs_invalid` counts them in a comparison. |
| `diff::ParamChange` | Each param of a pack that differs between two sides: `PackDiff::param_changes`. For an app tier, a param is a property. |
| `Snapshot::macros`, `Snapshot::upstream_unknown` | The user macros that the content of a record reads, and the packs with no upstream digest. Neither needs a request, so both work on a record written before either existed. |
| `candidate::check`, `ops::snapshot_from_loaded` | A candidate that a program builds, with no file. The rules of a [candidate file](../reference/candidate-file.md) hold for it. It gives the snapshot that `snapshot --from-candidate` gives. |
| `Error::class` | `Transient`, `Terminal`, or `Unknown`: what a caller does before it tries again. `Retry` uses the same rule. A 5xx with the code of a refusal of Palette, or of a check of a validate route, is `Terminal`. So is the 5xx of the import validate route when it matches no pack of the body to a registry. |
| `api::codes::ValidateRoute` | `ValidateRoute::ALL` lists each route. `ValidateRoute::refuses_answer` says if a route refuses the whole request with a status and the parsed body of a Palette error. `ValidateRoute::of` gives the route that a method and a path name, and `ValidateRoute::refuses_request` joins the two. `ValidateRoute::ClusterCreate` is the cluster validate route, which `strata preflight` sends. |
| `Error::CandidateInvalid`, `api::codes::CreateRule` | A candidate that breaks a rule that only the create of Palette checks. `rule` names the rule, and its `code()` is the code of Palette. `Error::to_json` gives it as `code`. |

The command line adds only the arguments, the output, and the exit codes.

## Pin a version by digest

A program that promotes a version, or that compares two environments, needs two
answers. `snapshot.root` says if one object changed. `snapshot.content_digest()`
says if two objects are the same version. Keep the root to check a record later.
Compare content digests across registries, projects, and tenants.

[The knurl adapter](./knurl.md) is a program that does this: it gives both
digests to another tool, behind a trait of that tool.

## Add the dependency

The package is `strata-palette`, and Rust code names the crate `strata`. The
name `strata` on crates.io belongs to another crate, so `cargo add strata` gets
the wrong one. A dependency pins a release tag and names the package:

```toml
[dependencies]
strata = { git = "https://github.com/como-technologies/strata.git", tag = "v0.33.0", package = "strata-palette" }
```

## Use strata-preflight as a library

`strata-preflight` is a second crate in the same repository. It checks that
Palette can build a cluster. [Preflight in a pipeline](./preflight.md) shows
the command. A Rust program can run the same catalog over an entity of its own,
or read the ledger of runs, with no command line.

Give `strata-preflight` and `strata` the same git address and the same tag:

```toml
[dependencies]
strata = { git = "https://github.com/como-technologies/strata.git", tag = "v0.33.0", package = "strata-palette" }
strata-preflight = { git = "https://github.com/como-technologies/strata.git", tag = "v0.33.0" }
```

The package is `strata-preflight`, and Rust code names the crate
`strata_preflight`. A run takes a `strata::term::progress::Progress`, so the
program needs `strata` too. With one tag, Cargo builds one copy of `strata`.
`strata-preflight` is not on crates.io.

A program starts from these items:

| Item | Function |
| --- | --- |
| `world::World::from_file`, `world::World::from_cluster` | The input of a run. `from_file` reads the body of a create from a JSON or YAML file. It takes the cloud, or reads the cloud from the file. `from_cluster` reads a cluster as Palette holds it now, through a `Palette` client. |
| `World::with_change`, `World::with_records`, `World::with_palette` | What a world adds. `with_change` adds a change file. `with_records` adds the records of the profiles that the entity names. `with_palette` adds the profiles, the packs, and the cloud account that Palette gives. |
| `catalog::builtin`, `catalog::with_probes`, `rule::Catalog` | The rules of the binary, and the same rules with the files of custom probes after them. `Catalog::rules` lists them. |
| `rule::require_release`, `rule::release_in_range` | `require_release` gives `Error::Usage` for a release that is not a version, such as `4.11.x`. `release_in_range` puts such a release in each range of a rule, so a program calls `require_release` first. |
| `run::Selection`, `run::RunInputs`, `run::run` | The tiers and the rule ids to run, the inputs of a run, and the run. `run` gives a `Report`. A rule that finds a fault is a result in the report, not an `Err`. |
| `probe::Backend`, `probe::LiveBackend` | Where a check gets an answer from outside the world. `Backend::palette` takes the `refusal_codes` of the check: the codes of a 5xx that the check reads as an answer. `LiveBackend::new(None, true)` runs the static tier with no key and no network. A backend with a client keeps the answer of each request for the run. It is a `PaletteApi` too. `Palette::new(&backend)` reads the world through it, so the world and the checks share the answers. |
| `probe::is_answer` | `probe::is_answer(status, Reader)` says which 4xx is an answer. A check reads a 403 as the key, and the world reads it as an object that is not there. |
| `report::Report`, `report::CheckResult`, `probe::Verdict` | The result: one `CheckResult` with a `Verdict` for each rule that applies. `Report::exit_code` is the exit code of the command: `0`, `1`, or `2`. `Report::counted_failures` lists the failures that make it `1`. |
| `report::render`, `report::Format` | The report as terminal text, markdown, JSON, or an HTML page. `render` takes a `strata::term::color::Paint`, which colors the terminal text alone. |
| `rule::Fix::line`, `confidence::Confidence::text` | The two texts that each form of the report writes in one way. `Fix::line` gives the change and the owner, such as `Check the zones. (palette)`. `Confidence::text` gives `catch 7/7 (65%), clean 9/9 (72%)`, and ends with `older runs 3` when the counts omit three runs of older releases. |
| `confidence::Ledger`, `confidence::Confidence`, `confidence::promotion`, `catalog::builtin_ledger` | The ledger of runs, and the confidence of each rule that it gives. `catalog::builtin_ledger` gives the ledger in the binary. `Ledger::load` and `Ledger::extend` add the ledger of a company. `promotion` gives the level of each rule. With a release, it counts only the runs of that release. |
| `run::watch`, `run::WatchInputs` | The watch during a build. It needs a `Palette` client. |
| `fixture::load`, `probe::FixtureBackend`, `probe::run_check` | A world that holds the answers, for the test of a rule of a company. An entry of `_responses` with a status that is not 2xx follows the live backend. A 401, a 403, and a 429 are errors. A 5xx is an answer only when the check names its code. |
| `world::clouds`, `world::cloud_list` | The texts of the engines of a run, in the order of the help screen. An engine is a cloud of Palette, or `edge`, which Palette names `edge-native`. |
| `rule::Tier`, `rule::Level`, `rule::Severity`, `rule::Status`, `rule::EvidenceKind`, `rule::CheckKind`, `rule::Owner`, `curl::Saves`, `confidence::RunResult`, `probe::Verdict` | The closed sets of the rule format and of the report. Each is a `strata::closed_set::ClosedSet`, so `as_str` gives the text that a rule file and the report use. |
| `curl::KEY_LINE_BREAK` | The message of a `reproduce` line that runs no `curl`, because the key of Palette holds a line break. |
| `curl::Saves::comment` | The end of a `reproduce` line that sends a request at which Palette saves something. The comment says what Palette saves. The filter `saves_comment` gives it to a line of a rule. |
| `error::Error` | The one error type. A run that cannot answer is an `Err`, and the command gives exit code `2` for it. |

`examples/library-preflight/` is a separate crate that depends on
`strata-preflight` by path. It loads the catalog and reads
`crates/strata-preflight/rules/fixtures/aws/clean.json`. It runs the static rule
`aws-local-zone-letter` and prints the verdict. It needs no key, no network,
and no command line:

```rust
{{#include ../../../examples/library-preflight/src/main.rs:run}}
```

The program prints `PASS`, the id of the rule, and its title. It exits with
code `1` when the verdict is not `PASS`.

## Read the API

```bash
cargo doc --open
```

Each public item has documentation. The build refuses an item without it. In a
clone of strata, `just api-docs` builds the same pages into `target/doc/strata/`
and `target/doc/strata_preflight/`.

Each public function that returns a `Result` has an `# Errors` section. It names
the error kinds that the function gives, and when. A function that can panic has
a `# Panics` section. `just lint` refuses a public function that lacks the
section that it needs.

The crate documentation starts with an example that runs as a test.
`ops::snapshot_for`, `ops::compare`, `ops::verify`,
`ops::snapshot_from_candidate`, and `candidate::resolve` have an `# Examples`
section that runs as a test too.

`examples/library/` is a separate crate that uses strata as a library, with no
command line and no network. `examples/library-preflight/` does the same for
`strata-preflight`. One recipe runs both:

```bash
just library-example
```

CI runs it too. [Versions and compatibility](../reference/compatibility.md)
states what a version number promises for the library.
