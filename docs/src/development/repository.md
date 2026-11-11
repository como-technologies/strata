# Where to look in the repository

This page tells where each part of strata is. Find your task in
[A task and its place](#a-task-and-its-place).
[Where the code goes](./build-test.md#where-the-code-goes) gives each file.

## The folders

The root package, `strata-palette`, is the strata app. Each other part has a folder of its
own.

| Folder | What it holds |
| --- | --- |
| `src/` | The strata app: the command line and the library. |
| `tests/` | The tests of the app. They are one crate, `tests/main.rs`. |
| `crates/strata-preflight/` | The checks of build readiness. `strata preflight` runs this binary. |
| `crates/strata-report-endpoint/` | The service on Cloud Run that takes the reports of unknown values. |
| `crates/strata-knurl/` | The adapter for knurl. It builds on its own, with `just adapter-test`. |
| `observer/` | The observer: a Go program that shows the checks on a page in a cluster. |
| `xtask/` | The stand-in for Palette, the runner of the book, and the contract run. |
| `fuzz/` | The fuzz targets and the property tests. |
| `benches/` | The benchmarks. |
| `examples/` | The offline examples, the library examples, the files for CI, and the live example. |
| `docs/` | This book. |
| `deploy/` | The files that run a part: the observer on Kubernetes, the endpoint on Cloud Run, the timer of the contract run, and the image of a riff session. |
| `scripts/ci/` | The scripts that a pipeline runs. |
| `scripts/dev/` | The scripts that a maintainer runs. |
| `.github/workflows/` | CI, the release, and the workflows that other repositories call. |

A release carries the binaries `strata` and `strata-preflight`, the image of
the observer, and the scripts for a pipeline from `scripts/ci/`. The other
folders are for the development of strata.

Each tool reads its configuration from the root: `Cargo.toml`, `justfile`,
`clippy.toml`, `riff.toml`, and `.mdbook-lint.toml`.

## A task and its place

| To do this | Look here |
| --- | --- |
| Change the options of a command | `src/cli/` |
| Change what a command does | `src/ops/` |
| Add a read of Palette | `src/api/palette/` |
| Add a field to a record | `src/snapshot/` |
| Change a comparison | `src/diff/` |
| Change the text or the JSON output | `src/render/` |
| Add a test of the app | `tests/` |
| Add or change a rule of preflight | `crates/strata-preflight/rules/` |
| Change the page of the observer | `observer/internal/web/` |
| Give the stand-in a new answer of Palette | `xtask/src/palette.rs` |
| Add an output to the book | `xtask/src/chapters/`, then `just docs-examples` |
| Add a fuzz target or a benchmark | `fuzz/src/` or `benches/` |
| Change a check of CI | `.github/workflows/` and `scripts/ci/` |
| Find a recipe | `just --list` |

## Where a new part goes

- A new module of the app goes in `src/`. See
  [Where the code goes](./build-test.md#where-the-code-goes).
- A new Rust package that a user runs goes in `crates/`. Its folder has the
  name of the package.
- A new tool for the tests or the build goes in `xtask/`, `fuzz/`, or
  `benches/`.
- Code in another language goes in a folder of its own at the root, as
  `observer/` does.
