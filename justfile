# Local entry point; CI runs the same recipes.

# The OpenTofu version that the live example runs
tofu_version := env_var_or_default("TOFU_VERSION", "1.12.6")

default: build

# Release binary at target/release/strata
build:
    cargo build --release

# Release binary for one target at target/<target>/release/strata. Linux targets are static
# musl builds made in cross's containers; the others build natively with cargo.
build-target target:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{target}}" in
      *linux*) cross build --release --target "{{target}}" ;;
      *) cargo build --release --target "{{target}}" ;;
    esac

# Tag the version in Cargo.toml as v<version>; refuses on a dirty tree or an existing tag.
# Push the tag to start the release workflow.
release:
    scripts/tag-release.sh

# Rust unit + integration tests
test:
    cargo test

lint:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

# Build and run examples/library: a separate crate that uses strata as a library
library-example:
    cargo run --quiet --manifest-path examples/library/Cargo.toml --target-dir target/library-example

# Rebuild examples/ from the cached registry responses (no API key needed)
examples: build
    OFFLINE=1 STRATA_BIN=target/release/strata examples/fetch-examples.sh

# Pull fresh packs from the Public Repo and rebuild examples/ (needs PALETTE_API_KEY)
examples-fetch: build
    STRATA_BIN=target/release/strata examples/fetch-examples.sh

# Install the pinned OpenTofu into ~/.local/bin for the live example. Needs no root.
tofu-install:
    TOFU_VERSION={{tofu_version}} examples/live/tofu-install.sh

# Remove the OpenTofu that tofu-install put in ~/.local/bin
tofu-uninstall:
    TOFU_VERSION={{tofu_version}} examples/live/tofu-install.sh --uninstall

# Live example: build a cluster and two profile versions in Palette. Billable until live-down.
# When the apply fails, examples/live/diagnose.sh saves what holds each cluster.
live-up:
    #!/usr/bin/env bash
    set -uo pipefail
    examples/live/tf.sh init -input=false || exit 1
    examples/live/tf.sh apply && exit 0
    examples/live/diagnose.sh
    exit 1

# Run every live check against the live example and write a summary
live-verify: build
    examples/live/verify.sh

# Run the four live scripts in order from the baseline, under one runs/<UTC time>/ directory:
# verify.sh, in-place.sh off, in-place.sh on (against that off run), and registry-move.sh. Stops at
# the first script that fails. Billable: about 3 to 4 hours with cluster B twice.
live-session: build
    #!/usr/bin/env bash
    set -uo pipefail
    session=examples/live/runs/$(date -u +%Y%m%dT%H%M%SZ)
    OUT=$session examples/live/verify.sh || exit 1
    OUT=$session/in-place/off examples/live/in-place.sh off || exit 1
    OUT=$session/in-place/on OFF_RUN=$PWD/$session/in-place/off examples/live/in-place.sh on || exit 1
    OUT=$session/registry-move examples/live/registry-move.sh || exit 1
    echo "live-session: all four passed; results in $session"

# The drift report test (#25), with the flag off and then on, in one session directory. It opens
# and closes issues labeled strata-live-drift in this repository; see examples/live/README.md.
live-drift-report: build
    #!/usr/bin/env bash
    set -uo pipefail
    session=examples/live/runs/$(date -u +%Y%m%dT%H%M%SZ)
    OUT=$session/drift-report/off examples/live/drift-report.sh off || exit 1
    OUT=$session/drift-report/on examples/live/drift-report.sh on || exit 1
    echo "live-drift-report: both passed; results in $session"

# Tear the live example down, with every version of every profile name it uses, including the
# versions skip_destroy kept out of the OpenTofu state. When the destroy fails,
# examples/live/diagnose.sh saves what holds each cluster.
live-down:
    #!/usr/bin/env bash
    set -uo pipefail
    outputs=$(examples/live/tf.sh output -json) || { echo "live-down: no tofu outputs" >&2; exit 1; }
    export PALETTE_PROJECT_UID=$(jq -r '.project_uid.value' <<<"$outputs")
    names=$(jq -r '.profile_names.value[]' <<<"$outputs")
    examples/live/tf.sh destroy || { examples/live/diagnose.sh; exit 1; }
    examples/live/api.sh delete-named $names

# Open one GitHub issue per section of docs/backlog.md (needs gh and a GitHub remote)
backlog-issues:
    scripts/open-backlog-issues.sh
