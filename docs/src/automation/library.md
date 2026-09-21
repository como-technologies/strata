# The Rust library

The crate is a library. The command line is a small layer over it. A Rust
program can use the same parts:

| Item | Function |
| --- | --- |
| `api::PaletteApi` | The Palette API. `HttpApi` calls Palette. `MemoryApi` serves documents from memory, for tests. |
| `snapshot::Snapshot` | A record. |
| `diff::compute` | The comparison of two snapshots. |
| `render::Renderer` | The terminal, markdown, and JSON output. |
| `ops` | The operations of the commands: `compare`, `verify`, `same_release`, `consumers`, `list`. |
| `Snapshot::content_digest` | The digest of what a version is. See [The content digest](../internals/hashes.md#the-content-digest). |
| `Snapshot::pins` | Each pack that a version takes from a registry: its name, its version, and its digest. |
| `Snapshot::each_from_cluster` | A snapshot for each profile that a cluster applies, from one read of the cluster. |
| `Error::class` | `Transient`, `Terminal`, or `Unknown`: what a caller does before it tries again. `Retry` uses the same rule. |

The command line adds only the arguments, the output, and the exit codes.

## Pin a version by digest

A program that promotes a version, or that compares two environments, needs two
answers. `snapshot.root` says if one object changed. `snapshot.content_digest()`
says if two objects are the same version. Keep the root to verify a record, and
compare content digests across registries, projects, and tenants.

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

## Read the API

```bash
cargo doc --open
```

The crate documentation starts with an example that runs as a test.
`examples/library/` is a separate crate that uses strata as a library, with no
command line and no network:

```bash
just library-example
```

CI runs it too. [Versions and compatibility](../reference/compatibility.md)
states what a version number promises for the library.
