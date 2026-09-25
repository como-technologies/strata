# The Rust library

The crate is a library. The command line is a small layer over it. A Rust
program can use the same parts:

| Item | Function |
| --- | --- |
| `api::PaletteApi` | The Palette API. `HttpApi` calls Palette. `MemoryApi` serves documents from memory, for tests. |
| `api::Palette` | The client over a `PaletteApi`: the reads of a profile, a cluster, and a cluster summary. `audits`, `cluster_notifications`, and `resolved_values` read the audit log of a resource, the notifications of a cluster, and the value Palette gives each macro of a cluster. `api::Retry` tries a transient failure again. |
| `config::Config` | Where Palette is, and the key. `Config::from_env` reads the environment and then `~/.palette.env`, as the command does. |
| `reference::Ref` | What a ref means: a record file, `<name>@<version>`, a uid, `cluster:<name>`, or `pack:<name>@<version>`. |
| `ops::snapshot_for` | A record of what a ref names, through a `Palette` that a closure gives. The example in the crate documentation starts here. |
| `snapshot::Snapshot` | A record. |
| `diff::compute` | The comparison of two snapshots. |
| `render::Renderer` | The terminal, markdown, and JSON output. |
| `ops` | The operations of the commands: `snapshot_for`, `compare`, `verify`, `verify_many`, `same_release`, `consumers`, `list`, and `registry_packs`. Each command has a module of its own under `ops`. |
| `ops::verify_with`, `ops::VerifyOptions` | `verify`, with what the command's options add: `who` reads the audit log for a record that drifted, and `allow_no_digest` accepts a pack with no upstream digest. `verify_many` takes the same options. |
| `ops::same_release_with` | `same_release`, and `true` accepts a pair that holds a pack with no upstream digest, as `--allow no-digest` does. |
| `Snapshot::content_digest` | The digest of what a version is. See [The content digest](../internals/hashes.md#the-content-digest). |
| `Snapshot::pins` | Each pack that a version takes from a registry: its name, its version, and its digest as Palette lists it. [The pack-ref digest](../internals/palette.md#the-pack-ref-digest) says what that digest is for each kind of registry. |
| `Snapshot::each_from_cluster` | A snapshot for each profile that a cluster applies, from one read of the cluster. |
| `Snapshot::macros`, `Snapshot::upstream_unknown` | The user macros that the content of a record reads, and the packs with no upstream digest. Neither needs a request, so both work on a record written before either existed. |
| `candidate::check`, `ops::snapshot_from_loaded` | A candidate that a program builds, with no file: the rules of a [candidate file](../reference/candidate-file.md) hold for it, and it gives the snapshot that `snapshot --from-candidate` gives. |
| `Error::class` | `Transient`, `Terminal`, or `Unknown`: what a caller does before it tries again. `Retry` uses the same rule. |

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
strata = { git = "ssh://git@github.com/como-technologies/strata.git", tag = "v0.7.1" }
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
