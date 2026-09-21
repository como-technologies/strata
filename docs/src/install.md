# Install

## Requirements

| Tool | Function |
| --- | --- |
| `strata` | The command line. One static binary. |
| A Palette API key | Reads the profiles and clusters. |
| `curl`, `jq` | Read the JSON documents in scripts and in this book, and fetch a release. |
| `gh` | Downloads a release while this repository is private, and checks its signature. Also posts pull request comments and drift issues. |
| `cosign` 3 or later | Checks the signature of a release file. Optional. |
| Rust, `just` | Build from source. `cross` builds another target. |

## Download a release

Each release has one binary for each platform, a `SHA256SUMS` file, and a
Sigstore signature for each file.

| Target | Platform |
| --- | --- |
| `x86_64-unknown-linux-musl` | Linux, x86-64. No shared libraries. |
| `aarch64-unknown-linux-musl` | Linux, ARM64. No shared libraries. |
| `x86_64-apple-darwin` | macOS, Intel |
| `aarch64-apple-darwin` | macOS, Apple silicon |

### One command

`install-strata.sh` is in every release. It reads the release and finds the
binary for this machine. It checks each file against `SHA256SUMS`. Then it
prints the path of the binary:

```bash
gh release download --repo como-technologies/strata --pattern install-strata.sh -O - \
  | GH_TOKEN=$(gh auth token) bash -s -- --bin-only --dir ~/.local/bin
```

| Option | Effect |
| --- | --- |
| `--version v0.7.0` | Installs that release. The default, `latest`, is the newest one. |
| `--bin-only` | Installs `strata` alone. Without it, the CI scripts land beside the binary. |
| `--dir DIR` | Where to install. The default is `.strata`. |
| `--target TARGET` | Overrides the target this machine detects. |

The repository is private, so an anonymous download fails. The installer needs a
token that can read the repository. `gh release download` uses your GitHub login
to get the installer. A piped shell does not inherit that login, so `GH_TOKEN`
gives the token to the installer. In CI, the token of the job is already in the
environment. There, `STRATA_TOKEN`, `GITHUB_TOKEN` or `GH_TOKEN` is enough.

When the repository becomes public, the same script needs no token. `curl` then
gets it directly:

```bash
curl -fsSL https://github.com/como-technologies/strata/releases/latest/download/install-strata.sh \
  | bash -s -- --bin-only --dir ~/.local/bin
```

### One file by hand

```bash
v=v0.7.1; t=x86_64-unknown-linux-musl
gh release download $v --repo como-technologies/strata --pattern strata-$t --pattern SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS   # macOS: shasum -a 256 --check --ignore-missing SHA256SUMS
chmod +x strata-$t && sudo mv strata-$t /usr/local/bin/strata
```

## Check the signature

The release workflow signs each file with its own identity. The check names
the repository and the workflow:

```bash
gh release download $v --repo como-technologies/strata --pattern strata-$t.sigstore.json
cosign verify-blob strata-$t --bundle strata-$t.sigstore.json \
  --certificate-identity-regexp '^https://github.com/como-technologies/strata/\.github/workflows/release\.yml@refs/tags/v' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

## Build from source

```bash
just build                                        # target/release/strata
just build-target x86_64-unknown-linux-musl       # the same build as a release
```

`just build` runs `cargo build --release`. A Linux target builds in the
containers of `cross`.

Next: [Configure](./configure.md).
