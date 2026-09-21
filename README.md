# strata

[![version](https://img.shields.io/badge/version-0.7.1-blue)](Cargo.toml)
[![the book](https://github.com/como-technologies/strata/actions/workflows/ci.yml/badge.svg)](https://legendary-train-pzq9ojj.pages.github.io/)

Record, compare, and check what Spectro Cloud Palette holds: cluster profile
versions, the clusters that run them, and pack versions in a registry. A
published profile version is what Palette builds a cluster from. Strata calls it
a release.

A profile version is a stack of packs: OS, Kubernetes, CNI, CSI, and add-ons.
Palette lets a person edit a published version in place, and each project holds
its own copy of a version name. Strata calculates one digest for each version,
the root. Equal roots mean one release. When the roots differ, strata names the
layer, the value, and the impact on running clusters.

## Quick start

```bash
export PALETTE_API_KEY=...        # or put it in ~/.palette.env
export PALETTE_PROJECT=dev

strata list                                    # the refs in the project
strata snapshot aws-stack@1.1.1 -o aws-stack.json  # record a release
strata verify aws-stack.json                     # exit 1 when somebody changed it
strata compare aws-stack.json cluster:payments   # does the cluster run it?
strata compare aws-stack@1.1.0 aws-stack@1.1.1     # what changed, and the impact
strata compare pack:csi-aws-ebs@1.63.1 pack:csi-aws-ebs@1.65.0  # two pack versions
```

With no tenant, `just examples` builds real profiles offline. See the
[quick start](docs/src/quickstart.md).

## Commands

| Task | Commands |
| --- | --- |
| Find | `list` |
| Record | `snapshot`, `candidate` |
| Compare | `compare`, `same-release` |
| Check | `verify`, `consumers` |
| Reference | `schema` |

Run `strata --help` for the list, and `strata <command> --help` for the options.

## Install

```bash
gh release download --repo como-technologies/strata --pattern install-strata.sh -O - \
  | GH_TOKEN=$(gh auth token) bash -s -- --bin-only --dir ~/.local/bin
```

The installer finds the binary for this machine and checks it against
`SHA256SUMS`. It installs `strata` alone. To install one release, add
`--version v0.7.1`. The default is the latest release.

[Install](docs/src/install.md) gives the other platforms, the signature check,
and the source build.

## Documentation

The book is at <https://legendary-train-pzq9ojj.pages.github.io/>. It opens for the members of the
organization, as this repository does.

```bash
just docs-serve   # build the book and open it at http://localhost:3000
```

| Page | For |
| --- | --- |
| [Quick start](docs/src/quickstart.md) | the first commands |
| [Use cases](docs/src/use-cases/index.md) | a problem, the commands, the output, and the fix |
| [Commands](docs/src/commands/index.md) | each command, its output, and its options |
| [Automation](docs/src/automation/scripts.md) | scripts, pull request comments, drift issues, the library |

The source is in `docs/src/`. The book uses mdBook with `mdbook-mermaid` and
`mdbook-gruvbox`.

## Requirements

To use strata: the binary, a Palette API key, `jq` for the JSON documents, and
`gh` for the pull request and issue scripts.

To work on strata:

| Tool | Function |
| --- | --- |
| Rust, `just` | Build and test. `cross` builds another target. |
| `jq` | Builds the tenant that the book's commands run against. |
| `mdbook`, `mdbook-mermaid`, `mdbook-gruvbox`, `mdbook-lint` | Build the book and test its links. |
| `tofu`, `crane`, `helm`, AWS CLI | The live example. `just tofu-install` installs `tofu`. |

Install the book tools with
`cargo install mdbook mdbook-mermaid mdbook-gruvbox mdbook-lint`.

## Development

```bash
just lint && just test   # before a commit
just docs-check          # the output in the book is the output of this build
```

[Build and test](docs/src/development/build-test.md) and
[Write the documentation](docs/src/development/documentation.md) give the rules.
