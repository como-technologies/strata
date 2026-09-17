# Local entry point; CI runs the same recipes.

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

# Tear the live example down, with the add-on and move profile versions that skip_destroy kept
# out of Terraform's state. When the destroy fails, examples/live/diagnose.sh saves what holds
# each cluster.
live-down:
    #!/usr/bin/env bash
    set -uo pipefail
    export PALETTE_PROJECT_UID=$(examples/live/tf.sh output -raw project_uid)
    examples/live/tf.sh destroy || { examples/live/diagnose.sh; exit 1; }
    examples/live/api.sh delete-named strata-live-addon strata-live-move

# Open one GitHub issue per section of docs/backlog.md (needs gh and a GitHub remote)
backlog-issues:
    scripts/open-backlog-issues.sh
