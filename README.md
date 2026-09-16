# strata

A Spectro Cloud Palette cluster profile is a stack of layers: OS, Kubernetes,
CNI, CSI, and add-ons. Each layer is a pack with a version, a digest, and
values. Strata treats each published version of that stack as an immutable
release. It makes the release a thing a team can record, compare, and verify,
like any other build artifact.

- **record** a published version as a snapshot. The snapshot holds a Merkle
  root over the layers, three hashes per layer, the canonical contents, and
  the clusters that use the version.
- **compare** two versions. The report says what kind of change occurred in
  each layer, and what the change does to running clusters.
- **verify** that a published version still agrees with its record. A tool
  enforces immutability, not a sentence in a document.

Think of it as build, release, run. A pack is a build. A profile version is
a release. A cluster is a run. Strata is the record, the review, and the
audit of the release.

**Start here:** [docs/walkthrough.md](docs/walkthrough.md) shows a platform
team that uses strata through one release cycle. Each command in it runs
offline against `examples/`.

## Commands

```
strata list                                     # the profile and cluster refs in the scope
strata list --pack cilium                       # which profile versions carry a pack?
strata my-profile@1.2.0 my-profile@1.3.0        # compare two versions
strata compare A B --format markdown            # the pull request comment
strata compare A B --format json                # for scripts and AI clients
strata snapshot my-profile@1.2.0 -o rel.json    # record a release
strata snapshot --from-candidate my-profile.yaml -o candidate.json  # before it exists in Palette
strata compare rel.json cluster:<uid>/my-profile  # does the cluster run that release?
strata consumers my-profile@1.0.0               # which clusters still use it? exit 1 if any
strata verify rel.json                          # the immutability check
strata verify 'releases/**/*.json'              # every record: one line each, one summary
strata same-release dev.json prod.json          # one release in every project? --allow registry-move
strata schema diff                              # JSON Schema of the diff document
```

A ref is a profile uid, `<name>@<version>`, `cluster:<uid>`, or a snapshot
file. `strata list` prints the refs in the scope. `<name>@latest` is the
highest version of that name in the scope, by natural order, so `22.04` is
above `20.04` and `1.30.6` above `1.29.10`. `cluster:<uid>` is what a cluster
runs: every profile applied to it, in order. `cluster:<uid>/<profile>` selects
one applied profile by name. A cluster that runs a recorded version has the
same root as that record. The exit status is `0` for identical or unchanged,
`1` for different or drifted, and `2` for an error.

## Install

Each release ships one static binary per platform, a `SHA256SUMS` file, and
a Sigstore signature for every file. Download, check, and move onto your
path:

```
v=v0.4.0; t=x86_64-unknown-linux-musl
base=https://github.com/como-technologies/strata/releases/download/$v
curl -fsSLO $base/strata-$t
curl -fsSLO $base/SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS      # macOS: shasum -a 256 --check --ignore-missing SHA256SUMS
chmod +x strata-$t && sudo mv strata-$t /usr/local/bin/strata
```

Targets: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`,
`aarch64-apple-darwin`, `x86_64-apple-darwin`. The Linux binaries have no
shared library dependencies.

While this repository is private, anonymous downloads fail. Download with an
authenticated `gh` instead, then check and install as above:

```
gh release download $v --repo como-technologies/strata --pattern strata-$t --pattern SHA256SUMS
```

To check who built a file, verify its signature with
[cosign](https://github.com/sigstore/cosign) 3 or later. The release workflow signs
through its own identity, so the check pins the repository and the workflow:

```
curl -fsSLO $base/strata-$t.sigstore.json
cosign verify-blob strata-$t --bundle strata-$t.sigstore.json \
  --certificate-identity-regexp '^https://github.com/como-technologies/strata/\.github/workflows/release\.yml@refs/tags/v' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

## Build and configure

1. Run `cargo build --release`, or `just build`. The binary is
   `target/release/strata`. `just build-target <triple>` builds one of the
   release targets the same way the release workflow does.
2. Set `PALETTE_API_KEY`.
3. Set `PALETTE_PROJECT_UID` for project-scoped profiles. A snapshot records
   the project it was taken in, and `verify` checks each record there, so a
   scheduled job needs no per-project setup. `--project` overrides it.
4. Set `PALETTE_API` for a self-hosted Palette. The default is
   `https://api.spectrocloud.com`.
5. Set `STRATA_RETRIES` to change how many times a failed request is retried.
   The default is 3. Strata retries HTTP 5xx, 429, and transport errors with
   a doubling wait from one second, and honors `Retry-After`. Other errors
   are not retried.

Strata reads values that are not in the environment from `~/.palette.env`.

## Where strata fits

Strata is one command in whatever process a team already has. It does not
care how a profile version is published: Terraform, the API, or the Palette
UI. It does not care where the records live, which registry holds the packs,
which cloud the profile targets, or which CI system runs it. It needs an API
key, an optional project uid, and a place to keep JSON files.

The one idea it enforces: a published profile version is a release. It has a
name and a version. It is never changed after publication. A pack is a build.
A cluster is a run. Strata is the record, the review, and the audit of the
release.

| moment                              | what strata does                                                                 |
|-------------------------------------|----------------------------------------------------------------------------------|
| a profile change is under review    | `snapshot --from-candidate` on the file under review, then `compare --format markdown` against the last record. The comment is the evidence the approver reads, before anything is published. |
| a profile version is published      | `snapshot`. Keep the file with the release.                                      |
| a cluster is about to move versions | `compare` between the current and the proposed versions. The report shows the repave impact. |
| a cluster should run its release    | `compare` between the record and `cluster:<uid>/<profile>`. `IDENTICAL` means the cluster runs what was released. |
| on a schedule                       | `verify` over each recorded snapshot. This enforces the immutability rule.      |
| before end of life                  | `consumers`: each cluster that uses the version, with its project and state. Exit `0` means nothing uses it. |

## Pull request comments

`scripts/pr-comment.sh A B` runs the comparison, posts it as one comment on
the pull request, and adds the `repave` label when the maximum impact
reaches `repave`. On the next push it updates the same comment, which it
finds by the `<!-- strata -->` marker on the first line, and removes the
label when the impact no longer reaches the threshold. `--label` and
`--threshold` change both. `--dry-run` prints the comment and says what it
would do with the label, without touching the host. The script needs `gh`
logged in and `jq`. The pull request and repository come from `--pr` and
`--repo`, or from `gh` when run inside a checkout of the branch. Run locally
it posts the same comment CI would.

On GitHub Actions, call the reusable workflow instead of copying anything.
The caller needs `pull-requests: write`:

```yaml
on: pull_request
permissions:
  contents: read
  pull-requests: write
jobs:
  strata:
    uses: como-technologies/strata/.github/workflows/comment.yml@v0.4.0
    with:
      a: releases/1.1.0/dev.json
      b: releases/1.1.1/dev.json
      strata-version: v0.4.0
    secrets:
      strata-token: ${{ secrets.STRATA_READ_TOKEN }}     # while this repository is private
      palette-api-key: ${{ secrets.PALETTE_API_KEY }}   # only when a ref is not a snapshot file
```

The workflow downloads the release binary, checks it against `SHA256SUMS`,
and runs the script. It comments with the caller's token. Any other CI
system installs strata and runs the script directly.

While this repository is private, two things are needed before another
repository can call the workflow. First, allow access in this repository's
Settings, under Actions, General, Access: "Accessible from repositories in
the organization". Second, pass `strata-token`: a fine-grained token or a
GitHub App token with read access to this repository's contents. The
caller's own token cannot read another private repository, so without it
the checkout and the download fail.

## Documentation

- [docs/walkthrough.md](docs/walkthrough.md): one team, one profile, one
  release cycle, with each command and its output.
- [docs/model.md](docs/model.md): the three hashes, the Merkle root, the
  canonical form, the impact levels, and the snapshot document.
- [docs/output.md](docs/output.md): the terminal report, the markdown
  comment, the JSON document for scripts and AI clients, and colors.
- [examples/README.md](examples/README.md): real packs and profiles for
  offline use.
- [examples/live/README.md](examples/live/README.md): a real cluster and two
  profile versions in Palette, with every check that needs them and the
  recorded run.
- [docs/backlog.md](docs/backlog.md): the planned work, one section per
  GitHub issue; `just backlog-issues` opens them.

## Library

The crate is a library with a thin command line on top. A Rust program can
depend on `strata` and use the same pieces: `api::PaletteApi` with an HTTP
implementation and an in-memory one for tests, `snapshot::Snapshot`,
`diff::compute`, `render::Renderer`, and the operations in `ops`. The command
line adds only argument parsing, printing, and exit codes.

To depend on it, pin a release tag. The repository is private, so Cargo has to
fetch it with the git command line, which uses your SSH setup:

```toml
[dependencies]
strata = { git = "ssh://git@github.com/como-technologies/strata.git", tag = "v0.4.0" }
```

```
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo build
```

Setting `git-fetch-with-cli = true` under `[net]` in `.cargo/config.toml` does
the same. Without it, Cargo's built-in git fails with "no authentication
methods succeeded".

`cargo doc --open` shows the API, and the crate docs start with an example
that runs as a test. `examples/library/` is a separate crate that uses strata
this way, with no command line and no network; `just library-example` runs it,
and so does CI. [docs/model.md](docs/model.md) states what a version number
promises for the library, the documents, and the command line.

## Tests

`cargo test`, or `just test`, runs unit tests, library tests against an
in-memory Palette API, end-to-end tests of the binary over fixtures with the
shape of the API responses, and tests of the scripts. The tests do not use
the network. CI runs `just lint` and `just test` on each push and pull
request, and builds the Linux binary with the same step a release uses.

The fixtures follow the Palette API's shapes, but only a real tenant proves
them. A change to how strata reads Palette, such as cluster refs, consumers,
candidates, or the API client, also needs the live example:
`just live-up`, `just live-verify`, and `just live-down`. It builds a billable
cluster, so it runs by hand and never in CI.
[examples/live/README.md](examples/live/README.md) has the prerequisites, the
17 checks, what to do when one fails or a teardown sticks, and the recorded
run.

## Releasing

1. Set the new version in `Cargo.toml` and commit.
2. Run `just release`. It tags `HEAD` as `v<version>` and refuses on a dirty
   tree or an existing tag.
3. Push the tag. The release workflow builds the four binaries, writes
   `SHA256SUMS`, signs every file, verifies the signatures, and publishes the
   GitHub release with generated notes.

## Known limits

- A cluster snapshot records variable definitions, not the values a cluster
  was given (#20).
- Two profiles that pin the same pack under different tag names, such as
  `24.04` and `LTS__24.x`, differ in `IDENTITY`, and on the OS or Kubernetes
  layer the report calls that a repave (#19).
- Projects and clusters are named by uid only; `PALETTE_PROJECT` is ignored
  (#18).
- A snapshot rebuilt with `--from-raw` loses the project it was taken in
  (#22).
- The text reports truncate values longer than 120 characters. The JSON
  document and `--full` carry the full value.
