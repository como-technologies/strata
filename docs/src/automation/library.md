# The Rust library

The crate is a library. The command line is a small layer over it. A Rust
program can use the same parts. `strata-preflight` is a library too: see
[Use strata-preflight as a library](#use-strata-preflight-as-a-library).

| Item | Function |
| --- | --- |
| `api::PaletteApi` | The Palette API. `HttpApi` calls Palette. `MemoryApi` serves documents from memory, for tests. |
| `api::PaletteApi::post_json` | A `POST` that asks a question and changes nothing. `HttpApi` and `Retry` send it, and `MemoryApi::posted` keeps each body for a test. An API that cannot send it says so. |
| `api::Palette::validate_repave`, `ops::palette_repave`, `ops::validate_body`, `diff::PaletteRepave` | The answer of Palette to whether a change of the profiles of a cluster replaces its nodes, for a cluster and a profile version, with its reasons and whether it agrees with the impact table. |
| `api::Palette::validate_profile_packs`, `api::Palette::validate_profile_name`, `api::Palette::validate_profile_import`, `ops::profile_validation`, `ops::profile_validate_body`, `ops::profile_import_body`, `ops::snapshot_from_candidate`, `ops::require_acceptance`, `validation::ProfileValidation` | The answer of Palette, with each error. Palette says if it takes a candidate in a new profile: its name, its version, and its packs. For a profile version, it says if it takes the packs as an update of itself. When `validate` is `true`, `snapshot_from_candidate` puts the answer in `source.paletteValidation`. The caller decides what a refusal does. `require_acceptance` gives the error that the command gives. `ProfileValidation::accepts` is true when `ProfileValidation::errors` is empty, so no answer refuses with no error. |
| `api::Palette` | The client over a `PaletteApi`: the reads of a profile, a cluster, and a cluster summary. `audits`, `cluster_notifications`, `resolved_values`, and `profile_resolved_values` read the audit log of a resource, the notifications of a cluster, and the value Palette gives each macro of a cluster, a cluster group, or a profile version. `api::AuditKind` names the kind of a resource in the audit log. `api::Retry` tries a transient failure again. |
| `api::Palette::pack_document`, `cloud_account`, `cluster_events`, `subject_by_id`, `api::subject_path`, `api::cluster_summary_path`, `api::cluster_events_path` | The document of a pack version by its uid, the cloud account of a cloud type, and the first 50 events of a cluster. `subject_by_id` gives a cluster, an edge host, or a cluster group by uid or by name. The three functions give the route of a request, with each segment escaped, for a program that shows the request. `cluster_events` gives an empty list when Palette gives `items: null`. |
| `json` | The reads of a document of Palette: `str_at`, `text`, `text_or`, `opt_str`, `opt_text`, `flag`, `opt_flag`, `arr`, `strings`, `text_map`, `value_at`, and `value_or_null`. For a field that is not there, each gives a fixed value: empty text, no entry, `false`, `null`, or `None`. `text_or` gives the default that the program passes. So a program reads a document as strata does. |
| `cloud::CLOUD_ALL`, `cloud::CLOUD_CUSTOM`, `cloud::CLOUD_EDGE_NATIVE` | The cloud types of Palette that a program compares or sends by name. |
| `config::Config` | Where Palette is, and the key. `Config::from_env` reads the environment and then `~/.palette.env`, as the command does. |
| `reference::Ref` | What a ref means: a record file, `<name>@<version>`, a uid, `cluster:<name>` with `@pending` or not, `edgehost:<name>`, `clustergroup:<name>`, `appprofile:<name>@<version>`, `pack:<name>@<version>`, or a set of clusters, `tag:<key>=<value>` or `filter:<name>` (`Ref::ClusterSet`). |
| `ops::snapshot_for`, `ops::SnapshotOptions` | A record of what a ref names, through a `Palette` that a closure gives. The options add what the options of `snapshot` add: `macros`, `shape`, `raw_dir`, and `validate`. An option that does not fit the ref is a `usage` error before any request. The example in the crate documentation starts here. |
| `ops::snapshot_pair` | The two sides of `compare`: when one side is a cluster group, the `cluster:` ref of the other side holds only the profiles that the group pins. |
| `api::Palette::with_pending` | The documents of a cluster after it takes the update that waits: the profiles of `profileUpdates` in place of its templates. `Snapshot::mark_pending` marks the record. |
| `api::Palette::fetch_subject`, `edge_hosts`, `cluster_group_members` | A cluster, an edge host, or a cluster group in the shape of a cluster, by uid or by name. Also the edge hosts of the scope, and the virtual clusters of a group. `Snapshot::with_kind` and `snapshot::SubjectKind` say which object a record holds. `ops::cluster_group_snapshot` gives the record of a group with its members. `RawCluster::save_dir` saves the document of a group with `********` over each password of its values, as the record holds them. |
| `api::Palette::fetch_app_profile`, `resolve_app_profile`, `ops::app_profile_snapshot` | An app profile in the shape of a profile, with each tier as a pack in the layer `app`, and the record of it. Its `source.type` is `app`. |
| `ops::select`, `ops::ClusterQuery`, `ops::saved_filters`, `ops::TagCondition`, `ops::tenant_of` | The clusters of a query of labels, by the search of Palette or, where the tenant has none, by the listing and the labels matched in strata; and the saved tag filters of the tenant. `ListOptions::query` lists them, and `tenant_of` gives a client at tenant scope for the search. |
| `ops::snapshot_set`, `ops::SetReport` | A record of each cluster of a query, written to a directory, with `prune` to remove the records of the clusters that left. |
| `ops::behind`, `ops::BehindOptions`, `ops::BehindReport` | `behind`: each pack version that records pin, the newer versions of the registry, the impact of each bump, and, with a previous report, what arrived since. `ops::is_floating` says whether a tag is a floating tag. |
| `ops::consumers_with`, `ops::ConsumersOptions` | `consumers`, with `list_only` to read the consumer list of Palette alone. Each `Consumer` has a `kind`, an `ops::ConsumerKind`: `cluster`, `edgehost`, `template`, or `app`. `ConsumersReport::edge_hosts_read` says how many edge hosts strata read. |
| `snapshot::Snapshot` | A record. |
| `snapshot::Template`, `snapshot::Tier`, `snapshot::UpstreamSource`, `snapshot::Implementation`, `diff::Side`, `diff::ChangeFlag`, `candidate::LayerKind`, `conjunction::Conjunction`, `api::Method`, `profile_type::ProfileType`, `pack_state::PackState`, `oci_provider::OciProvider` | The closed sets of a document, each printed as the documents name it. `Source::template` is the template that a record reads, and `Source::is_draft` is true when it is the draft. The keys of `ClusterSource::tiers` and `TierDiff::tier` are tiers of a cluster record. `PackHash::upstream_source` is the field of Palette that gave the upstream digest of a pack. `Tool::implementation` is the implementation that wrote a document. `RunningBehind::side` is the side of a comparison whose cluster runs behind. `CandidateLayer::kind` is the type that a layer of a candidate gives. `SavedFilter::conjunction` is how the conditions of a saved filter join. `PackDiff::flags` and `Bump::flags` are the change flags of a pack, and `ops::Bump::of` gives the bump from a comparison. `Error::Api` and `Error::RetryAfterTooLong` name the method of the request. `ConsumedProfile::kind` is the type `app` of an app profile. `PackBuild::state`, `PackRegistry::state`, `Pin::pinned_state`, and `Newer::state` are the state of a pack version in its registry. `Pack::registry_provider_type` is the provider type of the OCI registry of a pack. |
| `closed_set::ClosedSet`, `closed_set::Open` | The trait of each closed set. `ALL` lists the values of a set, `as_str` gives the text of a value in the documents, and `parse` reads that text. Import the trait to call them. `Open` holds a value that a later release wrote, and keeps its text. |
| `diff::compute` | The comparison of two snapshots. |
| `render::Renderer` | The terminal, markdown, and JSON output. |
| `ops` | The operations of the commands: `snapshot_for`, `compare`, `verify`, `verify_many`, `same_release`, `consumers`, `behind`, `list`, `registry_packs`, and the rows above. Each command has a module of its own under `ops`; `fleet` and `repave` hold the queries of labels and the answer of Palette on a repave. |
| `ops::verify_with`, `ops::VerifyOptions` | `verify`, with what the command's options add: `who` reads the audit log for a record that drifted, and `allow_no_digest` accepts a pack with no upstream digest. `verify_many` takes the same options. |
| `ops::same_release_with` | `same_release`, and `true` accepts a pair that holds a pack with no upstream digest, as `--allow no-digest` does. |
| `Snapshot::content_digest` | The digest of what a version is. See [The content digest](../internals/hashes.md#the-content-digest). |
| `Snapshot::pins` | Each pack that a version takes from a registry: its name, its version, and its digest as Palette lists it. [The pack-ref digest](../internals/palette/packs.md#the-pack-ref-digest) says what that digest is for each kind of registry. `Pack::registry_provider_type` names the kind of registry that a pack came from. |
| `Snapshot::each_from_cluster` | A snapshot for each profile that a cluster applies, from one read of the cluster. |
| `snapshot::cluster_tiers`, `snapshot::ClusterTier`, `api::Palette::cloud_config` | The tiers of a cluster record, each with its fields, its hash, and the fields that strata could not read, from the raw documents of the cluster: the cluster document, the role bindings and the namespaces for `policy`, and, for `shape`, the cloud config that `cloud_config` reads. `RawCluster::cloud_config` holds it. |
| `cluster_template::ClusterTemplateRef` | A cluster template. `consumers` counts each template that links a version. `PaletteRepave::cluster_template` names the template that governs a cluster. |
| `diff::TierDiff`, `diff::RunningBehind` | Each tier of two cluster records compared, and each pack that a cluster runs at another version than its profile: `Diff::cluster_tiers` and `Diff::running_behind`. |
| `snapshot::pack_content`, `Pack::images`, `Pack::charts`, `diff::ArtifactChange` | The images and the charts that the values of a pack declare, and each one that moved between two sides: `PackDiff::image_changes` and `chart_changes`, counted in `Summary::images_changed` and `charts_changed`. `Snapshot::load` fills the lists of a record written before them. |
| `Pack::annotations`, `Pack::image_id`, `diff::ImageIdChange` | The annotation `imageId` of a pack reference: the machine image that an OS pack names. `Diff::image_ids_changed` names each pack with another image on each side, and `Diff::nothing_differs` is the rule of the exit code of `compare`. |
| `Pack::invalid`, `Pack::invalid_reason`, `diff::InvalidPack` | Whether Palette marks a pack reference invalid, and why. `Verify::invalid_packs` names each such pack of the version now, and `Summary::packs_invalid` counts them in a comparison. |
| `diff::ParamChange` | Each param of a pack that differs between two sides: `PackDiff::param_changes`. For an app tier, a param is a property. |
| `Snapshot::macros`, `Snapshot::upstream_unknown` | The user macros that the content of a record reads, and the packs with no upstream digest. Neither needs a request, so both work on a record written before either existed. |
| `candidate::check`, `ops::snapshot_from_loaded` | A candidate that a program builds, with no file: the rules of a [candidate file](../reference/candidate-file.md) hold for it, and it gives the snapshot that `snapshot --from-candidate` gives. |
| `Error::class`, `api::codes::REFUSAL_CODES`, `api::codes::ValidateRoute` | `Transient`, `Terminal`, or `Unknown`: what a caller does before it tries again. `Retry` uses the same rule. A 5xx with a code of `REFUSAL_CODES` or of a check of a validate route is `Terminal`. So is the 5xx of the import validate route when it matches no pack of the body to a registry. `ValidateRoute::ALL` lists each route. `ValidateRoute::refuses_answer` says if a route refuses the whole request with a status and the body of a Palette error. `ValidateRoute::ClusterCreate` is the cluster validate route, which `strata preflight` sends. |
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

Pin a release tag. The repository is private, so Cargo must fetch it with the
git command line, which uses your SSH configuration:

```toml
[dependencies]
strata = { git = "ssh://git@github.com/como-technologies/strata.git", tag = "v0.25.0" }
```

```bash
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo build
```

`git-fetch-with-cli = true` under `[net]` in `.cargo/config.toml` does the same.
Without it, the git of Cargo fails with "no authentication methods succeeded".

The crate is not on crates.io. The name `strata` there belongs to another crate,
so `cargo add strata` gets the wrong one.

## Use strata-preflight as a library

`strata-preflight` is a second crate in the same repository. It checks that
Palette can build a cluster. [Preflight in a pipeline](./preflight.md) shows
the command. A Rust program can run the same catalog over an entity of its own,
or read the ledger of runs, with no command line.

Give `strata-preflight` and `strata` the same git address and the same tag:

```toml
[dependencies]
strata = { git = "ssh://git@github.com/como-technologies/strata.git", tag = "v0.25.0" }
strata-preflight = { git = "ssh://git@github.com/como-technologies/strata.git", tag = "v0.25.0" }
```

The package is `strata-preflight`, and Rust code names the crate
`strata_preflight`. A run takes a `strata::term::progress::Progress`, so the
program needs `strata` too. With one tag, Cargo builds one copy of `strata`.
[Add the dependency](#add-the-dependency) says how Cargo fetches the private
repository. The crate is not on crates.io.

A program starts from these items:

| Item | Function |
| --- | --- |
| `world::World::from_file`, `world::World::from_cluster` | The input of a run. `from_file` reads the body of a create from a JSON or YAML file. It takes the cloud, or reads the cloud from the file. `from_cluster` reads a cluster as Palette holds it now, through a `Palette` client. |
| `World::with_change`, `World::with_records`, `World::with_palette` | What a world adds. `with_change` adds a change file. `with_records` adds the records of the profiles that the entity names. `with_palette` adds the profiles, the packs, and the cloud account that Palette gives. |
| `catalog::builtin`, `catalog::with_probes`, `rule::Catalog` | The rules of the binary, and the same rules with the files of custom probes after them. `Catalog::rules` lists them. |
| `run::Selection`, `run::RunInputs`, `run::run` | The tiers and the rule ids to run, the inputs of a run, and the run. `run` gives a `Report`. A rule that finds a fault is a result in the report, not an `Err`. |
| `probe::Backend`, `probe::LiveBackend` | Where a check gets an answer from outside the world. `LiveBackend` with `palette: None` runs the static tier with no key and no network. |
| `report::Report`, `report::CheckResult`, `probe::Verdict` | The result: one `CheckResult` with a `Verdict` for each rule that applies. `Report::exit_code` is the exit code of the command: `0`, `1`, or `2`. `Report::counted_failures` lists the failures that make it `1`. |
| `report::render`, `report::Format` | The report as terminal text, markdown, JSON, or an HTML page. |
| `confidence::Ledger`, `confidence::Confidence`, `confidence::promotion`, `catalog::LEDGER` | The ledger of runs, and the confidence of each rule that it gives. `catalog::LEDGER` is the ledger in the binary. `Ledger::load` and `Ledger::extend` add the ledger of a company. `promotion` gives the level of each rule. |
| `run::watch`, `run::WatchInputs` | The watch during a build. It needs a `Palette` client. |
| `fixture::load`, `probe::FixtureBackend`, `probe::run_check` | A world that holds the answers, for the test of a rule of a company. |
| `error::Error` | The one error type. A run that cannot answer is an `Err`, and the command gives exit code `2` for it. |

`examples/library-preflight/` is a separate crate that depends on
`strata-preflight` by path. It loads the catalog and reads
`preflight/rules/fixtures/aws/clean.json`. It runs the static rule
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

The crate documentation starts with an example that runs as a test.
`examples/library/` is a separate crate that uses strata as a library, with no
command line and no network. `examples/library-preflight/` does the same for
`strata-preflight`. One recipe runs both:

```bash
just library-example
```

CI runs it too. [Versions and compatibility](../reference/compatibility.md)
states what a version number promises for the library.
