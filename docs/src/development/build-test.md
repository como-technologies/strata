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

## The tests

`cargo test` runs:

- the unit tests
- the library tests against `MemoryApi`, a Palette API in memory
- the tests of the binary over fixtures with the shape of the API responses
- the tests of the scripts in `scripts/` and `examples/live/`, with stand-ins
  for `gh` and `tofu`

No test uses the network.

## Continuous integration

`.github/workflows/ci.yml` runs on each push to `main` and on each pull request:

| Job | Runs |
| --- | --- |
| lint and test | `just lint`, `just test`, `just library-example`, and `just examples` |
| docs | `just docs-check`, and `just docs` with `mdbook-lint` |
| build | the Linux musl binary, with the step that a release uses |

Run the same recipes in a clean checkout before you push. A file that is not
tracked can hide a failure:

```bash
git worktree add --detach ../strata-check HEAD
cd ../strata-check && just lint && just test && just library-example && just examples && just docs-check
```

## The backlog

`docs/backlog.md` holds one section for each GitHub issue, and it is the source
of each issue body. `just backlog-issues` opens the issues that do not exist, and
updates the body of each issue that does, by title.

## A change to how strata reads Palette

The fixtures follow the shapes of the Palette API, but only a real tenant proves
them. A change to cluster refs, consumers, candidates, or the API client also
needs [the live example](./live-example.md). It builds a billable cluster, so it
runs by hand and never in CI.
