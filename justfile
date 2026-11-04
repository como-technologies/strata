# Local entry point; CI runs the same recipes. Run `just --list` to see each recipe.
#
# `just --list` prints the LAST comment line before a recipe as its help. Put an explanation
# above a blank line, and one help line directly above the recipe.

# The OpenTofu version that the live example runs
tofu_version := env_var_or_default("TOFU_VERSION", "1.12.6")

# Build the release binary
default: build

# ---- build and test -------------------------------------------------------------------------

# strata-preflight is a package of its own in preflight/; `strata preflight` runs its binary from
# the same directory. The two build together, and a release ships both.

# Build the release binaries at target/release/strata and target/release/strata-preflight
build:
    cargo build --release -p strata -p strata-preflight

# The observer is a Go program in observer/. The build is static (no cgo), and the version is that of Cargo.toml.

# Build the observer as a static binary at target/observer/strata-observer
observer-build:
    CGO_ENABLED=0 go -C observer build -trimpath -ldflags "-X main.version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)" -o ../target/observer/strata-observer .

# Give the documents of the strata binary of this commit to the allow-list of the observer. --write also refreshes its test data.
observer-contract *args: build stand-in
    scripts/observer-contract.sh {{args}}

# Serve the dashboard of the observer from test data: ok, quiet, first-load, no-records, stale, lost, first-seen
observer-dev state="ok":
    go -C observer run ./internal/web/devserver -state {{state}}

# The image is distroless/static, so its strata binary must be the static musl build. The recipe `build` gives a glibc binary that the image cannot run, and `build-target` needs cross.

# Build the image strata-observer:dev from a local binary, for a test on kind
observer-image: (build-target "x86_64-unknown-linux-musl")
    mkdir -p dist && cp target/x86_64-unknown-linux-musl/release/strata dist/strata-amd64
    docker build -f deploy/observer/Dockerfile --build-arg VERSION=dev --platform linux/amd64 -t strata-observer:dev .

# Render deploy/observer and make sure it holds four objects. Needs kubectl.
observer-manifests:
    test "$(kubectl kustomize deploy/observer | grep -c '^kind: ')" = 4

# Linux targets are static musl builds in the containers of cross. The other targets build
# natively with cargo.

# Build the release binary for one target at target/<target>/release/strata
build-target target:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{target}}" in
      *linux*) cross build --release --target "{{target}}" -p strata -p strata-preflight ;;
      *) cargo build --release --target "{{target}}" -p strata -p strata-preflight ;;
    esac

# strata-fuzz holds the fuzz targets and the property tests. It runs each target on its corpus and
# on random input.

# Run the tests of strata, strata-preflight, xtask (the stand-in for Palette), strata-benches (the input of the benchmarks), strata-fuzz, strata-report-endpoint, the Go observer, and the request counter of the live example. No network.
test: stand-in
    cargo test
    cargo test -p strata-preflight
    cargo test -p xtask
    cargo test -p strata-benches
    cargo test -p strata-fuzz
    cargo test -p strata-report-endpoint
    go -C observer test ./...
    go -C examples/live/count-requests test ./...

# The benchmarks are the package strata-benches in benches/. They are not tests. `just test` does
# not run them, and no recipe fails on a time. See "The benchmarks" in
# docs/src/development/build-test.md.

# Run the benchmarks and print the time of each. Arguments go to criterion, for example: just bench --save-baseline before
bench *args:
    cargo bench -p strata-benches -- {{args}}

# The measure time of this recipe is short, so a run takes minutes. CI runs it. It fails when a
# benchmark does not build or panics, and never on a time.

# Run each benchmark with a short measure time, as the benchmarks job of CI does
bench-short:
    cargo bench -p strata-benches -- --warm-up-time 1 --measurement-time 3

# The rustdoc command of `just lint` and `just api-docs`. The two recipes share one command, so
# that they share one build. `just lint` adds the packages xtask, strata-benches,
# strata-report-endpoint, and strata-fuzz, and runs the command a second time with all items. The
# build of the public items does not read the comment of a private item. The build of all items does not warn when a public item links to a private item.
# So `just lint` needs both.
rustdoc := 'RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p strata -p strata-preflight'

# The last two lines fail when a module enters observer/go.mod or the go.mod of the request counter: both use the standard library only.

# Test the format, the clippy lints, and the rustdoc of the public items and of all items, and the format and the vet of the Go observer and of the request counter
lint:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    {{rustdoc}} -p xtask -p strata-benches -p strata-report-endpoint -p strata-fuzz
    {{rustdoc}} -p xtask -p strata-benches -p strata-report-endpoint -p strata-fuzz --document-private-items
    test -z "$(gofmt -l observer)"
    test -z "$(gofmt -l examples/live/count-requests)"
    go -C observer vet ./...
    go -C examples/live/count-requests vet ./...
    ! grep -q '^require' observer/go.mod
    ! grep -q '^require' examples/live/count-requests/go.mod

# Build and run examples/library and examples/library-preflight, crates that use strata and strata-preflight as libraries
library-example:
    cargo run --quiet --manifest-path examples/library/Cargo.toml --target-dir target/library-example
    cargo run --quiet --manifest-path examples/library-preflight/Cargo.toml --target-dir target/library-example

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

# ---- fuzzing --------------------------------------------------------------------------------
# The fuzz targets are the tests of fuzz/, in the package strata-fuzz. `just test` runs each one on
# its corpus and on random input, on stable Rust. The recipes here run a target under libFuzzer,
# which needs a nightly toolchain and cargo-bolero. See "The fuzzing" in
# docs/src/development/build-test.md.

# Name each fuzz target and each property test. Needs cargo-bolero.
fuzz-list:
    scripts/dev/fuzz.sh list

# Run one fuzz target under libFuzzer, for a time such as 10m. The target is a name of fuzz-list, or the start of one. Needs a nightly toolchain and cargo-bolero.
fuzz target time="60s":
    scripts/dev/fuzz.sh run "{{target}}" "{{time}}"

# The seeds come from examples/, the catalog of strata-preflight, and the output of the book.

# Write the seed corpus of each fuzz target into fuzz/src/__fuzz__/
fuzz-seed: examples
    cargo test -p strata-fuzz -- --ignored write_the_seed_corpus

# ---- documentation --------------------------------------------------------------------------
# The book is docs/src/. It needs mdbook, mdbook-mermaid, and mdbook-gruvbox. docs/book/ is the
# output, and it is not tracked.

# mdbook exits with code 0 on a warning, so the script reads its log and fails on a WARN line.

# Build the book into docs/book/, and fail when mdbook prints a warning
docs:
    scripts/dev/build-book.sh docs

# Build the book and serve it at http://localhost:3000 with live reload
docs-serve:
    mdbook serve docs --open

# The command is that of `just lint`, with the public items only. Add --open to open the pages.

# Build the rustdoc of strata and strata-preflight into target/doc/, and print the path of the HTML
api-docs *args:
    {{rustdoc}} {{args}}
    @echo "{{justfile_directory()}}/target/doc/strata/index.html"

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

# The list is in xtask/facts/. A script reads the JSON with jq. See docs/src/development/facts.md.

# Print the list of the facts of Palette as JSON
facts:
    @cargo run --quiet -p xtask -- facts

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

# Each recipe that builds runs the checks of the lab first, examples/live/preflight.sh: the tools,
# the credentials of the cloud CLIs, the APIs of the GCP project, and the leftovers of a run
# before. A failed check stops the recipe before it spends anything.

# Build the live example: a cluster and the profile versions. Billable until live-down.
live-up: build
    #!/usr/bin/env bash
    set -uo pipefail
    examples/live/preflight.sh || exit 1
    examples/live/tf.sh init -input=false || exit 1
    examples/live/tf.sh apply && exit 0
    examples/live/diagnose.sh
    exit 1

# Build the profiles of the live example and no cluster: seconds, and no cost
live-profiles: build
    #!/usr/bin/env bash
    set -uo pipefail
    examples/live/preflight.sh || exit 1
    examples/live/tf.sh init -input=false || exit 1
    examples/live/tf.sh apply -var cluster=false

# The checks of the lab itself, with no build: the tools, the key, the cloud CLIs, the APIs, the leftovers (#91)
live-preflight: build
    examples/live/preflight.sh

# Watch a build on the terminal: strata preflight --watch --verbose on a cluster from the moment Palette lists it, one line for each rule at each reading (#91). --build makes cluster B beside A, --down removes it after; needs live-profiles or live-up.
live-watch *args: build
    examples/live/watch.sh {{args}}

# The test plan of #91 for one engine: each fault on purpose fails, and a clean entity passes; the run goes to the ledger. Needs live-profiles with that cloud; --build adds the build of cluster A, watched, and the checks of the cluster; --build --down tears it down after.
live-preflight-plan engine *args: build
    examples/live/preflight-plan.sh {{engine}} {{args}}

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

# The profile validate checks: what Palette answers when strata asks whether it takes the packs;
# no cluster, no profile, no cost (#152)
live-validate-packs: build
    examples/live/validate-packs.sh

# Each case sends one fault to the validate route of a cluster. A rule of the preflight catalog
# stays until the route refuses the fault of its case. Needs the key and a directory of bodies.

# The cluster validate route checks: does the route refuse the fault that a rule copies (#243)
live-validate-route bodies: build
    examples/live/validate-route.sh {{bodies}}

# The case bodies of live-validate-route on AWS: the clean body, and a profile for each case that
# Palette takes. It runs validate-route.sh on the bodies, and then deletes the profiles. Needs
# live-profiles with cloud = aws; no cluster (#408, #413)
live-route-bodies bodies: build
    examples/live/route-bodies.sh {{bodies}}

# The profile export checks: the export of each version of the example, read as a candidate;
# each request is a GET, no cluster, no cost (#156)
live-export: build
    examples/live/export.sh

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

# The fleet test: list clusters --tag and --filter, and a record of each cluster of a set, with --prune (#80). Needs live-up; labels cluster A and makes a saved tag filter through the API, and puts both back.
live-fleet: build
    examples/live/fleet.sh

# The everyday loop, end to end: a change, the gates, the publish, the questions to Palette about a move, preflight on the body of a create, the nightly jobs, an override, an edit in place, an incident, and a retire, with the records in a git repository (#81, #385). Needs live-up; undoes each change.
live-loop: build
    examples/live/loop.sh

# The observer on kind (#644): the manifests, the records, the pod, and the page, as a user with only the reader role sees them. Needs live-up, docker, kind, and kubectl. Makes a kind cluster and deletes it; writes nothing to the tenant.
live-observer-kind: build observer-image
    examples/live/observer-kind.sh

# The observer on the real cluster fred (#656): a plain kubectl apply and then a profile in Palette, with the image of OBSERVER_IMAGE. Needs live-up, kubectl, gh, go, and a token file with only read:packages (GHCR_PULL_TOKEN_FILE). Makes a namespace and a profile, and removes both; the edit in place of the add-on version goes back.
live-observer: build
    examples/live/observer.sh

# The managed engine test on EKS: the release, the shape tier, and the repave answers of Palette, and a change of the node group (#89). Needs cloud = "eks" in terraform.tfvars and live-up.
live-eks: build
    examples/live/eks.sh

# The managed engine test on GKE: the release, the shape tier, and the repave answers of Palette; GKE_POOL_CHANGE=1 adds a change of the node pool (#90). Needs cloud = "gke" in terraform.tfvars and live-up.
live-gke: build
    examples/live/gke.sh

# One state per version: a bump as a new resource in a new state, with no feature_preview flag, and the destroy that a cluster in use refuses (#81). Needs live-up; leaves nothing behind.
live-version-states: build
    examples/live/version-states.sh

# The upstream test: strata behind against the registries, and scripts/ci/upstream-report.sh around a version that arrives in the example's own registry (#79). Needs live-profiles; opens and closes issues of this repository with the label strata-live-behind.
live-upstream: build
    examples/live/upstream.sh

# The registry kinds test: the digest of each registry kind, and a push again of one version into each (#75). Needs live-profiles; builds and removes its own registry host.
live-registry-kinds: build
    examples/live/registry-kinds.sh

# The cluster template test: the lock on a linked version, a cluster outside the template (#66), and a cluster in the template with the two repave routes (#276). Needs live-profiles and no cluster; builds two clusters; ends with the removal of the example.
live-template: build
    examples/live/template.sh

# The cluster test: a change of the value a cluster gives a variable, and a deletion through the API (#66). Needs live-up; ends with the cluster gone.
live-cluster: build
    examples/live/cluster.sh

# The order test: a reorder of the packs of a published version, and of the profiles of a cluster, through the API (#66). Needs live-up; leaves the versions edited, so run live-down after it.
live-order: build
    examples/live/order.sh

# The audit log test: who made an edit through the API, and an update a cluster accepted (#69). Needs live-profiles; the cluster stage needs live-up; the stage of the key with no audit permission needs STRATA_LIVE_NO_AUDIT_KEY (#664). Undoes each edit.
live-who: build
    examples/live/who.sh

# The draft test: a PUT of the add-on version with no publish, <uid>@draft, and the publish (#155). Needs live-profiles and no cluster. Puts the edit back.
live-draft: build
    examples/live/draft.sh

# The scope test: a profile of the tenant beside a profile of the project with one name and one version, and the ref that two scopes answer (#113). Needs live-profiles and no cluster. Deletes the tenant profile.
live-scope: build
    examples/live/scope.sh

# The release test: the route that gives the release of Palette answers a call with no key, and refuses a call with the key (#146). Needs only the key; writes nothing.
live-release: build
    examples/live/release.sh

# The contract run (#390, #393): ask the real tenant the requests of the facts of Palette, and compare each answer with the stand-in. It sends GET requests only, and the read-only calls of the clouds in the lab account (--no-cloud sends none). --with-cluster holds the floor of a window with a cluster: a skipped fact of the floor is an error. Needs the key and just examples; --state FILE also reads the latest releases of the Palette SDK and the Terraform provider.
live-contract *args: examples stand-in
    examples/live/contract.sh {{args}}

# Open, update, and close the issues of a contract run (#390): one for each fact that differs, one for each new release. Needs a `gh` login. --dry-run calls no host.
contract-report *args:
    examples/live/contract-report.sh {{args}}

# The set has a state of its own, so live-down never removes it. See "The profiles of the run" in docs/src/development/facts.md.

# Make, remove, or list the standing cluster profiles of the contract run: only profiles, no AWS resource, no registry, no cluster (#531)
contract-profiles action:
    examples/live/contract-profiles.sh {{action}}

# The timer runs deploy/contract/weekly.sh of this checkout once a week. See "The weekly contract run" in docs/src/development/facts.md.

# Install the weekly contract run on this machine as a systemd user timer (#390)
contract-timer-install:
    deploy/contract/timer.sh install

# Stop the weekly contract run and delete its units; the results stay (#390)
contract-timer-remove:
    deploy/contract/timer.sh remove

# Show the weekly contract run: the timer, and the last result (#390)
contract-timer-status:
    deploy/contract/timer.sh status

# The lead runs this on kadomony after the merge. The script reads no secret: see "The endpoint of the report" in docs/src/development/report-endpoint.md.

# Build the image of the endpoint of the report, and deploy it to Cloud Run in the project of the lab (#397). Needs a `gcloud` login. --dry-run prints the gcloud commands and runs none.
report-deploy *args:
    deploy/report/deploy.sh {{args}}

# Send one report to the deployed endpoint of the report, twice, and check that one issue of the label unknown-value holds both (#397). It serves the stand-in tenant of the book, and reads and writes no Palette tenant. Needs a `gh` login, and the address that report-deploy printed.
live-report-send *args: examples stand-in
    examples/live/report-send.sh {{args}}

# The pending update test: when Palette says an update waits for cluster A, what a record, the listing, and compare say, what cluster:<uid>@pending holds (#70, #78), and what compare --repave asks Palette about it (#276). Needs live-up; undoes each change.
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

# The image digest test: what cluster A pulls when a digest sits in the image list of a pack alone, and when it sits in the manifest that the workload reads (#630). Needs live-up, kubectl, and crane; undoes each change.
live-image-digest: build
    examples/live/image-digest.sh

# The macro repave test: a macro in a field that repaves cluster A, and what a change of its value alone does (#72). Needs live-up; ends with a repave that waits for approval, so live-down follows it.
live-macro-repave: build
    examples/live/macro-repave.sh

# The repave test: compare --repave asks Palette if a change repaves cluster A (#73). Needs live-up; takes no update on the cluster.
live-repave: build
    examples/live/repave.sh

# The registry move of a running cluster: the OS pack of the release moves to another registry, and cluster A takes it (#135). Needs live-up; billable: up to two repaves of cluster A.
live-registry-move-nodes: build
    examples/live/registry-move-nodes.sh

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

# The limited key test: the status and the error code that Palette answers to a key that cannot read config/rbacs of cluster A (#276). Needs live-up and STRATA_LIVE_LIMITED_KEY; reads only.
live-tiers-limited: build
    examples/live/tiers.sh 6

# The pool override test (#515): an override on a machine pool of cluster A. It checks the shape tier of a new record, and compare of an older record. Needs live-up; sets both overrides on the worker pool through the API, and then restores the pool.
live-tiers-pool: build
    examples/live/tiers.sh 7

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

# Set the version everywhere it appears, and write the outputs of the book that print it again, for example: just version 1.2.3
version VERSION:
    scripts/dev/set-version.sh {{VERSION}}

# Tag the version in Cargo.toml as v<version>
release:
    scripts/dev/tag-release.sh
