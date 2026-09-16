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

# Rebuild examples/ from the cached registry responses (no API key needed)
examples: build
    OFFLINE=1 STRATA_BIN=target/release/strata examples/fetch-examples.sh

# Pull fresh packs from the Public Repo and rebuild examples/ (needs PALETTE_API_KEY)
examples-fetch: build
    STRATA_BIN=target/release/strata examples/fetch-examples.sh

# Open one GitHub issue per section of docs/backlog.md (needs gh and a GitHub remote)
backlog-issues:
    scripts/open-backlog-issues.sh
