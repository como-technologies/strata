# Upstream issues

`scripts/ci/upstream-report.sh` runs `strata behind` on a schedule. It keeps one
issue open for each pack that a record pins behind a newer version of its
registry, and closes the issue when each pin of the pack is current.

It is the other half of [Drift issues](./drift-issues.md), and it is a separate
job on purpose. Drift says that a version changed under its record: it is an
error.
Behind says that the registry has a newer version: it is a decision. One job
that reports both fails for two reasons, and each reason needs another person.

## What the script does

```text
{{#include ../output/behind-report.txt:1:14}}
```

Each issue has the marker `<!-- strata-behind: <pack> <registry uid> -->` on its
first line, and the label `strata-behind`:

- **Opened** on the first run that finds the pack behind. The issue says that
  this run is the first to find it behind.
- **Updated** on each later run. The issue gives the pinned versions, and the
  newer versions with the time Palette first listed each. It gives the impact of
  the bump, the versions that arrived since the last update, and each record
  that pins the pack.
- **Closed**, with a comment, on the first run where each pin of the pack is
  current.

A pack that strata could not check keeps its issue as it is. An issue of a pack
that no record pins is left alone.

The bump column of an issue gives the impact of the move to the newest version. It
also says whether Palette accepts the move, with the lines that `strata behind`
shows under the bump: `palette accepts`; `palette refuses` with the code and the
clusters, one line for each error; or `palette gave no answer` with the reason.
See
[Palette accepts the move, or refuses it](../commands/behind.md#palette-accepts-the-move-or-refuses-it).

## Two runs name what arrived

The second line of each issue keeps the pins of its pack as JSON, in a comment
that the host does not show. The next run gives them to `strata behind --since`,
so the issue names each version that arrived since its last update, with no
storage outside the code host. `--save FILE` keeps the report of the whole run,
for a job that stores it.

## Run it

```bash
scripts/ci/upstream-report.sh 'releases/**/*.json'
```

| Option | What it does |
| --- | --- |
| `--repo OWNER/REPO` | The repository of the issues. The CI variables give it on a runner. |
| `--host github\|gitlab` | The code host. The CI variables choose it when this is absent. |
| `--label NAME` | The label of the issues. `strata-behind` by default. |
| `--no-impact` | Do not compare each pin that is behind with the newest version, and do not ask Palette about the move. |
| `--save FILE` | Keep the report of the run. |
| `--dry-run` | Print each issue body and the action for each issue, and call no host. |

| Exit | Meaning |
| --- | --- |
| `0` | No pin is behind. |
| `1` | A pin is behind. |
| `2` | A pin could not be checked, or strata checked nothing. |

The script handles the issues first, so a scheduled job shows the state too.
When strata checked nothing, the script exits `2` and touches no issue. The
script also exits `2` when strata prints nothing, with each `jq` from 1.6.

## On a host

The GitHub drop-in `examples/ci/github/strata.yml` runs the job `upstream` on
the schedule of the drift report, through the reusable workflow
`.github/workflows/upstream.yml`. The GitLab drop-in
`examples/ci/gitlab/strata.yml` runs the job `strata-upstream` on a scheduled
pipeline, with the label `STRATA_BEHIND_LABEL`. The host functions are the ones
of `verify-report.sh`: see [GitLab CI](./gitlab.md) for the token that the
issues need.
