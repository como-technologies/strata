# Install

## Requirements

| Tool | Function |
| --- | --- |
| `strata` | The command line. One static binary. |
| `strata-preflight` | The preflight checks; `strata preflight` runs it from the same directory. One static binary, in every release from the one after v0.15.0. |
| A Palette API key | Reads the profiles and clusters. |
| `curl`, `jq` | Read the JSON documents in scripts and in this book, and fetch a release. |
| `gh` | Posts pull request comments and drift issues. |
| `cosign` 3 or later | Checks the signature of `SHA256SUMS`. Optional, unless you give `--require-signature`. |
| Rust, `just` | Install with Cargo, or build from source. `cross` builds another target. |

## Download a release

Each release has these files:

- Two binaries for each platform, `strata` and `strata-preflight`.
- The CI scripts.
- The installer.
- A `SHA256SUMS` file over all of them.
- A Sigstore signature for each file.

| Target | Platform |
| --- | --- |
| `x86_64-unknown-linux-musl` | Linux, x86-64. No shared libraries. |
| `aarch64-unknown-linux-musl` | Linux, ARM64. No shared libraries. |
| `x86_64-apple-darwin` | macOS, Intel |
| `aarch64-apple-darwin` | macOS, Apple silicon |

### One command

`install-strata.sh` is in every release. It reads the release and finds the
binary for this machine. It checks the signature of `SHA256SUMS` when `cosign` 3 or
later is on the `PATH`. It checks each file against `SHA256SUMS`. It puts the two
binaries in the directory, and without `--bin-only` also the CI scripts. Then it
prints the path of the strata binary:

```bash
curl -fsSL https://github.com/como-technologies/strata/releases/latest/download/install-strata.sh \
  | bash -s -- --bin-only --dir ~/.local/bin
```

| Option | Effect |
| --- | --- |
| `--version v0.32.2` | Installs that release. The default is `STRATA_VERSION`, else `latest`, the newest release. |
| `--repo OWNER/REPO` | The repository to read. The default is `STRATA_REPO`, else `como-technologies/strata`. |
| `--bin-only` | Installs the two binaries alone. The release after `v0.7.1` is the first one with this option. |
| `--dir DIR` | Where to install. The default is `.strata`. |
| `--target TARGET` | Overrides the target this machine detects. |
| `--require-signature` | Stops with exit `2` before any download when `cosign` 3 or later is not on the `PATH`. Also stops when the release has no signature. |

The installer needs no token. It reads the release from the API of GitHub. When
`STRATA_TOKEN`, `GITHUB_TOKEN`, or `GH_TOKEN` is set, it sends the first of them.
GitHub limits the requests that have no token for each address, and the runners
of a pipeline can share one. So give a token in a pipeline. A private fork in
`--repo` also needs a token.

### One file by hand

```bash
v=v0.32.2; t=x86_64-unknown-linux-musl
url=https://github.com/como-technologies/strata/releases/download/$v
curl -fsSL -O "$url/strata-$t" -O "$url/SHA256SUMS"
sha256sum --check --ignore-missing SHA256SUMS   # macOS: shasum -a 256 --check --ignore-missing SHA256SUMS
chmod +x strata-$t && sudo mv strata-$t /usr/local/bin/strata
```

## Check the signature

The release workflow signs each file of a release with its own identity. A
checksum shows that a file is the file that the release lists. The signature
shows that the release workflow of this repository, at that tag, made the list.
One signature of `SHA256SUMS` covers each file that the list names, because the
installer checks each file against the list.

The installer checks that signature when `cosign` 3 or later is on the `PATH`. It
prints `signature verified` and continues. If the signature does not verify, it
stops with exit `2` and installs nothing.

Without `cosign`, it installs and
prints that it did not check the signature. A `cosign` older than 3 gives the
same result, because the bundles of a release are in the format of `cosign` 3.
The installer prints `signature not verified: cosign older than 3`. Give
`--require-signature` to stop in these cases.

The identity to expect is the release workflow at the tag of the release, and
the issuer is the OIDC service of GitHub Actions:

| Field | Value |
| --- | --- |
| Identity | `https://github.com/como-technologies/strata/.github/workflows/release.yml@refs/tags/<tag>` |
| Issuer | `https://token.actions.githubusercontent.com` |

To check by hand, download the list with its bundle. `$v` and `$url` are those of
[One file by hand](#one-file-by-hand):

```bash
curl -fsSL -O "$url/SHA256SUMS" -O "$url/SHA256SUMS.sigstore.json"
cosign verify-blob SHA256SUMS --bundle SHA256SUMS.sigstore.json \
  --certificate-identity https://github.com/como-technologies/strata/.github/workflows/release.yml@refs/tags/$v \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

Each file has a bundle of its own, `<file>.sigstore.json`, for a check of one
file.

### The reusable workflows

Seven reusable workflows of this repository install strata: `comment.yml`,
`merge-gate.yml`, `preflight.yml`, `promote-gate.yml`, `retire-gate.yml`,
`upstream.yml`, and `verify.yml`. Each one runs the same three steps:

1. It checks out this repository at the tag in `strata-version`, into `.strata`.
2. It installs `cosign` with the action `sigstore/cosign-installer`.
3. It runs `.strata/scripts/ci/install-strata.sh` with `--require-signature`
   and `--bin-only`.

The installer is the only code that checks a release. It makes sure that the
release workflow of this repository signed `SHA256SUMS` at that tag, with the
identity above. Then it checks each binary against the list. The binaries go to
`.strata/bin`. Each workflow gets `strata` and `strata-preflight` there.

The job fails in each of these cases:

| Case | Why the job fails |
| --- | --- |
| The signature does not verify. | The release workflow of this repository did not make the list. |
| The release has no signature. | `--require-signature` makes a missing signature an error. |
| A binary does not match `SHA256SUMS`. | The file is not the file that the release lists. |

The workflow file and the installer come from the same tag, so one tag sets the
trust. Give that tag to the workflow and to `strata-version`. The installer of
v0.24.0 and before has no `--require-signature`, so with such a `strata-version`
the job stops with `unknown option`. An organization that limits the actions of
its repositories must allow `actions/checkout` and `sigstore/cosign-installer`.
`preflight-comment.yml` installs no binary: it runs one script from the tag.

## Install with Cargo

The package of strata on crates.io is `strata-palette`. It gives the command
`strata`:

```bash
cargo install --locked strata-palette
```

The package does not give `strata-preflight`. So `strata preflight` needs the
binaries of a release.

## Build from source

```bash
just build                                        # target/release/strata
just build-target x86_64-unknown-linux-musl       # the same build as a release
```

`just build` runs `cargo build --release`. A Linux target builds in the
containers of `cross`.

Next: [Configure](./configure.md).
