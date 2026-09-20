# Install

## Requirements

| Tool | Function |
| --- | --- |
| `strata` | The command line. One static binary. |
| A Palette API key | Reads the profiles and clusters. |
| `jq` | Reads the JSON documents in scripts and in this book. |
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

```bash
v=v0.6.3; t=x86_64-unknown-linux-musl
gh release download $v --repo como-technologies/strata --pattern strata-$t --pattern SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS   # macOS: shasum -a 256 --check --ignore-missing SHA256SUMS
chmod +x strata-$t && sudo mv strata-$t /usr/local/bin/strata
```

The repository is private, so an anonymous download fails. `gh` uses your GitHub
login. When the repository becomes public, `curl` does the same:

```bash
base=https://github.com/como-technologies/strata/releases/download/$v
curl -fsSLO $base/strata-$t && curl -fsSLO $base/SHA256SUMS
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
