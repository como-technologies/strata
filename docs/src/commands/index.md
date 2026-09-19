# Commands

`strata --help` lists every command, and `strata <command> --help` gives the
options of one:

```text
{{#include ../output/help.txt}}
```

## The commands in a release cycle

```mermaid
flowchart LR
  subgraph Find
    list
    packs
  end
  subgraph Record
    snapshot
    candidate
  end
  subgraph Compare
    compare
    same-release
  end
  subgraph Check
    verify
    consumers
  end
  list --> snapshot --> compare --> verify
  candidate --> snapshot
  snapshot --> same-release
  verify --> consumers
```

| Task | Command | Page |
| --- | --- | --- |
| Find a ref, a project, or the profiles with a pack | `strata list` | [list](./list.md) |
| Find the versions a registry offers for a pack | `strata packs` | [packs](./packs.md) |
| Record a profile version or a cluster | `strata snapshot` | [snapshot](./snapshot.md) |
| Write a candidate file for a review | `strata candidate` | [candidate](./candidate.md) |
| See what changed between two refs | `strata compare` | [compare](./compare.md) |
| Make sure that records in several projects are one release | `strata same-release` | [same-release](./same-release.md) |
| Make sure that Palette still has what you recorded | `strata verify` | [verify](./verify.md) |
| Find the clusters that use a version | `strata consumers` | [consumers](./consumers.md) |
| Get the JSON Schema of a document | `strata schema` | [schema](./schema.md) |

## Refs

A ref names what a command reads. [Refs](../reference/refs.md) gives the
details.

| Ref | Example |
| --- | --- |
| A profile uid | `64e0000000000000000000c3` |
| A name and a version | `aws-stack@1.1.1` |
| The highest version of a name | `aws-stack@latest` |
| All profiles that a cluster runs | `cluster:payments` |
| One profile that a cluster runs | `cluster:payments/aws-stack` |
| A pack version in a registry | `pack:csi-aws-ebs@1.65.0` |
| A snapshot file | `releases/1.1.1/dev.json` |

## Exit status

| Exit | `compare` | `same-release` | `verify` | `consumers` |
| --- | --- | --- | --- | --- |
| `0` | identical | each pair passes | each record unchanged | no cluster uses it |
| `1` | different | a pair fails | a record drifted | a cluster uses it |
| `2` | error | error | strata cannot check a record | error |

`list`, `packs`, `snapshot`, `candidate`, and `schema` exit `0`, or `2` on an
error.
Treat `2` as a failed job. It never means "no change".

## Output formats

`compare`, `same-release`, `verify`, `consumers`, `list`, and `packs` take the
same options:

| Option | Output |
| --- | --- |
| `--format terminal` | The default. Text, with color on a terminal. |
| `--format markdown` | Tables for a pull request comment or an issue. |
| `--format json`, `--json` | A JSON document for a script. |
| `--compact` | The JSON on one line. |
| `--color auto\|always\|never`, `--no-color` | Color of the terminal text. |
| `-q`, `--quiet` | No progress messages on standard error. |

[Report formats](../reference/formats.md) and
[JSON documents](../reference/documents.md) describe each output.
