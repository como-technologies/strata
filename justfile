# Local entry point; CI runs the same recipes.

default: build

# Release binary at target/release/palette-profile-diff
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
    OFFLINE=1 PPD=target/release/palette-profile-diff examples/fetch-examples.sh

# Pull fresh packs from the Public Repo and rebuild examples/ (needs PALETTE_API_KEY)
examples-fetch: build
    PPD=target/release/palette-profile-diff examples/fetch-examples.sh
