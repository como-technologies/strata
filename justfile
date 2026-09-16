# Local entry point; CI runs the same recipes.

default: build

# Release binary at target/release/strata
build:
    cargo build --release

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
