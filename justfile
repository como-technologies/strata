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

# knurl is a private repository, so Cargo fetches it with the git command line. The adapter is
# its own package: `cargo build` and `cargo test` at the root never build it, and no release
# carries it.

# Test the knurl adapter against the rules of knurl, on the built examples. Needs git access to knurl.
adapter-test: examples
    CARGO_NET_GIT_FETCH_WITH_CLI=true cargo fmt --check --manifest-path adapter/Cargo.toml
    CARGO_NET_GIT_FETCH_WITH_CLI=true cargo clippy --manifest-path adapter/Cargo.toml --target-dir target/adapter --all-targets -- -D warnings
    CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test --manifest-path adapter/Cargo.toml --target-dir target/adapter

# Run the strata command and the port of knurl side by side, against the playground: no tenant, no key
adapter-by-hand chapter="cluster-release":
    adapter/scripts/by-hand.sh {{chapter}}

# Call the platform port of knurl by hand, over HTTP: just adapter-observe unit intermediate aws-stack 1.1.0
adapter-observe *args:
    @CARGO_NET_GIT_FETCH_WITH_CLI=true cargo run --quiet --manifest-path adapter/Cargo.toml --target-dir target/adapter --example observe -- {{args}}

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

# The pack state checks: deprecated, disabled, and deleted packs; no cluster, and no profile stays (#68)
live-pack-states: build
    examples/live/pack-states.sh

# Read-only. It needs live-profiles; the instance checks need live-up, and are SEEN without it.

# The knurl adapter against the tenant: each port method against the strata answer for the same object
live-knurl: build
    examples/live/knurl.sh

# The merge gate over records of the live profiles: needs live-profiles, no cluster, no cost (#57)
live-merge-gate: build
    examples/live/merge-gate.sh

# The registry test: push into the example's own OCI pack registry, and read what Palette does with each push (#66). Needs live-profiles.
live-registry: build
    examples/live/registry.sh

# The upstream test: strata behind against the registries, and scripts/ci/upstream-report.sh around a version that arrives in the example's own registry (#79). Needs live-profiles; opens and closes issues of this repository with the label strata-live-behind.
live-upstream: build
    examples/live/upstream.sh

# The registry kinds test: the digest of each registry kind, and a push again of one version into each (#75). Needs live-profiles; builds and removes its own registry host.
live-registry-kinds: build
    examples/live/registry-kinds.sh

# The cluster template test: the lock on a linked version, and a cluster outside the template (#66). Needs live-profiles and no cluster; ends with the removal of the example.
live-template: build
    examples/live/template.sh

# The cluster test: a change of the value a cluster gives a variable, and a deletion through the API (#66). Needs live-up; ends with the cluster gone.
live-cluster: build
    examples/live/cluster.sh

# The order test: a reorder of the packs of a published version, and of the profiles of a cluster, through the API (#66). Needs live-up; leaves the versions edited, so run live-down after it.
live-order: build
    examples/live/order.sh

# The audit log test: who made an edit through the API, and an update a cluster accepted (#69). Needs live-profiles; the cluster stage needs live-up. Undoes each edit.
live-who: build
    examples/live/who.sh

# The pending update test: when Palette says an update waits for cluster A, what a record, the listing, and compare say, and what cluster:<uid>@pending holds (#70, #78). Needs live-up; undoes each change.
live-pending: build
    examples/live/pending.sh

# The app profile test: a record of appprofile:<name>@<version>, and a plain update of it (#78). Needs no cluster; makes an app profile, and deletes it.
live-app-profile: build
    examples/live/app-profile.sh

# The cluster group test: a record of clustergroup:<uid>, and compare of the group with its virtual cluster (#78). Needs live-up; makes a group and a virtual cluster in cluster A, and removes them.
live-cluster-group: build
    examples/live/cluster-group.sh

# The edge host test: a record of edgehost:<uid>, and consumers of a profile bound to an edge host (#78). Needs live-up; registers an edge host with no device, and deletes it.
live-edge-host: build
    examples/live/edge-host.sh

# The macro test: what a project macro does to a version, a cluster copy, and a record, when it changes (#72). Needs live-up; undoes each change.
live-macros: build
    examples/live/macros.sh

# The macro repave test: a macro in a field that repaves cluster A, and what a change of its value alone does (#72). Needs live-up; ends with a repave that waits for approval, so live-down follows it.
live-macro-repave: build
    examples/live/macro-repave.sh

# The repave test: compare --repave asks Palette if a change repaves cluster A (#73). Needs live-up; takes no update on the cluster.
live-repave: build
    examples/live/repave.sh

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

# The cluster tiers test: what a cluster record keeps beside the release, and compare of two cluster records (#76). Needs live-up; builds and removes cluster B.
live-tiers: build
    examples/live/tiers.sh

# What clusters A and B still have in the cloud that terraform.tfvars names; 0 on every line after live-down
live-inventory:
    examples/live/inventory.sh

# The recipe also deletes the versions that skip_destroy kept out of the OpenTofu state. When the
# destroy fails, examples/live/diagnose.sh saves what holds each cluster.

# Remove the live example and every version of each profile name that it uses
live-down:
    #!/usr/bin/env bash
    set -uo pipefail
    outputs=$(examples/live/tf.sh output -json) || { echo "live-down: no tofu outputs" >&2; exit 1; }
    export PALETTE_PROJECT_UID=$(jq -r '.project_uid.value' <<<"$outputs")
    names=$(jq -r '.profile_names.value[]' <<<"$outputs")
    # Palette now and then refuses one read of a profile with OperationForbidden, and the next
    # read passes: a destroy that fails gets one more try before the diagnostics.
    examples/live/tf.sh destroy || examples/live/tf.sh destroy || { examples/live/diagnose.sh; exit 1; }
    examples/live/api.sh delete-named $names

# ---- release --------------------------------------------------------------------------------

# The recipe refuses a dirty tree or a tag that exists. Push the tag to start the release workflow.

# Set the version everywhere it is written, for example: just version 1.2.3
version VERSION:
    scripts/dev/set-version.sh {{VERSION}}

# Tag the version in Cargo.toml as v<version>
release:
    scripts/dev/tag-release.sh
