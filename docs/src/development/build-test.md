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
| `src/main.rs`, `src/cli/` | The command line, and nothing else. `main.rs` parses the arguments, runs the command, and gives the exit code. `cli/args.rs` holds the arguments, and `cli/<command>.rs` the run of each command. `cli/mod.rs` holds the parts that the commands share: the connection, the time, and the exit code of a comparison. |
| `src/lib.rs` | The library: its modules, and the old path of each module that moved. |
| `src/api/` | Palette. `mod.rs` is the `PaletteApi` trait; `http.rs` sends it over HTTP, `retry.rs` tries it again, and `memory.rs` answers it from canned responses in tests. `raw.rs` holds the responses a snapshot is built from, and their directory on disk. `mask.rs` writes the mask over a secret value. `codes.rs` holds the codes of the answers of Palette that strata reads. `audit.rs` names the kinds of object in the audit log. |
| `src/api/palette/` | The reads of Palette. `profiles.rs` reads the profiles and the app profiles. `clusters.rs` reads the clusters, the cluster groups, and the edge hosts. `listings.rs` reads the listings, the searches, and the audit log. `mod.rs` builds the routes and reads each page. |
| `src/ops/` | One module for each command: `verify.rs`, `same_release.rs`, `consumers/`, `behind.rs`, `list.rs`, and `packs.rs`. `consumers/app.rs` reads the consumers of an app profile, and `consumers/reads.rs` the reads beside the consumer list. `consumer_list.rs` reads the consumer list of a profile document. `fleet.rs` finds the clusters of a query of labels and writes a record of each; `repave.rs` asks Palette if a change repaves, and `validate.rs` asks Palette if it takes the packs. None is a command of its own. `mod.rs` takes a snapshot and compares two. |
| `src/snapshot/` | A record: the document in `mod.rs`, the pack in `pack.rs`, the record of a cluster in `cluster.rs`, and the build from the responses in `build.rs`. `shape.rs` and `policy.rs` hold the fields of the `shape` tier and the `policy` tier of a cluster record. |
| `src/cloud.rs` | The cloud types of Palette, and the rule for a pack that supports a cloud. |
| `src/pack_type.rs`, `src/profile_type.rs`, `src/scope.rs`, `src/conjunction.rs` | The closed sets of a document: the type of a pack or a profile, the scope of a version, and the join of a search. Each is one enum, with its `parse` and its `as_str`. |
| `src/legend.rs` | The legend of a document: the meaning of each value of a closed set, by its name in the documents. |
| `src/diff/` | The comparison of two snapshots. `mod.rs` holds the document and `compute`, and `leaf.rs` the comparison of a leaf. `pack.rs` compares the packs, `variables.rs` the variables, and `cluster.rs` the cluster records. `verify.rs` holds the verify report. `impact.rs` holds the impact of a change to a pack: the rules table and its levels. |
| `src/validation.rs` | The answer of Palette to a validate route, which a record and a comparison share. |
| `src/repave.rs` | The reasons of Palette for a repave, and a repave that waits for approval. |
| `src/cluster_template.rs` | The cluster templates of Palette, which link profile versions to clusters. |
| `src/render/` | The output: `terminal/`, `markdown.rs`, and `json.rs`. `terminal/lines.rs` paints the lines of a change for the terminal. `terminal/table.rs` holds the tables of the terminal: the width of each column, and the cells that match a query. A line that two renderers show is in the module of its command: `list.rs`, `behind.rs`, `consumers.rs`, `same_release.rs`, or `verify.rs`. Other shared lines are in `notes.rs` for the notes of a comparison, in `changes.rs` for the lines of a change, or in `mod.rs`. `unified.rs` holds the unified diffs of `--full`. |
| `src/candidate/` | The candidate file. `mod.rs` holds the format, `rules.rs` the check of the file, and `resolve.rs` the resolve of each layer. `pack.rs` finds the versions of a pack in the registries, and `presets.rs` holds the options of a pack template. `export.rs` reads the profile export of Palette as a candidate, and `gaps.rs` holds the rules that only the create of a profile version checks. |
| `src/registry.rs` | The registries of Palette: the listings, read one time for each command. The registry that a name or a uid names, the last sync of a registry, and the state that the listing gives a pack version. |
| `src/json.rs` | The reads of a document of Palette: text, arrays, the items of a listing, and the first text that is not empty. Each module reads a document with them. |
| `src/file.rs` | The read and the write of a file, with the path in each error. Each layer reads and writes its files with them. |
| `src/text.rs` | The texts that a message builds from a list of names, such as `a, b, and c`. |
| `src/config.rs`, `src/error/` | The settings, and the error kinds with their exit codes. `error/class.rs` holds the class of a failure for a retry, and `error/json.rs` the JSON of an error. |
| `src/term/` | The terminal: the color decision in `color.rs`, and the progress notes on standard error in `progress.rs`. |
| `src/content/` | The content that the hashes read. `canon.rs` holds the canonical form, and `layer.rs` the layers of Palette. `hash.rs` holds the content digest and the root of a record. |
| `src/reference/` | What a ref means. `version.rs` holds the version order. |
| `xtask/` | The development tools. `palette.rs` is the stand-in for the Palette API. `tenant.rs` and `chapters.rs` build the tenant of each chapter of the book. `book.rs` is the runner that writes `docs/src/output/`. `checks.rs` holds the rules that the pages must keep, and `files.rs` holds the scans of a directory. |
| `tests/` | The tests of the crate, in one crate: `main.rs` names each file as a module, and `common/mod.rs` holds each helper that two files use. `cargo test verify::` runs the tests of `verify.rs`. |
| `adapter/` | The adapter for knurl, a package of its own: see [The knurl adapter](../automation/knurl.md). |
| `preflight/` | `strata-preflight`, a package of its own in the workspace: the rule format (`rule.rs`), the catalog (`catalog.rs`, `rules/*.yaml`), the jq engine (`jq.rs`), the world (`world.rs`), the checks (`probe.rs`), the run and the watch (`run.rs`), the confidence and the ledger (`confidence.rs`, `evidence/runs.yaml`), the report (`report.rs`, `html.rs`), and the fixtures of its tests (`fixture.rs`, `rules/fixtures/`). See [How preflight works](../internals/preflight.md). |

`api/palette/tests.rs`, `candidate/tests.rs`, and `error/tests.rs` hold the unit
tests of their directory.

A new read of Palette goes in `api/palette/`. A new command gets a module in
`ops/`. A new field of a record goes in `snapshot/`, outside the root unless it
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
  for `gh` and `tofu`, and for a `jq` before 1.7.1
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

The run of the binary gives junk to each command that the help of strata lists,
except `preflight`, which runs another binary. Each case has a home of its own,
and no setting of the machine that names a project or a tenant. So a case
never reads the `~/.palette.env` of the person who runs the tests.

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
| lint and test | `just lint`, `just test`, `just library-example`, `just examples`, and `cargo test -p xtask` |
| docs | `just docs-check`, and `just docs` with `mdbook-lint` |
| build | the Linux musl binary, with the step that a release uses |
| the lowest Rust version | `cargo check --workspace --all-targets` with the `rust-version` of `Cargo.toml`, so the number a reader trusts is the number CI builds with |

CI does not run `just adapter-test`. The adapter needs knurl, a private
repository, and CI has no key for it: see
[The knurl adapter](../automation/knurl.md#test-it).

`ci` adds two jobs of its own. `Gate` needs the checks and does no other work:
it is the one check run that the verify reads (see
[How a change lands](./changes.md#the-verify)). `publish the book` needs the
checks and runs only on `main`, so the site is a build that every check passed.
The `docs` job builds the book once and passes it to the publish job, so no job
builds it twice.

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
