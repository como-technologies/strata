# The Rust library

The crate is a library. The command line is a small layer over it. A Rust
program can use the same parts:

| Item | Function |
| --- | --- |
| `api::PaletteApi` | The Palette API. `HttpApi` calls Palette. `MemoryApi` serves documents from memory, for tests. |
| `api::PaletteApi::post_json` | A `POST` that asks a question and changes nothing. `HttpApi` and `Retry` send it, and `MemoryApi::posted` keeps each body for a test. An API that cannot send it says so. |
| `api::Palette::validate_repave`, `ops::palette_repave`, `ops::validate_body`, `diff::PaletteRepave` | Palette's answer to whether a change of the profiles of a cluster replaces its nodes, for a cluster and a profile version, with its reasons and whether it agrees with the impact table. |
| `api::Palette::validate_profile_packs`, `ops::profile_validation`, `ops::profile_validate_body`, `ops::snapshot_from_candidate`, `ops::require_acceptance`, `validation::ProfileValidation` | The answer of Palette, with each error. Palette says if it takes the packs of a candidate in a new profile, or of a profile version as an update of itself. When `validate` is `true`, `snapshot_from_candidate` puts the answer in `source.paletteValidation`. The caller decides what a refusal does. `require_acceptance` gives the error that the command gives. |
| `api::Palette` | The client over a `PaletteApi`: the reads of a profile, a cluster, and a cluster summary. `audits`, `cluster_notifications`, and `resolved_values` read the audit log of a resource, the notifications of a cluster, and the value Palette gives each macro of a cluster. `api::AuditKind` names the kind of a resource in the audit log. `api::Retry` tries a transient failure again. |
| `config::Config` | Where Palette is, and the key. `Config::from_env` reads the environment and then `~/.palette.env`, as the command does. |
| `reference::Ref` | What a ref means: a record file, `<name>@<version>`, a uid, `cluster:<name>` with `@pending` or not, `edgehost:<name>`, `clustergroup:<name>`, `appprofile:<name>@<version>`, `pack:<name>@<version>`, or a set of clusters, `tag:<key>=<value>` or `filter:<name>` (`Ref::ClusterSet`). |
| `ops::snapshot_for` | A record of what a ref names, through a `Palette` that a closure gives. The example in the crate documentation starts here. |
| `ops::snapshot_pair` | The two sides of `compare`: when one side is a cluster group, the `cluster:` ref of the other side holds only the profiles that the group pins. |
| `api::Palette::with_pending` | The documents of a cluster after it takes the update that waits: the profiles of `profileUpdates` in place of its templates. `Snapshot::mark_pending` marks the record. |
| `api::Palette::fetch_edge_host`, `fetch_cluster_group`, `edge_hosts`, `cluster_group_members` | An edge host and a cluster group in the shape of a cluster, the edge hosts of the scope, and the virtual clusters of a group. `Snapshot::with_kind` and `snapshot::SubjectKind` say which object a record holds. `ops::cluster_group_snapshot` gives the record of a group with its members. |
| `api::Palette::fetch_app_profile`, `resolve_app_profile`, `ops::app_profile_snapshot` | An app profile in the shape of a profile, with each tier as a pack in the layer `app`, and the record of it. `snapshot::APP_KIND` is its `source.type`. |
| `ops::select`, `ops::ClusterQuery`, `ops::saved_filters`, `ops::TagCondition`, `ops::tenant_of` | The clusters of a query of labels, by the search of Palette or, where the tenant has none, by the listing and the labels matched in strata; and the saved tag filters of the tenant. `ListOptions::query` lists them, and `tenant_of` gives a client at tenant scope for the search. |
| `ops::snapshot_set`, `ops::SetReport` | A record of each cluster of a query, written to a directory, with `prune` to remove the records of the clusters that left. |
| `ops::behind`, `ops::BehindOptions`, `ops::BehindReport` | `behind`: each pack version that records pin, the newer versions of the registry, the impact of each bump, and, with a previous report, what arrived since. `ops::is_floating` says whether a tag is a floating tag. |
| `ops::consumers_with`, `ops::ConsumersOptions` | `consumers`, with `list_only` to read the consumer list of Palette alone. Each `Consumer` has a `kind`, `cluster` or `edgehost`, and `ConsumersReport::edge_hosts_read` says how many edge hosts strata read. |
| `snapshot::Snapshot` | A record. |
| `diff::compute` | The comparison of two snapshots. |
| `render::Renderer` | The terminal, markdown, and JSON output. |
| `ops` | The operations of the commands: `snapshot_for`, `compare`, `verify`, `verify_many`, `same_release`, `consumers`, `behind`, `list`, `registry_packs`, and the rows above. Each command has a module of its own under `ops`; `fleet` and `repave` hold the queries of labels and the answer of Palette on a repave. |
| `ops::verify_with`, `ops::VerifyOptions` | `verify`, with what the command's options add: `who` reads the audit log for a record that drifted, and `allow_no_digest` accepts a pack with no upstream digest. `verify_many` takes the same options. |
| `ops::same_release_with` | `same_release`, and `true` accepts a pair that holds a pack with no upstream digest, as `--allow no-digest` does. |
| `Snapshot::content_digest` | The digest of what a version is. See [The content digest](../internals/hashes.md#the-content-digest). |
| `Snapshot::pins` | Each pack that a version takes from a registry: its name, its version, and its digest as Palette lists it. [The pack-ref digest](../internals/palette/packs.md#the-pack-ref-digest) says what that digest is for each kind of registry. `Pack::registry_provider_type` names the kind of registry that a pack came from. |
| `Snapshot::each_from_cluster` | A snapshot for each profile that a cluster applies, from one read of the cluster. |
| `snapshot::cluster_tiers`, `snapshot::ClusterTier`, `api::Palette::cloud_config` | The tiers of a cluster record, each with its fields and its hash, from the cluster document and, for `shape`, the cloud config that `cloud_config` reads. `RawCluster::cloud_config` holds it. |
| `diff::TierDiff`, `diff::RunningBehind` | Each tier of two cluster records compared, and each pack that a cluster runs at another version than its profile: `Diff::cluster_tiers` and `Diff::running_behind`. |
| `snapshot::pack_content`, `Pack::images`, `Pack::charts`, `diff::ArtifactChange` | The images and the charts that the values of a pack declare, and each one that moved between two sides: `PackDiff::image_changes` and `chart_changes`, counted in `Summary::images_changed` and `charts_changed`. `Snapshot::load` fills the lists of a record written before them. |
| `Pack::annotations`, `Pack::image_id`, `diff::ImageIdChange` | The annotations of a pack reference, and the machine image that an OS pack names in `imageId`. `Diff::image_ids_changed` names each pack with another image on each side, and `Diff::nothing_differs` is the rule of the exit code of `compare`. |
| `diff::ParamChange` | Each param of a pack that differs between two sides: `PackDiff::param_changes`. For an app tier, a param is a property. |
| `Snapshot::macros`, `Snapshot::upstream_unknown` | The user macros that the content of a record reads, and the packs with no upstream digest. Neither needs a request, so both work on a record written before either existed. |
| `candidate::check`, `ops::snapshot_from_loaded` | A candidate that a program builds, with no file: the rules of a [candidate file](../reference/candidate-file.md) hold for it, and it gives the snapshot that `snapshot --from-candidate` gives. |
| `Error::class`, `error::REFUSAL_CODES`, `error::PACK_REFUSAL_CODES`, `error::PROFILE_PACK_REFUSAL_CODES` | `Transient`, `Terminal`, or `Unknown`: what a caller does before it tries again. `Retry` uses the same rule. A 5xx with a code in one of the three lists is `Terminal`. |

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
strata = { git = "ssh://git@github.com/como-technologies/strata.git", tag = "v0.16.0" }
```

```bash
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo build
```

`git-fetch-with-cli = true` under `[net]` in `.cargo/config.toml` does the same.
Without it, the git of Cargo fails with "no authentication methods succeeded".

The crate is not on crates.io. The name `strata` there belongs to another crate,
so `cargo add strata` gets the wrong one.

## Read the API

```bash
cargo doc --open
```

Each public item has documentation. The build refuses an item without it.

The crate documentation starts with an example that runs as a test.
`examples/library/` is a separate crate that uses strata as a library, with no
command line and no network:

```bash
just library-example
```

CI runs it too. [Versions and compatibility](../reference/compatibility.md)
states what a version number promises for the library.
