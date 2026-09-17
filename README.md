# strata

Record, compare, and check Spectro Cloud Palette cluster profile versions as
releases.

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
strata snapshot aws-pxk@1.1.1 -o aws-pxk.json  # record a release
strata verify aws-pxk.json                     # exit 1 when somebody changed it
strata compare aws-pxk.json cluster:payments   # does the cluster run it?
strata compare aws-pxk@1.1.0 aws-pxk@1.1.1     # what changed, and the impact
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
v=v0.5.0; t=x86_64-unknown-linux-musl
gh release download $v --repo como-technologies/strata --pattern strata-$t --pattern SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
chmod +x strata-$t && sudo mv strata-$t /usr/local/bin/strata
```

[Install](docs/src/install.md) gives the other platforms, the signature check,
and the source build.

## Documentation

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
| `python3` | Runs the commands of the book: `just docs-examples`. |
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
