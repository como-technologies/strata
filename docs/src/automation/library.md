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

The command line adds only the arguments, the output, and the exit codes.

## Add the dependency

Pin a release tag. The repository is private, so Cargo must fetch it with the
git command line, which uses your SSH configuration:

```toml
[dependencies]
strata = { git = "ssh://git@github.com/como-technologies/strata.git", tag = "v0.6.2" }
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
