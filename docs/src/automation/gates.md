# The promotion gate and the retire gate

Two questions are gates: a job asks strata, and the next step runs only on exit
`0`. Strata publishes nothing and deletes nothing. The step that acts is yours.

| Gate | The question | The command | Exit `1` means |
| --- | --- | --- | --- |
| promotion | Are the records of this promotion one release? | `same-release` | a pair differs, and the reason names the pack and the value path |
| retire | Does a cluster or an edge host that the key can read, or a cluster template, still use this version? | `consumers` | one does, or strata could not read a cluster, and the report names each one |

Both take a moment that a person picks, so both run on demand. A promotion gate
on every pull request goes red whenever one environment is ahead of another,
which is the normal state.

## In a terminal

```bash
strata same-release releases/dev.json releases/prod.json
strata consumers aws-stack@1.0.0
```

Give the records in promotion order. `consumers` asks Palette at the time of the
command, so run it immediately before the delete.

## On GitHub

`examples/ci/github/strata.yml` holds both jobs with the other three, ready to
copy. Each one runs from **Actions > strata > Run workflow**, with the records
or the version as the input:

```yaml
  promote-gate:
    if: github.event_name == 'workflow_dispatch' && inputs.promote != ''
    uses: como-technologies/strata/.github/workflows/promote-gate.yml@v0.32.2
    with:
      records: ${{ inputs.promote }}
      strata-version: v0.32.2
    secrets:
      strata-token: ${{ secrets.STRATA_READ_TOKEN }}
      palette-api-key: ${{ secrets.PALETTE_API_KEY }}
```

| Input | Gate | Function |
| --- | --- | --- |
| `records` | promotion | The records, in promotion order, separated by spaces. |
| `allow` | promotion | The differences to accept, separated by spaces or new lines: `registry-move` for a promotion that moves packs to another registry, and `no-digest`. |
| `version` | retire | The version to retire, as `name@version` or a profile uid. |
| `project` | both | The project that holds a ref such as `name@version`, by name; empty is tenant scope. A record that is a file needs none. |
| `palette-api` | both | A self-hosted Palette; empty is `https://api.spectrocloud.com`. |
| `strata-version` | both | The strata release, a tag of this repository. |

While this repository is private, the caller needs the access setting as well as `strata-token`: see [While this repository is private](./pr-comments.md#while-this-repository-is-private).

The promotion gate reads records, which are files, so it needs
`palette-api-key` only when a record is a ref. A ref such as `name@version`
resolves in one project. The promotion gate takes `project`, or the secret
`palette-project-uid`, which names the project by its uid and has priority over
`project`. The retire gate reads the clusters, so it always needs the key.

The workflow gives each difference of `allow` its own `--allow`, so
`registry-move no-digest` runs `strata same-release` with
`--allow registry-move --allow no-digest`.

## On GitLab

`examples/ci/gitlab/strata.yml` holds `strata-promote-gate` and
`strata-retire-gate`. Both are manual jobs: set `STRATA_PROMOTE` to the records,
or `STRATA_RETIRE` to `name@version`, and run the job from the pipeline.
`STRATA_PROMOTE_ALLOW` takes the place of the input `allow`. A ref in a project
reads `PALETTE_PROJECT` or `PALETTE_PROJECT_UID`. [GitLab
CI](./gitlab.md#the-variables) says what each variable does.

## Put the delete after the gate

```bash
strata consumers aws-stack@1.0.0 && your_delete_command
```

`&&` runs the delete only on exit `0`. A cluster that strata cannot read counts
as a consumer, so it exits `1` and the delete does not run. When strata finds no
consumer and a reading failed, it exits `2`, and the delete does not run either:
a consumer can be missing.

Exit `0` means that no cluster in the projects the key can read applies the
version, and no edge host is bound to it. Do not add `--list-only` to the gate:
it reads the consumer list of Palette alone, and that list is also empty when
the read fails. Give the retire gate a key that reads every project of the tenant. A
key of one project misses a cluster of another project that runs a tenant
profile. See [strata consumers](../commands/consumers.md).

To see the reach of a key,
run `strata list clusters --tag <label>`. It searches every project that the
key can read, and it names the project of each cluster.

On GitLab, put that line in the `script:` of `strata-retire-gate`. On GitHub the
drop-in's job calls a reusable workflow. Such a job holds no steps of its own,
so the delete goes in a second job:

```yaml
  delete:
    needs: retire-gate
    runs-on: ubuntu-latest
    steps:
      - run: your_delete_command
```

`needs:` runs the delete only when the gate passed. It also puts minutes and
another runner between the check and the delete. A cluster can start the version
in that time. A team that needs the two together runs
`strata consumers <name>@<version> && your_delete_command` in one ordinary job,
with the binary installed by `scripts/ci/install-strata.sh`.

See
[Retire a version](../use-cases/retire.md) and
[Promote a release to another project](../use-cases/promote.md).
