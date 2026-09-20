# Local entry point; CI runs the same recipes. Run `just --list` to see each recipe.
#
# `just --list` prints the LAST comment line before a recipe as its help. Put an explanation
# above a blank line, and one help line directly above the recipe.

# The OpenTofu version that the live example runs
tofu_version := env_var_or_default("TOFU_VERSION", "1.12.6")

# Build the release binary
default: build

# ---- build and test -------------------------------------------------------------------------

# Build the release binary at target/release/strata
build:
    cargo build --release

# Linux targets are static musl builds in the containers of cross. The other targets build
# natively with cargo.

# Build the release binary for one target at target/<target>/release/strata
build-target target:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{target}}" in
      *linux*) cross build --release --target "{{target}}" ;;
      *) cargo build --release --target "{{target}}" ;;
    esac

# Run the unit, library, binary, and script tests. No network.
test: stand-in
    cargo test

# Test the format, the clippy lints, and the rustdoc
lint:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

# Build and run examples/library, a separate crate that uses strata as a library
library-example:
    cargo run --quiet --manifest-path examples/library/Cargo.toml --target-dir target/library-example

# ---- examples -------------------------------------------------------------------------------

# Build examples/ from the tracked registry responses. Needs no API key.
examples: build
    OFFLINE=1 STRATA_BIN=target/release/strata examples/fetch-examples.sh

# Fetch the packs from the Public Repo again and build examples/. Needs PALETTE_API_KEY.
examples-fetch: build
    STRATA_BIN=target/release/strata examples/fetch-examples.sh

# ---- documentation --------------------------------------------------------------------------
# The book is docs/src/. It needs mdbook, mdbook-mermaid, and mdbook-gruvbox. docs/book/ is the
# output, and it is not tracked.

# Build the book into docs/book/
docs:
    mdbook build docs

# Build the book and serve it at http://localhost:3000 with live reload
docs-serve:
    mdbook serve docs --open

# xtask runs each command of the book against a local stand-in for Palette. Needs jq, which the
# book's own commands use.

# Write the output of each command that the book shows into docs/src/output/
docs-examples: examples stand-in
    cargo run --quiet -p xtask -- docs-examples

# Build the development tooling: the stand-in for Palette, and the runner for the book
stand-in:
    cargo build --quiet -p xtask

# Fail when docs/src/output/ is not the output of this build of strata
docs-check: examples stand-in
    cargo run --quiet -p xtask -- docs-examples --check
    docs/examples/check-pages.sh

# Test each link of the book with mdbook-lint
docs-lint:
    mdbook-lint lint docs/src/

# Install the mermaid and gruvbox theme files into docs/ again
docs-theme:
    mdbook-mermaid install docs
    mdbook-gruvbox install docs

# ---- live example ---------------------------------------------------------------------------
# A real cluster and profile versions in a Palette tenant. See docs/src/development/live-example.md.
# Every recipe that creates something is billable until live-down.

# Install the pinned OpenTofu into ~/.local/bin. Needs no root.
tofu-install:
    TOFU_VERSION={{tofu_version}} examples/live/tofu-install.sh

# Remove the OpenTofu that tofu-install put in ~/.local/bin
tofu-uninstall:
    TOFU_VERSION={{tofu_version}} examples/live/tofu-install.sh --uninstall

# When the apply fails, examples/live/diagnose.sh saves what holds each cluster.

# Build the live example: a cluster and the profile versions. Billable until live-down.
live-up:
    #!/usr/bin/env bash
    set -uo pipefail
    examples/live/tf.sh init -input=false || exit 1
    examples/live/tf.sh apply && exit 0
    examples/live/diagnose.sh
    exit 1

# Build the profiles of the live example and no cluster: seconds, and no cost
live-profiles:
    #!/usr/bin/env bash
    set -uo pipefail
    examples/live/tf.sh init -input=false || exit 1
    examples/live/tf.sh apply -var cluster=false

# Serve the tenant of one chapter of the book on this machine: no key, no tenant, no cost
playground chapter="list": build examples stand-in
    cargo run --quiet -p xtask -- playground {{chapter}}

# Run every read-only live check and write a summary
live-verify: build
    examples/live/verify.sh

# The pack ref checks against the tenant's registries: read-only, no cluster, no cost (#55)
live-packs: build
    examples/live/packs.sh

# The merge gate over records of the live profiles: needs live-profiles, no cluster, no cost (#57)
live-merge-gate: build
    examples/live/merge-gate.sh

# The scripts run in order from the baseline: verify.sh, in-place.sh off, in-place.sh on (against
# that off run), and registry-move.sh. The recipe stops at the first script that fails. Billable:
# about 3 to 4 hours, with cluster B two times.

# Run the four live scripts under one runs/<UTC time>/ directory
live-session: build
    #!/usr/bin/env bash
    set -uo pipefail
    session=examples/live/runs/$(date -u +%Y%m%dT%H%M%SZ)
    OUT=$session examples/live/verify.sh || exit 1
    OUT=$session/in-place/off examples/live/in-place.sh off || exit 1
    OUT=$session/in-place/on OFF_RUN=$PWD/$session/in-place/off examples/live/in-place.sh on || exit 1
    OUT=$session/registry-move examples/live/registry-move.sh || exit 1
    echo "live-session: all four passed; results in $session"

# The test opens and closes issues with the label strata-live-drift in this repository.

# Run the drift report test (#25) with the flag off and then on
live-drift-report: build
    #!/usr/bin/env bash
    set -uo pipefail
    session=examples/live/runs/$(date -u +%Y%m%dT%H%M%SZ)
    OUT=$session/drift-report/off examples/live/drift-report.sh off || exit 1
    OUT=$session/drift-report/on examples/live/drift-report.sh on || exit 1
    echo "live-drift-report: both passed; results in $session"

# The recipe also deletes the versions that skip_destroy kept out of the OpenTofu state. When the
# destroy fails, examples/live/diagnose.sh saves what holds each cluster.

# Remove the live example and every version of each profile name that it uses
live-down:
    #!/usr/bin/env bash
    set -uo pipefail
    outputs=$(examples/live/tf.sh output -json) || { echo "live-down: no tofu outputs" >&2; exit 1; }
    export PALETTE_PROJECT_UID=$(jq -r '.project_uid.value' <<<"$outputs")
    names=$(jq -r '.profile_names.value[]' <<<"$outputs")
    examples/live/tf.sh destroy || { examples/live/diagnose.sh; exit 1; }
    examples/live/api.sh delete-named $names

# ---- release --------------------------------------------------------------------------------

# The recipe refuses a dirty tree or a tag that exists. Push the tag to start the release workflow.

# Set the version everywhere it is written, for example: just version 1.2.3
version VERSION:
    scripts/dev/set-version.sh {{VERSION}}

# Tag the version in Cargo.toml as v<version>
release:
    scripts/dev/tag-release.sh

# Open or update one GitHub issue for each section of docs/backlog.md. Needs gh.
backlog-issues:
    scripts/dev/open-backlog-issues.sh
