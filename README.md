# strata

[![version](https://img.shields.io/badge/version-0.8.0-blue)](Cargo.toml)
[![the book](https://github.com/como-technologies/strata/actions/workflows/ci.yml/badge.svg)](https://legendary-train-pzq9ojj.pages.github.io/)

Record, compare, and check what Spectro Cloud Palette holds: cluster profile
versions, the clusters that run them, and pack versions in a registry. A
published profile version is what Palette builds a cluster from. Strata calls it
a release.

A cluster profile is a set of layers, kept in the order given: OS, Kubernetes,
CNI, CSI, and add-ons. Strata calculates one digest for each version, the root.
Equal roots mean one release. When the roots differ, strata names the layer, the
value, and the impact on running clusters.
[Introduction](docs/src/introduction.md) gives the rest of the model.

## Quick start

```bash
export PALETTE_API_KEY=...        # or put it in ~/.palette.env
export PALETTE_PROJECT=dev

strata list                                                     # the refs in the project
strata snapshot aws-stack@1.1.1 -o aws-stack.json               # record a release
strata verify aws-stack.json                                    # exit 1 when somebody changed it
strata compare aws-stack.json cluster:payments                  # does the cluster run it?
strata compare aws-stack@1.1.0 aws-stack@1.1.1                  # what changed, and the impact
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
  | GH_TOKEN=$(gh auth token) bash -s -- --dir ~/.local/bin
```

[Install](docs/src/install.md) gives the options, the other platforms, the
signature check, and the source build.

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

The source is in `docs/src/`.

## Requirements

To use strata: the binary, a Palette API key, `jq`, and `gh`.
[Install](docs/src/install.md) lists each.

To work on strata:
[Build and test](docs/src/development/build-test.md) and
[Write the documentation](docs/src/development/documentation.md) list the tools
and how to install them.

## Development

```bash
just lint && just test   # before a commit
just docs-check          # the output in the book is the output of this build
```

[Build and test](docs/src/development/build-test.md) and
[Write the documentation](docs/src/development/documentation.md) give the rules.
[CONTRIBUTING](CONTRIBUTING.md) gives the steps for a pull request,
[CODE_OF_CONDUCT](CODE_OF_CONDUCT.md) gives the conduct this project keeps, and
[SECURITY](SECURITY.md) says how to report a vulnerability.

## License

Apache License 2.0. See [LICENSE](LICENSE).
