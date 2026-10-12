# Build and test

```bash
just build             # target/release/strata and target/release/strata-preflight
just lint              # cargo fmt --check, clippy with -D warnings, rustdoc with -D warnings
just test              # the unit, library, binary, and script tests, of both packages
just library-example   # examples/library, a crate that uses strata as a library
just examples          # examples/ from the tracked registry data
just docs-check        # docs/src/output/ is the output of this build
```

`just --list` shows each recipe.

The crate is edition 2024 and needs Rust 1.88 or later. `rust-version` in
`Cargo.toml` says so, and the `msrv` job of CI builds with that version.

## Where the code goes

| Path | What it holds |
| --- | --- |
| `src/main.rs` | The command line: the arguments, the output, and the exit codes. Nothing else. |
| `src/api/` | Palette. `mod.rs` is the `PaletteApi` trait; `http.rs` sends it over HTTP, `retry.rs` tries it again, and `memory.rs` answers it from canned responses in tests. `palette.rs` holds the reads of profiles, clusters, listings, the audit log, notifications, and macro values. `raw.rs` holds the responses a snapshot is built from, and their directory on disk. `audit.rs` names the kinds of object in the audit log. |
| `src/ops/` | One module for each command: `verify.rs`, `same_release.rs`, `consumers.rs`, `behind.rs`, `list.rs`, and `packs.rs`. `fleet.rs` finds the clusters of a query of labels and writes a record of each; `repave.rs` asks Palette if a change repaves, and `validate.rs` asks Palette if it takes the packs. None is a command of its own. `mod.rs` takes a snapshot and compares two. |
| `src/snapshot.rs` | A record, and how it is built from the responses. |
| `src/shape.rs` | The fields of the `shape` tier of a cluster record, for each cloud type. |
| `src/diff.rs` | The comparison of two snapshots. |
| `src/validation.rs` | The answer of Palette to a validate route, which a record and a comparison share. |
| `src/impact.rs` | The impact of a change to a pack: the rules table and its levels. |
| `src/render/` | The output: `terminal.rs`, `markdown.rs`, and `json.rs`, with the shared lines in `mod.rs` and the unified diffs of `--full` in `unified.rs`. |
| `src/candidate.rs` | The candidate file: its rules, and the snapshot it gives. |
| `src/json.rs` | The reads of a document of Palette: text, arrays, the items of a listing, and the first text that is not empty. Each module reads a document with them. |
| `src/config.rs`, `src/error.rs`, `src/color.rs`, `src/progress.rs` | The settings, the error kinds and their exit codes, the color decision, and the progress notes on standard error. |
| `src/canon.rs`, `src/layer.rs`, `src/version.rs`, `src/reference.rs` | The canonical form, the layers, the version order, and what a ref means. |
| `xtask/` | The development tools: the stand-in for the Palette API (`palette.rs`), the tenant of each chapter of the book (`tenant.rs`, `chapters.rs`), and the runner that writes `docs/src/output/` (`book.rs`). |
| `tests/` | The tests of the crate, in one crate: `main.rs` names each file as a module, and `common/mod.rs` holds each helper that two files use. `cargo test verify::` runs the tests of `verify.rs`. |
| `adapter/` | The adapter for knurl, a package of its own: see [The knurl adapter](../automation/knurl.md). |
| `preflight/` | `strata-preflight`, a package of its own in the workspace: the rule format (`rule.rs`), the catalog (`catalog.rs`, `rules/*.yaml`), the jq engine (`jq.rs`), the world (`world.rs`), the checks (`probe.rs`), the run and the watch (`run.rs`), the confidence and the ledger (`confidence.rs`, `evidence/runs.yaml`), the report (`report.rs`, `html.rs`), and the fixtures of its tests (`fixture.rs`, `rules/fixtures/`). See [How preflight works](../internals/preflight.md). |

A new read of Palette goes in `api/palette.rs`. A new command gets a module in
`ops/`. A new field of a record goes in `snapshot.rs`, outside the root unless it
is content: see [What the root does not cover](../internals/hashes.md#what-the-root-does-not-cover).

## Where a script goes

| Who runs it | Directory | Examples |
| --- | --- | --- |
| A user, in a pipeline | `scripts/ci/` | `install-strata.sh`, `merge-gate.sh`, `verify-report.sh` |
| A maintainer, for the repository | `scripts/dev/` | `set-version.sh`, `tag-release.sh` |
| A maintainer, for one crate or one example | beside that crate or example | `examples/live/`, `adapter/scripts/` |

A script in `scripts/ci/` is a product: a page under Automation documents it, and
`tests/ci_scripts.rs` tests it. A script beside a crate or an example is a
harness for that one thing, and a `just` recipe runs it.

The development tooling is the `xtask` crate. A `just` recipe wraps each command
that the book and the examples need, and `cargo xtask <command>` runs the same
command directly. `cargo xtask` with no command prints its usage and exits `2`. The alias is in
`.cargo/config.toml`, and the crate reaches no release.

## The tests

`cargo test` runs:

- the unit tests
- the library tests against `MemoryApi`, a Palette API in memory
- the tests of the binary over fixtures with the shape of the API responses
- the tests of the scripts in `scripts/` and `examples/live/`, with stand-ins
  for `gh` and `tofu`
- the fuzzing in `tests/fuzz.rs`

No test uses the network.

### The fuzzing

`tests/fuzz.rs` sends random and hostile input to each entry point that reads a
ref, a file, a directory, or an answer from Palette. The cases are:

- refs of punctuation, and text that is not ASCII
- records of junk bytes
- a path that names a directory
- API answers with the right keys and the wrong types
- the binary itself with random arguments, against an address that refuses the
  connection

One property holds everywhere: strata answers, or it errors with a kind and
exit `2`. It never panics.

The generator is deterministic, so a failure repeats. Each case names its seed,
and a seed runs alone:

```bash
STRATA_FUZZ_SEED=1337 cargo test fuzz::
```

The default seed is fixed, so each run of CI fuzzes the same cases. Give a new
seed to look for more.

One test measures itself, because a fuzz test that stops reaching the code it
fuzzes still passes. `a_profile_of_junk_gives_a_record_that_round_trips`
fails when 50 or fewer of its 2000 cases build a record.

## Continuous integration

`.github/workflows/checks.yml` holds every check. The `ci` workflow runs it on each push to
`main` and on each pull request, and the release workflow runs it before it builds a binary:

| Job | Runs |
| --- | --- |
| lint and test | `just lint`, `just test`, `just library-example`, and `just examples` |
| docs | `just docs-check`, and `just docs` with `mdbook-lint` |
| build | the Linux musl binary, with the step that a release uses |
| the lowest Rust version | `cargo check --workspace --all-targets` with the `rust-version` of `Cargo.toml`, so the number a reader trusts is the number CI builds with |

`ci` adds one job of its own, `publish the book`, which needs the checks and runs
only on `main`, so the site is a build that every check passed. The `docs` job
builds the book once and passes it to the publish job, so no job builds it twice.

The checks are a workflow of their own because a caller cannot grant a called
workflow more than it has itself. `publish the book` needs `pages: write`, which
a release does not have, and a release that called a workflow asking for it
failed before it started.

Run the same recipes in a clean checkout before you push. A file that is not
tracked can hide a failure:

```bash
git worktree add --detach ../strata-check HEAD
cd ../strata-check && just lint && just test && just library-example && just examples && just docs-check
```

## A change to how strata reads Palette

The fixtures follow the shapes of the Palette API, but only a real tenant proves
them. A change to cluster refs, consumers, candidates, or the API client also
needs [the live example](./live-example.md). It builds a billable cluster, so it
runs by hand and never in CI.
