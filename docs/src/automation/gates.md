# The promotion gate and the retire gate

Two questions are gates: a job asks strata, and the next step runs only on exit
`0`. Strata publishes nothing and deletes nothing. The step that acts is yours.

| Gate | The question | The command | Exit `1` means |
| --- | --- | --- | --- |
| promotion | Are the records of this promotion one release? | `same-release` | a pair differs, and the reason names the pack and the value path |
| retire | Does a cluster that the key can read still run this version? | `consumers` | a cluster does, or strata could not read one, and the report names each one |

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
    uses: como-technologies/strata/.github/workflows/promote-gate.yml@v0.7.1
    with:
      records: ${{ inputs.promote }}
      strata-version: v0.7.1
    secrets:
      strata-token: ${{ secrets.STRATA_READ_TOKEN }}
      palette-api-key: ${{ secrets.PALETTE_API_KEY }}
```

| Input | Gate | Function |
| --- | --- | --- |
| `records` | promotion | The records, in promotion order, separated by spaces. |
| `allow` | promotion | `registry-move`, for a promotion that moves packs to another registry. |
| `version` | retire | The version to retire, as `name@version` or a profile uid. |
| `project` | retire | The project that holds the version, by name; empty is tenant scope. |
| `palette-api` | both | A self-hosted Palette; empty is `https://api.spectrocloud.com`. |
| `strata-version` | both | The strata release, a tag of this repository. |

The promotion gate reads records, which are files, so it needs
`palette-api-key` only when a record is a ref. The retire gate reads the
clusters, so it always needs the key.

## On GitLab

`examples/ci/gitlab/strata.yml` holds `strata-promote-gate` and
`strata-retire-gate`. Both are manual jobs: set `STRATA_PROMOTE` to the records,
or `STRATA_RETIRE` to `name@version`, and run the job from the pipeline. See
[GitLab CI](./gitlab.md).

## Put the delete after the gate

```bash
strata consumers aws-stack@1.0.0 && your_delete_command
```

`&&` runs the delete only on exit `0`. A cluster that strata cannot read counts
as a consumer, so it exits `1` and the delete does not run.

Exit `0` means that no cluster in the projects the key can read applies the
version. Give the retire gate a key that reads every project of the tenant. A
key of one project misses a cluster of another project that runs a tenant
profile. See [strata consumers](../commands/consumers.md).

On GitLab, put that line in the `script:` of `strata-retire-gate`. On GitHub the
drop-in's job calls a reusable workflow, and such a job holds no steps of its
own, so the delete goes in a second job:

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
