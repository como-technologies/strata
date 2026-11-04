# Build and test

```bash
just build             # target/release/strata and target/release/strata-preflight
just observer-build    # target/observer/strata-observer, a static Go binary
just observer-contract # the documents of this build of strata go through the allow-list of the observer
just observer-dev      # the page of the observer, from test data: see The API of the observer
just observer-image    # strata-observer:dev, the image, from the static binary of this tree
just observer-manifests # kubectl kustomize deploy/observer holds four objects
just observer-playground # the observer against the stand-in, and its page at http://127.0.0.1:8080
just lint              # cargo fmt --check, clippy with -D warnings, rustdoc with -D warnings of the public and of all items, gofmt and go vet of the observer and of the request counter
just test              # the unit, library, binary, and script tests, the tests of xtask, and the Go tests of the observer
just library-example   # examples/library and examples/library-preflight, crates that use the libraries
just examples          # examples/ from the tracked registry data
just docs-check        # docs/src/output/ is the output of this build
just docs-lint         # each link of the book resolves
just docs              # the book builds, and mdbook prints no warning
just bench             # the benchmarks of benches/, with the time of each
```

`just --list` shows each recipe.

The observer needs Go. The line `go` in `observer/go.mod` names the version, and
the job `check` of CI installs that version from the file. `just lint` fails when
a line of `observer/go.mod` starts with `require`, because the observer uses the
standard library only.

`just observer-contract` gives the documents of the strata binary of this commit
to the allow-list of the observer. It builds `strata` and the stand-in. It
serves four chapters of the book one at a time, and runs the commands of the
observer against each one. Then it runs the contract tests of `internal/docs`
on the documents. One test fails when a field of the allow-list is not in the
schema of the binary. One test fails when the clear value of a cluster variable
passes the allow-list.

The script also builds `strata-observer` and runs `once` with the real binary of
`strata`. It needs `jq` for these checks. It makes sure of four facts:

- On the chapter `cluster-release`, `payments` has no finding, and `ledger` has
  one `cluster-differs` finding with the pack `csi/csi-aws-ebs`.
- On the second chapter of `edited-in-place`, a record of the first chapter
  gives one `published-changed` finding with `who`.
- On the chapter `cluster-values`, the value `team-payments` is in the output of
  `strata compare` and not in the output of `once`.
- With the stand-in stopped, two runs of `once` with one state directory give
  `page` `lost` and the error kind `transport`.

After each run, the script makes sure that the work directory is empty.

`just observer-playground` runs the observer on your machine against the stand-in,
and serves its page. It takes the records from the first state of the chapter
`observer`, before the edit in place. Then it serves the state that you name, and the
observer watches that one. The default is `observer-2`, with one finding and the
newer versions. `observer-1` is the tenant before the edit, with no finding.
`OBSERVER_CLUSTER` names another cluster, such as `ledger`.

`scripts/observer-lib.sh` holds the functions that start and stop the stand-in. The contract script and the
playground script both source it.

`just docs-examples` and `just docs-check` build `strata-observer` first, because a
step of the chapter `observer` runs it. The runner puts it on the `PATH` of a step.
The job `docs` of CI installs Go for this step.

`just observer-contract --write` also copies five documents to
`observer/internal/docs/testdata/`. They hold made-up data of the stand-in. Do
not edit them by hand. `go test` reads them, so it needs neither the binary nor
the network, and the contract tests skip. The job `check` of CI runs
`just observer-contract` after `just examples`.

`just lint` builds the rustdoc of `strata`, `strata-preflight`, `xtask`,
`strata-benches`, `strata-report-endpoint`, and `strata-fuzz` two times. It fails on a warning of each build. The first build
documents the public items, and it warns when a public item links to a private
item. The second build documents all items, so it reads the comments of the
private items too. The package `strata-fuzz` has code in tests only, and rustdoc
builds no test. So its crate is `#![cfg(any(test, doc))]` and its dependencies
are normal dependencies, and the build reads the comments of its tests.

`just docs` runs `scripts/dev/build-book.sh`. mdbook exits with code `0` when
it prints a warning, so the script reads the log. It fails on a line that
starts with `WARN` or `ERROR`. A note of a preprocessor is not such a line. The
version note of `mdbook-mermaid` is an example, and it does not fail the build.

The crate is edition 2024 and needs Rust 1.88 or later. `rust-version` in the
`[workspace.package]` table of `Cargo.toml` says so. Each package of the workspace takes it with
`rust-version.workspace = true`, and a test makes sure of this. The `msrv` job of CI builds with
that version.

## Where the code goes

| Path | What it holds |
| --- | --- |
| `src/main.rs`, `src/cli/` | The command line, and nothing else. `main.rs` parses the arguments, runs the command, and gives the exit code. `cli/args.rs` holds the arguments, and `cli/<command>.rs` the run of each command. `cli/mod.rs` holds the parts that the commands share: the connection, the time, and the exit code of a comparison. |
| `src/lib.rs` | The library and its modules. The root holds the helpers of the whole crate, and a re-export for each module that moved to `model/`. |
| `src/api/` | Palette. `mod.rs` is the `PaletteApi` trait; `http.rs` sends it over HTTP, `retry.rs` tries it again, and `memory.rs` answers it from canned responses in tests. `raw.rs` holds the responses a snapshot is built from, and their directory on disk. `mask.rs` writes the mask over a secret value. `codes.rs` holds the codes of the answers of Palette that strata reads. `audit.rs` names the kinds of object in the audit log. |
| `src/api/palette/` | The reads of Palette. `profiles.rs` reads the profiles and the app profiles. `clusters.rs` reads the clusters, their events, the cloud accounts, the cluster groups, and the edge hosts. `listings.rs` reads the listings, the searches, and the audit log. `mod.rs` builds the routes and reads each page. |
| `src/api/registry.rs` | The registries of Palette: the listings, read one time for each command. The registry that a name or a uid names, the last sync of a registry, and the state that the listing gives a pack version. |
| `src/ops/` | One module for each command: `verify.rs`, `same_release.rs`, `consumers/`, `behind.rs`, `list.rs`, `packs.rs`, and `report.rs`. `mod.rs` takes a snapshot and compares two. `fleet.rs` finds the clusters of a query of labels and writes a record of each. |
| `src/ops/behind/`, `src/ops/packs/` | `behind/bump.rs` is a child of `behind.rs`. It asks the validate routes about the bump of a pin, for `behind`. `packs/tests.rs` holds the tests of `packs.rs`. |
| `src/ops/consumers/`, `src/ops/consumer_list.rs` | `consumers/app.rs` reads the consumers of an app profile, and `consumers/reads.rs` the reads beside the consumer list. `consumer_list.rs` reads the consumer list of a profile document. |
| `src/ops/repave.rs`, `validate.rs`, `live.rs`, `source.rs` | None is a command of its own. `repave.rs` asks Palette if a change repaves. `validate.rs` asks Palette if it takes the packs. `live.rs` reads the object of a record as Palette holds it now, for `verify`. `source.rs` chooses what `snapshot` and `candidate` read from their refs and flags, before any request. |
| `src/snapshot/` | A record: the document in `mod.rs`, the pack in `pack.rs`, the record of a cluster in `cluster.rs`, and the build from the responses in `build.rs`. `shape.rs` and `policy.rs` hold the fields of the `shape` tier and the `policy` tier of a cluster record. |
| `src/model/` | The types of Palette. A type that is a fact of Palette, and that two modules share, goes here. The crate root holds none. |
| `src/model/cloud.rs` | The cloud types of Palette as the closed set `Cloud`, and the rule for a pack that supports a cloud. |
| `src/model/cluster_state.rs` | The states of a cluster in `status.state`, as the closed set `ClusterState`. |
| `src/model/pack_type.rs`, `pack_state.rs`, `oci_provider.rs`, `profile_type.rs`, `scope.rs`, `conjunction.rs` | The closed sets of a document. `pack_type.rs` and `pack_state.rs` hold the type and the state of a pack, and `oci_provider.rs` the provider type of an OCI registry. `profile_type.rs` holds the type of a profile, `scope.rs` the scope of a version, and `conjunction.rs` the join of a search. Each is one enum. `impl_closed_set` gives it `ALL` and `as_str`, and the trait `ClosedSet` gives `parse`. |
| `src/model/layer.rs` | The layers of Palette, in the order that Palette applies them. |
| `src/model/validation.rs` | The answer of Palette to a validate route, which a record and a comparison share. |
| `src/model/repave.rs` | The reasons of Palette for a repave, and a repave that waits for approval. |
| `src/model/cluster_template.rs` | The cluster templates of Palette, which link profile versions to clusters. |
| `src/closed_set/` | The closed sets of a document. `mod.rs` holds the trait `ClosedSet`, its public macro `impl_closed_set`, and `Open`. `legend.rs` holds the legend of a document: the meaning of each value of a closed set, by its name in the documents. `strata-preflight` uses the macro and the legend for its own sets. |
| `src/closed_set/unknown.rs`, `src/closed_set/unknown/` | `unknown.rs` holds what strata does with a value that a set does not name: the watch, the note, and the line of the log. `unknown/sighting.rs` holds the facts that a line and an entry both state, and the checks of them. `unknown/log.rs` holds the line and the read of the log. `unknown/report.rs` holds the report and its check, `unknown/hint.rs` the hint, and `unknown/send.rs` the send. |
| `src/diff/` | The comparison of two snapshots. `mod.rs` holds the document and `compute`, and `leaf.rs` the comparison of a leaf. `pack.rs` compares the packs, `variables.rs` the variables, and `cluster.rs` the cluster records. `verify.rs` holds the verify report. `impact.rs` holds the impact of a change to a pack: the rules table and its levels. |
| `src/render/` | The output: `terminal/`, `markdown.rs`, and `json.rs`. `terminal/lines.rs` paints the lines of a change for the terminal. `terminal/table.rs` holds the tables of the terminal: the width of each column, and the cells that match a query. A line that two renderers show is in the module of its command: `list.rs`, `behind.rs`, `consumers.rs`, `report.rs`, `same_release.rs`, or `verify.rs`. Other shared lines are in `notes.rs` for the notes of a comparison, in `changes.rs` for the lines of a change, or in `mod.rs`. `unified.rs` holds the unified diffs of `--full`. |
| `src/candidate/` | The candidate file. `mod.rs` holds the format, `rules.rs` the check of the file, and `resolve.rs` the resolve of each layer. `pack.rs` finds the versions of a pack in the registries, and `presets.rs` holds the options of a pack template. `export.rs` reads the profile export of Palette as a candidate, and `gaps.rs` holds the rules that only the create of a profile version checks. |
| `src/json.rs` | The reads of a document of Palette: text, arrays, flags, values, the items of a listing, and the first text that is not empty. Each module reads a document with them. The helpers that `preflight/` and `adapter/` use are public, so that those packages write no read of their own. |
| `src/file.rs` | The read and the write of a file, with the path in each error. Each layer reads and writes its files with them. |
| `src/text.rs` | The texts that a message builds. A list of names, such as `a, b, and c`. A text cut to a length, and `-` for an empty text. A text in the cell of a markdown table, and a text as a value in a query string. `strata-preflight` calls the public ones. |
| `src/config.rs`, `src/error/` | The settings, and the error kinds with their exit codes. `error/class.rs` holds the class of a failure for a retry, and `error/json.rs` the JSON of an error. |
| `src/term/` | The terminal. `color.rs` holds the color decision for a stream, and `progress.rs` the progress notes on standard error. `ask.rs` asks a question with a yes or no answer. `strata-preflight` uses the same decision. |
| `src/content/` | The content that the hashes read. `canon.rs` holds the canonical form. `hash.rs` holds the content digest and the root of a record. |
| `src/reference/` | What a ref means. `version.rs` holds the version order. |
| `xtask/src/palette.rs`, `xtask/src/palette/`, `xtask/src/tenant/` | The development tools. `palette.rs` is the stand-in for the Palette API, and `palette/report.rs` is the stand-in for the endpoint of the report. `tenant/` builds the tenant of each chapter of the book, with one file for each kind of object. |
| `xtask/src/chapters/`, `xtask/src/book.rs` | `chapters/` holds the chapters, with one file for each group of pages. `chapters/fields.rs` holds the documents that no page shows: see [The fields of the stand-in](./facts.md#the-fields-of-the-stand-in). `book.rs` is the runner that writes `docs/src/output/`. |
| `xtask/src/checks.rs`, `xtask/src/files.rs`, `xtask/src/size.rs` | `checks.rs` holds the rules that the pages must keep, and `files.rs` holds the scans of a directory. `size.rs` holds the tests of the size limits of [Write the code](./code.md#the-size-of-a-module), for `xtask`, `src/`, and the other packages. |
| `xtask/facts/`, `xtask/src/facts/`, `xtask/src/contract/` | `facts/` holds the list of the facts of Palette and of the clouds, and `src/facts/` holds the check of it: see [The facts of Palette](./facts.md). `src/contract/` holds the contract run. It compares the facts with a real tenant, and it sends the read-only calls of the clouds. See [The weekly contract run](./facts.md#the-weekly-contract-run). |
| `tests/` | The tests of the crate, in one crate: `main.rs` names each file as a module, and `common/mod.rs` holds each helper that two files use. `routes.rs` makes sure that the routes of the tests are the routes that strata builds. `cargo test verify::` runs the tests of `verify.rs`. |
| `tests/common/palette.rs`, `tests/common/answer.rs` | `common/palette.rs` names the routes of Palette and builds the documents of a profile: the pack ref, the pack manifests, and the variables. The generator of `benches/`, the package `fuzz/`, and the tests of `strata-preflight` include that file too, so the routes and the documents have one source. `common/answer.rs` holds the check of an answer and the check of a ref. `tests/fuzz.rs` and `fuzz/` use it, so one rule decides what an answer is. |
| `tests/common/closed_set.rs`, `tests/common/walk.rs`, `tests/common/process.rs` | `common/closed_set.rs` holds the checks of a closed set, and `common/walk.rs` holds the walk of a directory. `preflight/tests/` includes both. `common/process.rs` holds the lock for the processes of the tests: see [Write the code](./code.md#tests). The tests of `strata-preflight` and of `xtask` include it too. |
| `benches/` | `strata-benches`, a package of its own in the workspace: see [The benchmarks](#the-benchmarks). `src/` holds the generator of the input, and each other `*.rs` file is a benchmark. |
| `adapter/` | The adapter for knurl, a package of its own: see [The knurl adapter](../automation/knurl.md). |
| `observer/` | `strata-observer` is a Go program in its own module: see [The checks of the observer](./observer.md) and [The server of the observer](./observer-api.md). `main.go` holds the command line, and `main_test.go` holds its tests. `internal/docs`, `internal/runner`, `internal/baseline`, `internal/model`, `internal/check`, `internal/api`, and `internal/web` are its packages. `scripts/observer-contract.sh` gives them the documents of the binary, and `scripts/observer-playground.sh` runs the observer against the stand-in. `just observer-build` writes the static binary at `target/observer/strata-observer`. `just lint` and `just test` check the module. |
| `examples/live/count-requests/` | A small Go program in a module of its own, with the standard library only. It runs a command and sends each request of the command on to Palette. It writes one line for each request, with no query and no key. `examples/live/observer.sh` uses it to count the requests of a cycle of the observer. `just lint` and `just test` check the module. |
| `deploy/observer/`, `scripts/ci/observer-records.sh` | The manifests and the Dockerfile of the observer: see [The image and the manifests of the observer](./observer-deploy.md). The script writes the records of the observer as Secrets. `.github/workflows/observer-image.yml` builds and signs the image. `tests/observer_deploy.rs` ties the manifests to the code of the observer. |
| `preflight/` | `strata-preflight`, a package of its own in the workspace: see [How preflight works](../internals/preflight.md). `catalog.rs` and `rules/*.yaml` hold the catalog, and `jq.rs` holds the jq engine. `world/` holds the world. `confidence.rs` and `evidence/runs.yaml` hold the confidence and the ledger, and `fixture.rs` and `rules/fixtures/` the fixtures of the tests. |
| `preflight/src/rule/` | `rule/` holds the rule format: `mod.rs` holds the rule and the catalog, `check.rs` the four kinds of check, and `validate.rs` the checks of the loader. |
| `preflight/src/probe/` | `probe/` holds the checks. `mod.rs` runs a check, and `backend.rs` holds the backends. `palette.rs` and `exec.rs` run a check of Palette and of a command. `outcome.rs` holds the answer. |
| `preflight/src/run/` | `run/` holds the run: `mod.rs` holds the run, `watch.rs` the watch, `verbose.rs` the lines of `--verbose`, and `rules.rs` the listing of the rules. |
| `preflight/src/report/` | `report/` holds the report, and `html.rs` the page. In `report/`, `mod.rs` holds the document, `scorecard.rs` the counts, and `label.rs` the word, the color, and the class of a label. `terminal.rs` and `markdown.rs` hold the text forms, and `reproduce.rs` holds the lines to run by hand. |
| `preflight/tests/` | `tests/closed_set.rs` of the package makes sure that each closed set of the rule format gives the text that serde writes, with the checks of `tests/common/closed_set.rs`. |
| `report-endpoint/` | `strata-report-endpoint`, a package of its own in the workspace: the service that takes the report of the unknown values. `accept.rs` checks a request, and `limit.rs` limits the rate. `digest.rs` keeps the hash of each report that the service took, and `merge.rs` merges a report into the issues. `tracker.rs` writes to GitHub, and `server.rs` holds the rules and the loop. No release carries it: see [The endpoint of the report](./report-endpoint.md). |
| `fuzz/` | `strata-fuzz`, a package of its own in the workspace: the fuzz targets and the property tests of `strata`, `strata-preflight`, and `strata-report-endpoint`, see [The fuzzing](#the-fuzzing). `src/` holds one module for each part, such as `mask.rs`, and `support.rs` holds what the targets share. `src/__fuzz__/<module>__<test>/` holds the corpus and the crashes of one target. The package has no code outside its tests. |
| `fuzz/src/seed.rs`, `fuzz/src/seed/`, `fuzz/src/lib.rs` | `seed.rs` holds the table of the targets and their seed corpus. `seed/catalog.rs` holds the seeds that come from the catalog of `strata-preflight`, and `seed/regressions.rs` holds the regression cases. `lib.rs` includes `tests/common/palette.rs` and `tests/common/answer.rs` with `#[path]`, so the fuzz package has no second list of the routes and no second check of an answer. |

`api/palette/tests.rs`, `candidate/tests.rs`, and `error/tests.rs` hold the unit
tests of their directory. In `preflight/src/`, `probe/`, `report/`, `rule/`,
`run/`, and `world/` each have a `tests.rs` for the same job.

A new read of Palette goes in `api/palette/`. A new command gets a module in
`ops/`. A new field of a record goes in `snapshot/`, outside the root unless it
is content: see [What the root does not cover](../internals/hashes.md#what-the-root-does-not-cover).

## Where a script goes

| Who runs it | Directory | Examples |
| --- | --- | --- |
| A user, in a pipeline | `scripts/ci/` | `install-strata.sh`, `merge-gate.sh`, `verify-report.sh` |
| A maintainer, for the repository | `scripts/dev/` | `build-book.sh`, `fuzz.sh`, `set-version.sh`, `tag-release.sh` |
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

`just test` runs six commands, one after the other:

- `cargo test`
- `cargo test -p strata-preflight`
- `cargo test -p xtask`
- `cargo test -p strata-benches`
- `cargo test -p strata-fuzz`
- `cargo test -p strata-report-endpoint`

The workspace builds only the root package by default. So `cargo test` alone
does not run the tests of `xtask`, which hold the routes of the stand-in for
Palette. It does not run the tests of the generator of the benchmarks or the fuzz
targets either. When one test fails, `just test` stops and exits with a code that
is not zero. CI runs `just test`, so CI runs the tests of the stand-in, of the
benchmarks, and of the fuzz targets with the others.

`cargo test` runs:

- the unit tests
- the library tests against `MemoryApi`, a Palette API in memory
- the tests of the binary over fixtures with the shape of the API responses
- the tests of the scripts in `scripts/` and `examples/live/`, with stand-ins
  for `gh` and `tofu`, and for a `jq` older than 1.7
- the fuzzing in `tests/fuzz.rs`

`cargo test -p strata-fuzz` runs the fuzz targets and the property tests of
[The fuzzing](#the-fuzzing).

No test uses the network.

### The tests send no report

The release names the address of the endpoint of the report. A run of strata with the setting
`STRATA_REPORT=1`, or with a saved answer of yes, posts to that address. These rules stop it:

- A test starts strata with `strata_command` of `tests/common/mod.rs`, and gives the binary to a
  script with `give_strata`. Both set `STRATA_REPORT=0`, and set `STRATA_REPORT_URL` to an address
  that refuses a connection. The environment has priority over `~/.palette.env` and over a saved
  answer. `strata_bin` is private, and a test fails on a file that names the binary itself.
- A test that sends sets `STRATA_REPORT=1`, and sets `STRATA_REPORT_URL` to its stand-in endpoint.
- A test of the address that the release names removes both variables. It sends each request that
  leaves the machine to a stand-in proxy with `through_proxy`. The proxy refuses the request, so no
  address receives it, also when the code is wrong. The tests for `CI` and for `--offline` do this.
- A script of this repository that runs strata sets `STRATA_REPORT=0`, or it sources
  `examples/live/lib.sh`, which sets it. A test fails on a script that does neither. The runner of
  the book gives each step the address of its stand-in endpoint. The drop-ins in `scripts/ci/`
  and `examples/ci/` obey the settings of the user.

### The fuzzing

Two kinds of test send hostile input to strata. `tests/fuzz.rs` sends random
input from a generator with a fixed seed. The package `fuzz/` sends bytes that a
fuzzer chooses from the code that each input reaches. The first kind finds a
fault in a command. The second kind finds a fault deep in a parser.

#### The random input of `tests/fuzz.rs`

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

#### The targets of `fuzz/`

`fuzz/` is the package `strata-fuzz`. It holds one fuzz target for each parser
of text from a user or from Palette, and the property tests. A target is a test
that calls `bolero::check!`. [bolero](https://github.com/camshaft/bolero) runs it
with one of two engines:

- `cargo test` runs the target on stable Rust. It reads each file in the
  `crashes` and `corpus` directories of the target, and then it tries random
  bytes for one second.
- `cargo bolero` runs the same test under libFuzzer on nightly Rust. libFuzzer
  sees which code an input reaches. It changes the inputs that reach new code.
  So it finds the input that passes the next check of a parser.

So `just test` runs each target on its corpus on every push, and a scheduled job
fuzzes each target for a fixed time. A target calls the public API of `strata`,
`strata-preflight`, and `strata-report-endpoint`. The API does not grow for a target. A private parser,
such as the mask, runs through the public read that calls it.

The first column of each table is the name of the test, with the module of
`fuzz/src/` before it. A target reads bytes. A target that reads text skips an
input that is not UTF-8. An error with a kind is an answer of strata, and it is
not a fault. A panic is the only fault.

| Target | Input | What the target makes sure of |
| --- | --- | --- |
| `candidate::a_candidate_file_loads_as_the_candidate_it_writes_or_is_refused_with_a_kind` | A candidate file | The parse and the check give a candidate or an error with a kind. The YAML of a candidate that loads loads as the same candidate. |
| `canon::any_text_has_a_stable_canonical_form_as_values_and_as_a_manifest` | YAML text | The text is the values and the manifest of a pack. The record is the same each time. The canonical text of the values is YAML, and it gives the same hash when strata reads it again. A key of more than 1024 characters is the exception: YAML cannot read it again. |
| `mask::a_draft_pack_keeps_no_secret_in_clear_and_nothing_else_changes` | YAML text | The read of a draft masks the text. Each secret that a schema names has the mask, and no other value changes. A second mask changes nothing. |
| `mask::a_saved_cluster_keeps_no_password_of_its_group_values_in_clear` | YAML text, or JSON | `RawCluster::save_dir` masks each password in the values of a cluster group, also under a key that is not text. The directory loads again, and a second save changes nothing. |
| `reference::the_text_of_a_ref_parses_as_a_ref_that_prints_the_same_text` | A ref | The text of a ref parses as a ref, and the text that the ref prints parses as the same ref. |
| `reference::the_version_order_is_a_total_preorder` | Three versions, apart by a NUL byte | The version order is reflexive, reverses with its arguments, and is transitive. |
| `record::a_record_file_holds_what_each_record_holds_or_is_refused_with_a_kind` | A record file | The read gives a record or an error with a kind. A record writes and reads again with the same root. It compares equal to the record that strata read, in each part of the comparison and in each renderer. |
| `record::a_comparison_writes_the_document_it_reads` | A `strata-diff/1` document | Strata reads the document or refuses it with a kind, and writes the same document again. |
| `record::a_raw_directory_of_a_profile_builds_a_record_or_is_refused_with_a_kind` | The files of a raw directory of a profile, in the order of their names, apart by a NUL byte | The directory loads and builds a record, or it gives an error with a kind. |
| `record::a_raw_directory_of_a_cluster_builds_a_record_or_is_refused_with_a_kind` | The files of a raw directory of a cluster, in the order of their names, apart by a NUL byte | The same, for each profile that the cluster applies. |
| `palette::an_answer_of_palette_of_any_shape_gives_a_record_or_an_error_with_a_kind` | A JSON document | Each request answers with the document. Each ref, each listing, and each read of one object gives a record or an error with a kind. |
| `palette::an_error_of_palette_is_read_as_a_refusal_or_gives_an_error_with_a_kind` | The status of an HTTP error in two bytes, and its body as text | Each request answers with the error. No read gives a record, and no `--validate` question accepts. Each validate route reads the body as its table of codes says. The class of the error follows the rule of the retry. |
| `config::a_lookup_of_settings_gives_a_config_that_hides_its_key_or_is_refused_with_a_kind` | Lines of `NAME=value` | The lines are the lookup of `Config::from_lookup`. It gives a config or an error with a kind. The key is the first key of the lines, the address has no trailing slash, and the debug form shows no key. The reader of the file `~/.palette.env` is private and reads the home of the person, so no target runs it. |
| `report::a_request_body_is_refused_with_a_status_or_is_a_report_that_writes_as_itself` | The body of a request to the endpoint of the report | The check gives a report or a refusal with the status 400, 413, 415, or 422. A report that it takes passes the check of the library, and it writes as a body that gives the same report. |
| `preflight::a_rule_file_loads_as_the_section_it_writes_or_is_refused_with_a_kind` | A rule file | The file loads or gives an error with a kind. A section that loads writes as YAML that loads as the same section. |
| `preflight::a_jq_program_compiles_and_runs_or_is_refused_with_a_kind` | A program, a NUL byte, and a JSON input | The program compiles with and without `$item`, and it reads as a template. It runs when it has no word that loops, because jq has no step limit. |
| `preflight::a_world_file_gives_the_document_of_a_run_or_is_refused_with_a_kind` | A world file, as a fixture and as the body of a create | The file loads or gives an error with a kind. A world gives the document of a run for a new cluster and for an existing cluster. An existing cluster has an error exactly when its entity, or its `metadata`, is neither an object nor `null`. |

The five properties hold for any input. A decoder builds the document of each
test from the bytes: a tree of YAML values, or a version. So the document is
always valid, and the same bytes give the same document.

| Property | The test |
| --- | --- |
| The root does not change when the order of the keys changes. | `properties::the_root_does_not_change_when_the_order_of_the_keys_changes` |
| The comparison of two records of one profile is empty. | `properties::the_comparison_of_two_records_of_one_profile_is_empty` |
| The canonical form and the mask are idempotent. | `properties::the_canonical_form_is_idempotent` and `properties::the_mask_is_idempotent_and_leaves_no_secret_in_clear` |
| The version order is a total preorder. | `properties::the_version_order_is_a_total_preorder` |
| A record written and read again gives the same root. | `properties::a_record_written_and_read_again_has_the_same_root` |

The version order is a total preorder: two spellings of one version, such as
`1.0.1` and `1.0.1+2`, compare equal.

#### Run a target

`cargo bolero` needs a nightly toolchain and the `cargo-bolero` command:

```bash
cargo install cargo-bolero --version 0.13.5 --locked
rustup toolchain install nightly
```

```bash
just fuzz-list                              # the name of each target and property test
just fuzz reference::the_text_of_a_ref 10m       # one target, for 10 minutes
FUZZ_TOOLCHAIN=nightly-2026-09-01 just fuzz mask::a_draft_pack
```

The first argument of `just fuzz` is the name of a test, or the start of one
name. The recipe stops with the list of names when two tests start with the
argument. The second argument is a duration, and the default is 60 seconds. The
script `scripts/dev/fuzz.sh` runs `cargo bolero test` with the `fuzz` profile of
`Cargo.toml`, and a limit of 64 KiB for an input.

A run that finds nothing exits with `0`. Run the same test with `cargo test` to
see the corpus alone:

```bash
cargo test -p strata-fuzz reference::
```

The random bytes of `cargo test` are different on each run. A failure prints
`BOLERO_RANDOM_SEED=<n>`, and the same variable repeats the run:

```bash
BOLERO_RANDOM_SEED=183116997329908360160763470704180534434 cargo test -p strata-fuzz palette::
```

#### The corpus

A target has up to two directories in `fuzz/src/__fuzz__/<module>__<test>/`.
bolero reads them, and nothing else:

- `corpus/` holds the inputs that a fuzzer starts from. The fuzzer adds the
  inputs that reach new code.
- `crashes/` holds a regression case: an input that made the target fail.
  `cargo test` runs it first. A target that never failed has no `crashes/`
  directory, because Git keeps no empty directory.

`just fuzz-seed` writes the seeds into `corpus/`. A seed is a real input. It is a
file of `examples/`, a rule of the catalog of `strata-preflight`, or a file or an
output of this book. It can also be a record that the library builds from them.

The name of a seed file is a hash of its bytes, so a second run changes no file.
The recipe runs
`just examples` first, because it reads the profiles and the cluster that
`just examples` builds. `.gitattributes` marks the corpus as `-text`, so Git
keeps each byte of a seed, also a line ending.

Two tests keep the seeds useful. The first makes sure that each target is a test
of its module. The second runs each seed. At least one seed in three must reach
the code of its target, and each regression case must pass. A change of a schema
id that makes the seeds useless fails the second test, with the name of the
target.

`cargo bolero reduce` removes the inputs of `corpus/` that add no coverage. A
maintainer runs it for a target, and commits the files that remain. Do not commit
the whole corpus that a long run grows. A small corpus keeps `just test` fast.

#### A crash

libFuzzer writes the input that fails to `crashes/`, and the command exits with a
code that is not zero. A crash is a bug of strata, unless the check of the target
is wrong. Make the fix in four steps:

1. Run `cargo test -p strata-fuzz`. The file in `crashes/` makes the target fail.
2. Fix strata, or the check of the target.
3. Write a unit test that fails without the fix. [Write the code](./code.md#tests)
   gives the rule.
4. Keep the file in `crashes/`, with a name that says what it found.

The tracked regression case of the mask is an example. Its file is
`wave-3-mask-slices-a-char`. The mask of a draft cut the values of a user at a
byte that is not the start of a `char`. The text `{{ .spectroévar.x }}` showed
it. The test `a_draft_value_with_a_non_ascii_variable_name_is_masked_and_does_not_panic`
in `fuzz/src/mask.rs` states the same case in words.

#### The scheduled run

`.github/workflows/fuzz.yml` runs each target of `just fuzz-list` once a week, on
Monday, and on demand. A person starts it from the Actions page of the
repository with the input `time`, and the default is `10m`. Each target is a job
of its own, on nightly Rust.

A crash fails its job. The log shows the crash as base64 text, and the run
summary has the same text with the steps to repeat it. The artifact `new-files-<n>`
of the job holds the crash and the inputs that reached new code. No pull request
runs this workflow. It needs a nightly toolchain and ten minutes for each
target.

#### Add a target

1. Write a function in a module of `fuzz/src/`. It takes `&[u8]` and returns
   `true` when the input reached the code that the target fuzzes.
2. Write a test in the same module with a name that is a sentence. The test calls
   `bolero::check!().for_each` and your function.
3. Add the target to the table `TARGETS` of `fuzz/src/seed.rs` with a function
   that returns its seeds.
4. Run `just fuzz-seed`, and then `just test`.
5. Add the target to the table of this section.

A property test does not read a corpus. Write it in `fuzz/src/properties.rs` with
the decoder of that file.

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
| `benches/record.rs` | `canon_root/build/1_packs`, `50_packs`, `200_packs` | The record of a profile of that many packs with no manifest, from the responses of Palette. It covers the canonical form of the values of each pack, the hashes, and the root. |
| `benches/record.rs` | `canon_root/content_digest/200_packs` | The content digest of a record of 200 packs. |
| `benches/record.rs` | `snapshot/from_raw/200_packs_200_manifests` | The record of the largest profile that Palette takes, from the responses of Palette. |
| `benches/compare.rs` | `compare/identical`, `compare/one_change`, `compare/every_pack` | The comparison of two records of 200 packs and 200 manifests. The records are the same, or one pack moves to the next version, or each pack moves and each manifest changes. |
| `benches/render.rs` | `render/terminal/summary`, `render/terminal/full`, and the same for `markdown` and `json` | Each renderer on the comparison with a change in each pack, without `--full` and with it. The JSON renderer writes the same document in both cases. |
| `benches/fleet.rs` | `verify/1_clusters`, `100_clusters`, `1000_clusters` | `strata verify` of one record for each cluster, against an in-memory Palette. The time includes the read of each record file. |
| `benches/fleet.rs` | `consumers/1_clusters`, `100_clusters`, `1000_clusters` | `strata consumers` of a profile version that each cluster of the fleet runs. |
| `benches/resolve.rs` | `resolve/load/200_layers` | The read and the check of a candidate file of 200 layers. |
| `benches/resolve.rs` | `resolve/resolve/200_layers` | The resolve of those layers against the registry. |
| `benches/preflight.rs` | `preflight/full_catalog` | The jq engine of `strata-preflight` over the full catalog, for the entity of one create. The run has no Palette and no cloud, so the benchmark runs only the rules of the tier `static`. |

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
- `tests/common/palette.rs` names the routes. The tests and the fuzz package use the same file, and so do the tests of `strata-preflight`.

Each benchmark checks its input before it measures. A change of strata that makes the input fail
stops the benchmark with a panic, and not with the time of an error path.

### The times on the CI runner

The job `benchmarks` of CI runs `just bench-short` on a runner of GitHub, and prints the times in its
log. The runner is a shared machine, and the speed of one runner differs from the speed of another.
So the table gives a range: the lowest and the highest best estimate of three runs of the job,
rounded to two digits. The input is the profile of the size that Palette permits.

| Step | Input | Time |
| --- | --- | --- |
| The record, with the canonical form and the root | 1 pack | 32 to 60 µs |
| The record, with the canonical form and the root | 50 packs | 29 to 49 ms |
| The record, with the canonical form and the root | 200 packs | 120 to 200 ms |
| The record, with 200 manifests | 200 packs | 130 to 220 ms |
| The comparison of two records | no change | 85 to 130 ms |
| The comparison of two records | one pack changes | 87 to 130 ms |
| The comparison of two records | each pack changes | 170 to 260 ms |

The time of a record grows with the size of the values, because strata parses and hashes each text.
The time of a comparison grows with the number of packs that differ. Use the table to see the order
of magnitude. To see a change of the code, use [a comparison of two commits](#compare-two-commits)
on one machine.

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
`main` and on each pull request. The release workflow runs it before it builds a binary:

| Job | Runs |
| --- | --- |
| lint and test | `just lint`, `just test`, `just library-example`, `just examples`, and `just observer-contract` |
| docs | `just docs-check`, `just docs-lint`, and `just docs`, which fails when mdbook prints a warning |
| build | the Linux musl binary, with the step that a release uses |
| the lowest Rust version | `cargo check --workspace --all-targets` with the `rust-version` of the workspace in `Cargo.toml`, so the number a reader trusts is the number CI builds with |
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
a release does not have. A release that called a workflow that asked for it
failed before it started.

Run the same recipes in a clean checkout before you push. A file that is not
tracked can hide a failure:

```bash
git worktree add --detach ../strata-check HEAD
cd ../strata-check && just lint && just test && just library-example && just examples && just docs-check && just docs-lint && just docs && just observer-contract
```

## A change to how strata reads Palette

The fixtures follow the shapes of the Palette API, but only a real tenant proves
them. A change to cluster refs, consumers, candidates, or the API client also
needs [the live example](./live-example.md). It builds a billable cluster, so it
runs by hand and never in CI.
