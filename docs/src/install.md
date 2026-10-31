# Install

## Requirements

| Tool | Function |
| --- | --- |
| `strata` | The command line. One static binary. |
| `strata-preflight` | The preflight checks; `strata preflight` runs it from the same directory. One static binary, in every release from the one after v0.15.0. |
| A Palette API key | Reads the profiles and clusters. |
| `curl`, `jq` | Read the JSON documents in scripts and in this book, and fetch a release. |
| `gh` | Downloads a release while this repository is private. Also posts pull request comments and drift issues. |
| `cosign` 3 or later | Checks the signature of `SHA256SUMS`. Optional, unless you give `--require-signature`. |
| Rust, `just` | Build from source. `cross` builds another target. |

## Download a release

Each release has two binaries for each platform, `strata` and
`strata-preflight`, the CI scripts, the installer, a `SHA256SUMS` file over all
of them, and a Sigstore signature for each file.

| Target | Platform |
| --- | --- |
| `x86_64-unknown-linux-musl` | Linux, x86-64. No shared libraries. |
| `aarch64-unknown-linux-musl` | Linux, ARM64. No shared libraries. |
| `x86_64-apple-darwin` | macOS, Intel |
| `aarch64-apple-darwin` | macOS, Apple silicon |

### One command

`install-strata.sh` is in every release. It reads the release and finds the
binary for this machine. It checks the signature of `SHA256SUMS` when `cosign`
is on the `PATH`. It checks each file against `SHA256SUMS`. It puts the binary and
the three CI scripts in the directory. Then it prints the path of the binary:

```bash
gh release download --repo como-technologies/strata --pattern install-strata.sh -O - \
  | GH_TOKEN=$(gh auth token) bash -s -- --dir ~/.local/bin
```

| Option | Effect |
| --- | --- |
| `--version v0.24.0` | Installs that release. The default is `STRATA_VERSION`, else `latest`, the newest release. |
| `--repo OWNER/REPO` | The repository to read. The default is `STRATA_REPO`, else `como-technologies/strata`. |
| `--bin-only` | Installs the two binaries alone. The release after `v0.7.1` is the first one with this option. |
| `--dir DIR` | Where to install. The default is `.strata`. |
| `--target TARGET` | Overrides the target this machine detects. |
| `--require-signature` | Stops with exit `2` before any download when `cosign` is not on the `PATH`. Also stops when the release has no signature. |

The repository is private, so an anonymous download fails. The installer reads a
token from `STRATA_TOKEN`, `GITHUB_TOKEN`, or `GH_TOKEN`, in that order. The
token must read this repository. `gh release download` uses your GitHub login to
get the installer. A piped shell does not inherit that login, so `GH_TOKEN`
gives the token to the installer. The token of a job in another repository
cannot read this one, so a pipeline sets a token of its own: the drop-in passes
it as `STRATA_READ_TOKEN` on GitHub and as `STRATA_TOKEN` on GitLab.

When the repository becomes public, the same script needs no token. `curl` then
gets it directly:

```bash
curl -fsSL https://github.com/como-technologies/strata/releases/latest/download/install-strata.sh \
  | bash -s -- --dir ~/.local/bin
```

### One file by hand

```bash
v=v0.24.0; t=x86_64-unknown-linux-musl
gh release download $v --repo como-technologies/strata --pattern strata-$t --pattern SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS   # macOS: shasum -a 256 --check --ignore-missing SHA256SUMS
chmod +x strata-$t && sudo mv strata-$t /usr/local/bin/strata
```

## Check the signature

The release workflow signs each file of a release with its own identity. A
checksum shows that a file is the file that the release lists. The signature
shows that the release workflow of this repository, at that tag, made the list.
One signature of `SHA256SUMS` covers each file that the list names, because the
installer checks each file against the list.

The installer checks that signature when `cosign` is on the `PATH`. It prints
`signature verified` and goes on. If the signature does not verify, it stops
with exit `2` and installs nothing. Without `cosign`, it installs and prints that
it did not check the signature. Give `--require-signature` to stop in that case.

The identity to expect is the release workflow at the tag of the release, and
the issuer is the OIDC service of GitHub Actions:

| Field | Value |
| --- | --- |
| Identity | `https://github.com/como-technologies/strata/.github/workflows/release.yml@refs/tags/<tag>` |
| Issuer | `https://token.actions.githubusercontent.com` |

To check by hand, download the list with its bundle:

```bash
gh release download $v --repo como-technologies/strata --pattern SHA256SUMS --pattern SHA256SUMS.sigstore.json
cosign verify-blob SHA256SUMS --bundle SHA256SUMS.sigstore.json \
  --certificate-identity https://github.com/como-technologies/strata/.github/workflows/release.yml@refs/tags/$v \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

Each file has a bundle of its own, `<file>.sigstore.json`, for a check of one
file.

### The reusable workflows

Each reusable workflow of this repository that downloads a binary installs
`cosign` with the action `sigstore/cosign-installer`. It checks the signature of
`SHA256SUMS` with the identity above, at the release in `strata-version`, before
it checks the binary against the list. If the signature does not verify, the job
fails. An organization that limits the actions of its repositories must allow
`sigstore/cosign-installer`. `preflight-comment.yml` downloads no binary: it
runs one script from the tag.

## Build from source

```bash
just build                                        # target/release/strata
just build-target x86_64-unknown-linux-musl       # the same build as a release
```

`just build` runs `cargo build --release`. A Linux target builds in the
containers of `cross`.

Next: [Configure](./configure.md).
