# Build and test

```bash
just build             # target/release/strata
just lint              # cargo fmt --check, clippy with -D warnings, rustdoc with -D warnings
just test              # the unit, library, binary, and script tests
just library-example   # examples/library, a crate that uses strata as a library
just examples          # examples/ from the tracked registry data
just docs-check        # docs/src/output/ is the output of this build
```

`just --list` shows each recipe.

The development tooling is the `xtask` crate. A `just` recipe wraps each command
that the book and the examples need, and `cargo xtask <command>` runs the same
command directly. `cargo xtask` with no command prints the list. The alias is in
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
ref, a file, a directory, or an answer from Palette: refs of punctuation and
text that is not ASCII, records of junk bytes, a path that names a directory,
API answers of the right keys with the wrong types, and the binary itself with
random arguments against an address that refuses the connection.

One property holds everywhere: strata answers, or it errors with a kind and
exit `2`. It never panics.

The generator is deterministic, so a failure repeats. Each case names its seed,
and a seed runs alone:

```bash
STRATA_FUZZ_SEED=1337 cargo test --test fuzz
```

The default seed is fixed, so each run of CI fuzzes the same cases. Give a new
seed to look for more.

One test measures itself, because a fuzz test that stops reaching the code it
fuzzes still passes. `a_profile_of_junk_gives_a_record_that_round_trips`
fails when fewer than 50 of its 2000 cases build a record.

## Continuous integration

`.github/workflows/checks.yml` holds every check. The `ci` workflow runs it on each push to
`main` and on each pull request, and the release workflow runs it before it builds a binary:

| Job | Runs |
| --- | --- |
| lint and test | `just lint`, `just test`, `just library-example`, and `just examples` |
| docs | `just docs-check`, and `just docs` with `mdbook-lint` |
| build | the Linux musl binary, with the step that a release uses |

`ci` adds one job of its own, `publish the book`, which needs the checks and runs
only on `main`, so the site is a build that every check passed. The `docs` job
builds the book once and hands it over, so no job builds it twice.

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
