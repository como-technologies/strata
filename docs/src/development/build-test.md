# Build and test

```bash
just build             # target/release/strata and target/release/strata-preflight
just lint              # cargo fmt --check, clippy with -D warnings, rustdoc with -D warnings of the public and of all items
just test              # the unit, library, binary, and script tests, and the tests of xtask
just library-example   # examples/library and examples/library-preflight, crates that use the libraries
just examples          # examples/ from the tracked registry data
just docs-check        # docs/src/output/ is the output of this build
just docs-lint         # each link of the book resolves
just docs              # the book builds, and mdbook prints no warning
just bench             # the benchmarks of benches/, with the time of each
```

`just --list` shows each recipe.

`just lint` builds the rustdoc of `strata`, `strata-preflight`, and `xtask` two
times. It fails on a warning of each build. The first build documents the
public items, and it warns when a public item links to a private item. The
second build documents all items, so it reads the comments of the private items
too.

`just docs` runs `scripts/dev/build-book.sh`. mdbook exits with code `0` when
it prints a warning, so the script reads the log. It fails on a line that
starts with `WARN` or `ERROR`. A note of a preprocessor is not such a line. The
version note of `mdbook-mermaid` is an example, and it does not fail the build.

The crate is edition 2024 and needs Rust 1.88 or later. `rust-version` in
`Cargo.toml` says so, and the `msrv` job of CI builds with that version.

## Where the code goes

| Path | What it holds |
| --- | --- |
| `src/main.rs`, `src/cli/` | The command line, and nothing else. `main.rs` parses the arguments, runs the command, and gives the exit code. `cli/args.rs` holds the arguments, and `cli/<command>.rs` the run of each command. `cli/mod.rs` holds the parts that the commands share: the connection, the time, and the exit code of a comparison. |
| `src/lib.rs` | The library and its modules. |
| `src/api/` | Palette. `mod.rs` is the `PaletteApi` trait; `http.rs` sends it over HTTP, `retry.rs` tries it again, and `memory.rs` answers it from canned responses in tests. `raw.rs` holds the responses a snapshot is built from, and their directory on disk. `mask.rs` writes the mask over a secret value. `codes.rs` holds the codes of the answers of Palette that strata reads. `audit.rs` names the kinds of object in the audit log. |
| `src/api/palette/` | The reads of Palette. `profiles.rs` reads the profiles and the app profiles. `clusters.rs` reads the clusters, their events, the cloud accounts, the cluster groups, and the edge hosts. `listings.rs` reads the listings, the searches, and the audit log. `mod.rs` builds the routes and reads each page. |
| `src/ops/` | One module for each command: `verify.rs`, `same_release.rs`, `consumers/`, `behind.rs`, `list.rs`, and `packs.rs`. `consumers/app.rs` reads the consumers of an app profile, and `consumers/reads.rs` the reads beside the consumer list. `consumer_list.rs` reads the consumer list of a profile document. `fleet.rs` finds the clusters of a query of labels and writes a record of each; `repave.rs` asks Palette if a change repaves, and `validate.rs` asks Palette if it takes the packs. `bump.rs` asks the same routes about the bump of a pin for `behind`. None is a command of its own. `mod.rs` takes a snapshot and compares two. |
| `src/snapshot/` | A record: the document in `mod.rs`, the pack in `pack.rs`, the record of a cluster in `cluster.rs`, and the build from the responses in `build.rs`. `shape.rs` and `policy.rs` hold the fields of the `shape` tier and the `policy` tier of a cluster record. |
| `src/cloud.rs` | The cloud types of Palette as the closed set `Cloud`, and the rule for a pack that supports a cloud. |
| `src/cluster_state.rs` | The states of a cluster in `status.state`, as the closed set `ClusterState`. |
| `src/closed_set/` | The closed sets of a document. `mod.rs` holds the trait `ClosedSet`, its public macro `impl_closed_set`, and `Open`. `legend.rs` holds the legend of a document: the meaning of each value of a closed set, by its name in the documents. `strata-preflight` uses the macro and the legend for its own sets. |
| `src/pack_type.rs`, `src/pack_state.rs`, `src/oci_provider.rs`, `src/profile_type.rs`, `src/scope.rs`, `src/conjunction.rs` | The closed sets of a document. `pack_type.rs` and `pack_state.rs` hold the type and the state of a pack, and `oci_provider.rs` the provider type of an OCI registry. `profile_type.rs` holds the type of a profile, `scope.rs` the scope of a version, and `conjunction.rs` the join of a search. Each is one enum. `impl_closed_set` gives it `ALL` and `as_str`, and the trait `ClosedSet` gives `parse`. |
| `src/diff/` | The comparison of two snapshots. `mod.rs` holds the document and `compute`, and `leaf.rs` the comparison of a leaf. `pack.rs` compares the packs, `variables.rs` the variables, and `cluster.rs` the cluster records. `verify.rs` holds the verify report. `impact.rs` holds the impact of a change to a pack: the rules table and its levels. |
| `src/validation.rs` | The answer of Palette to a validate route, which a record and a comparison share. |
| `src/repave.rs` | The reasons of Palette for a repave, and a repave that waits for approval. |
| `src/cluster_template.rs` | The cluster templates of Palette, which link profile versions to clusters. |
| `src/render/` | The output: `terminal/`, `markdown.rs`, and `json.rs`. `terminal/lines.rs` paints the lines of a change for the terminal. `terminal/table.rs` holds the tables of the terminal: the width of each column, and the cells that match a query. A line that two renderers show is in the module of its command: `list.rs`, `behind.rs`, `consumers.rs`, `same_release.rs`, or `verify.rs`. Other shared lines are in `notes.rs` for the notes of a comparison, in `changes.rs` for the lines of a change, or in `mod.rs`. `unified.rs` holds the unified diffs of `--full`. |
| `src/candidate/` | The candidate file. `mod.rs` holds the format, `rules.rs` the check of the file, and `resolve.rs` the resolve of each layer. `pack.rs` finds the versions of a pack in the registries, and `presets.rs` holds the options of a pack template. `export.rs` reads the profile export of Palette as a candidate, and `gaps.rs` holds the rules that only the create of a profile version checks. |
| `src/registry.rs` | The registries of Palette: the listings, read one time for each command. The registry that a name or a uid names, the last sync of a registry, and the state that the listing gives a pack version. |
| `src/json.rs` | The reads of a document of Palette: text, arrays, flags, values, the items of a listing, and the first text that is not empty. Each module reads a document with them. The helpers that `preflight/` and `adapter/` use are public, so that those packages write no read of their own. |
| `src/file.rs` | The read and the write of a file, with the path in each error. Each layer reads and writes its files with them. |
| `src/text.rs` | The texts that a message builds. A list of names, such as `a, b, and c`. A text cut to a length. `-` for an empty text. A text in the cell of a markdown table. `strata-preflight` calls the public ones. |
| `src/config.rs`, `src/error/` | The settings, and the error kinds with their exit codes. `error/class.rs` holds the class of a failure for a retry, and `error/json.rs` the JSON of an error. |
| `src/term/` | The terminal: the color decision for a stream in `color.rs`, and the progress notes on standard error in `progress.rs`. `strata-preflight` uses the same decision. |
| `src/content/` | The content that the hashes read. `canon.rs` holds the canonical form, and `layer.rs` the layers of Palette. `hash.rs` holds the content digest and the root of a record. |
| `src/reference/` | What a ref means. `version.rs` holds the version order. |
| `xtask/` | The development tools. `palette.rs` is the stand-in for the Palette API. `tenant/` builds the tenant of each chapter of the book, with one file for each kind of object. `chapters/` holds the chapters, with one file for each group of pages. `book.rs` is the runner that writes `docs/src/output/`. `checks.rs` holds the rules that the pages must keep, and `files.rs` holds the scans of a directory. `size.rs` holds the tests of the size limits of [Write the code](./code.md#the-size-of-a-module). |
| `tests/` | The tests of the crate, in one crate: `main.rs` names each file as a module, and `common/mod.rs` holds each helper that two files use. `common/palette.rs` names the routes of Palette. The generator of `benches/` includes that file too, so the routes have one list. `cargo test verify::` runs the tests of `verify.rs`. |
| `benches/` | `strata-benches`, a package of its own in the workspace: see [The benchmarks](#the-benchmarks). `src/` holds the generator of the input, and each other `*.rs` file is a benchmark. |
| `adapter/` | The adapter for knurl, a package of its own: see [The knurl adapter](../automation/knurl.md). |
| `preflight/` | `strata-preflight`, a package of its own in the workspace: see [How preflight works](../internals/preflight.md). `rule.rs` holds the rule format, `catalog.rs` and `rules/*.yaml` the catalog, and `jq.rs` the jq engine. `world.rs` holds the world, `probe.rs` the checks, and `run.rs` the run and the watch. `confidence.rs` and `evidence/runs.yaml` hold the confidence and the ledger, and `fixture.rs` and `rules/fixtures/` the fixtures of the tests. `report/` holds the report, and `html.rs` the page. In `report/`, `mod.rs` holds the document, `scorecard.rs` the counts, `label.rs` the word, the color, and the class of a label, `terminal.rs` and `markdown.rs` the text forms, and `reproduce.rs` the lines to run by hand. `tests/closed_set.rs` of the package makes sure that each closed set of the rule format gives the text that serde writes. |

`api/palette/tests.rs`, `candidate/tests.rs`, and `error/tests.rs` hold the unit
tests of their directory.

A new read of Palette goes in `api/palette/`. A new command gets a module in
`ops/`. A new field of a record goes in `snapshot/`, outside the root unless it
is content: see [What the root does not cover](../internals/hashes.md#what-the-root-does-not-cover).

## Where a script goes

| Who runs it | Directory | Examples |
| --- | --- | --- |
| A user, in a pipeline | `scripts/ci/` | `install-strata.sh`, `merge-gate.sh`, `verify-report.sh` |
| A maintainer, for the repository | `scripts/dev/` | `build-book.sh`, `set-version.sh`, `tag-release.sh` |
| A maintainer, for one crate or one example | beside that crate or example | `examples/live/`, `adapter/scripts/` |

A script in `scripts/ci/` is a product: a page under Automation documents it, and
`tests/ci_scripts.rs` tests it. A script beside a crate or an example is a
harness for that one thing, and a `just` recipe runs it.

Each reusable workflow in `.github/workflows/` and each job of
`examples/ci/gitlab/strata.yml` wraps one script or one command. When that
script or command gets a new option, `tests/ci_scripts.rs` fails until the
table `WRAPPERS` names it. For each option, the table gives an input and a
variable, a value that the workflow sets, or the reason to omit it.
The same test file checks that a script passes the options of `verify` and
`behind` that a pipeline sets. `tests/workflows.rs` runs the `run` script of
each such step with a stand-in for the tool, because `act` is not in every
container. The expressions of GitHub (`${{ ... }}`) do not run there.

The development tooling is the `xtask` crate. A `just` recipe wraps each command
that the book and the examples need, and `cargo xtask <command>` runs the same
command directly. `cargo xtask` with no command prints its usage and exits `2`. The alias is in
`.cargo/config.toml`, and the crate reaches no release.

## The tests

`just test` runs four commands, one after the other: `cargo test`,
`cargo test -p strata-preflight`, `cargo test -p xtask`, and
`cargo test -p strata-benches`. The workspace builds only the root package by
default. So `cargo test` alone does not run the tests of `xtask`, which hold the
routes of the stand-in for Palette, and it does not run the tests of the
generator of the benchmarks. When one test fails,
`just test` stops and exits with a code that is not zero. CI runs `just test`, so
CI runs the tests of the stand-in with the others.

`cargo test` runs:

- the unit tests
- the library tests against `MemoryApi`, a Palette API in memory
- the tests of the binary over fixtures with the shape of the API responses
- the tests of the scripts in `scripts/` and `examples/live/`, with stand-ins
  for `gh` and `tofu`, and for a `jq` older than 1.7
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

## The benchmarks

`benches/` is the package `strata-benches`. Its benchmarks measure the time of the steps on the main
path of strata. The input has the size that Palette permits: 200 packs and 200 manifests in one
profile, and a fleet of up to 1000 clusters. The benchmarks measure time. They do not measure memory.

A benchmark is not a test. `just test` does not run it, and no check fails on a time.

```bash
just bench                       # each benchmark, with the default measure time of criterion
just bench canon_root/build/200  # the benchmarks whose name holds the text
just bench-short                 # a short measure time: the command of the CI job
```

`just bench` prints one block for each benchmark. The line `time:` gives three values for the time of
one run: the lowest estimate, the best estimate, and the highest estimate. The best estimate is the
one to read.

### What each benchmark measures

| File | Benchmark | What it measures |
| --- | --- | --- |
| `benches/record.rs` | `canon_root/build/1_packs`, `50_packs`, `200_packs` | The record of a profile of that many packs with no manifest, from the responses of Palette: the canonical form of the values of each pack, the hashes, and the root. |
| `benches/record.rs` | `canon_root/content_digest/200_packs` | The content digest of a record of 200 packs. |
| `benches/record.rs` | `snapshot/from_raw/200_packs_200_manifests` | The record of the largest profile that Palette takes, from the responses of Palette. |
| `benches/compare.rs` | `compare/identical`, `compare/one_change`, `compare/every_pack` | The comparison of two records of 200 packs and 200 manifests. The records are the same, or one pack moves to the next version, or each pack moves and each manifest changes. |
| `benches/render.rs` | `render/terminal/summary`, `render/terminal/full`, and the same for `markdown` and `json` | Each renderer on the comparison with a change in each pack, without `--full` and with it. The JSON renderer writes the same document in both cases. |
| `benches/fleet.rs` | `verify/1_clusters`, `100_clusters`, `1000_clusters` | `strata verify` of one record for each cluster, against an in-memory Palette. The time includes the read of each record file. |
| `benches/fleet.rs` | `consumers/1_clusters`, `100_clusters`, `1000_clusters` | `strata consumers` of a profile version that each cluster of the fleet runs. |
| `benches/resolve.rs` | `resolve/load/200_layers` | The read and the check of a candidate file of 200 layers. |
| `benches/resolve.rs` | `resolve/resolve/200_layers` | The resolve of those layers against the registry. |
| `benches/preflight.rs` | `preflight/full_catalog` | The jq engine of `strata-preflight` over the full catalog, for the entity of one create. The run has no Palette and no cloud, so a rule of another tier than `static` is not run. |

### The input

The generator is the library in `benches/src/`. It reads the real packs in `examples/packs/` and
tracks no large file of its own. It makes the same input on each run.

- A profile takes the five real packs in turn: the OS, Kubernetes, the network, the storage, and an
  add-on. After five packs, it takes the next version of each pack. So the values have the sizes and
  the shapes of real values.
- A manifest is a config map of 40 keys. The first packs of a profile hold the manifests.
- A change moves a pack to the next version of the same pack, with the values, the digest, and the
  uid of that version.
- A cluster of the verify fleet runs the OS, Kubernetes, and storage packs. A cluster of the
  consumer fleet states the profile that it runs, and no pack.
- Palette is `MemoryApi`. No benchmark uses the network.
- `tests/common/palette.rs` names the routes. The tests use the same file.

Each benchmark checks its input before it measures. A change of strata that makes the input fail
stops the benchmark with a panic, and not with the time of an error path.

### The times on the CI runner

The job `benchmarks` of CI runs `just bench-short` on a runner of GitHub, and prints the times in its
log. These are the best estimates for a profile of the sizes that Palette permits, rounded to two
digits:

| Step | Input | Time |
| --- | --- | --- |
| The record, with the canonical form and the root | 1 pack | 34 µs |
| The record, with the canonical form and the root | 50 packs | 29 ms |
| The record, with the canonical form and the root | 200 packs | 120 ms |
| The record, with 200 manifests | 200 packs | 130 ms |
| The comparison of two records | no change | 86 ms |
| The comparison of two records | one pack changes | 87 ms |
| The comparison of two records | each pack changes | 170 ms |

The time of a record grows with the size of the values, because strata parses and hashes each text.
The time of a comparison grows with the number of packs that differ. A shared runner is not a
quiet machine, so the times change from run to run. Use them to see the order of magnitude, and use
[a comparison of two commits](#compare-two-commits) to see a change.

When a benchmark is slow, criterion warns that it cannot complete 10 samples in the measure time. The
warning is not an error. The job fails only when a benchmark does not build or panics.

### Compare two commits

criterion keeps the results of a run in `target/criterion/`, under the name of a baseline. Save a
baseline at the first commit. Then run the benchmarks at the second commit against that baseline:

```bash
git switch --detach <first commit>
just bench --save-baseline first
git switch --detach <second commit>
just bench --baseline first
```

For each benchmark, criterion prints the change of the time, and says if the change is significant.
Do not run `cargo clean` between the two runs, because it removes the baseline. Run both on one
machine, with no other heavy work. The time of one machine does not compare with the time of
another machine. To save time, add `--warm-up-time 1 --measurement-time 3` to both commands.

## Continuous integration

`.github/workflows/checks.yml` holds every check. The `ci` workflow runs it on each push to
`main` and on each pull request, and the release workflow runs it before it builds a binary:

| Job | Runs |
| --- | --- |
| lint and test | `just lint`, `just test`, `just library-example`, and `just examples` |
| docs | `just docs-check`, `just docs-lint`, and `just docs`, which fails when mdbook prints a warning |
| build | the Linux musl binary, with the step that a release uses |
| the lowest Rust version | `cargo check --workspace --all-targets` with the `rust-version` of `Cargo.toml`, so the number a reader trusts is the number CI builds with |
| benchmarks | `just bench-short`: each benchmark of [The benchmarks](#the-benchmarks), with a short measure time |

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
cd ../strata-check && just lint && just test && just library-example && just examples && just docs-check && just docs-lint && just docs
```

## A change to how strata reads Palette

The fixtures follow the shapes of the Palette API, but only a real tenant proves
them. A change to cluster refs, consumers, candidates, or the API client also
needs [the live example](./live-example.md). It builds a billable cluster, so it
runs by hand and never in CI.
