# The everyday loop

Palette builds a cluster from a published profile version. After a team approves
a version, its content can still change, and its name and number stay the same.
Strata keeps a copy of the approved content in your repository, and it tells you
when Palette holds something else.

Strata runs in your pipeline and at your terminal. [The observer](./automation/observer.md)
runs it in one cluster, on a timer, and shows the answers on a page. Strata reads
Palette and writes files. It publishes nothing and deletes nothing, so each write
stays in your pipeline.

## When to use strata

Start from the question that you have.

| Your question | What answers it |
| --- | --- |
| Is this cluster healthy? Is it behind its profile? Who changed this object? | Palette: [health alerts](https://docs.spectrocloud.com/clusters/cluster-management/health-alerts/), [update notifications](https://docs.spectrocloud.com/profiles/cluster-profiles/modify-cluster-profiles/update-cluster-profile/), and [the audit log](https://docs.spectrocloud.com/audit-logs/). |
| How do I publish a version, or build or upgrade a cluster? | Palette, through your pipeline or [the Terraform provider](https://registry.terraform.io/providers/spectrocloud/spectrocloud/latest/docs). Strata does none of this. |
| What does this change do to my clusters, before I merge it? | `strata compare`. It names each pack that changes, and the impact, such as a repave. |
| Does Palette still hold what we approved? | `strata verify`. |
| Does this cluster run the release that we approved? | `strata compare`. |
| Do `dev` and `prod` hold one release? | `strata same-release`. |
| Do I want these answers all day, on a page in the cluster, with the name of the person who made a change? | [The observer](./automation/observer.md). It runs `verify`, `snapshot`, `compare`, and `behind` on a timer. |
| Which digest does each tag run as on this cluster? | [The observer](./automation/observer.md#the-pods-of-the-cluster), with the optional read of the pods. |
| Do I want a message in a channel of my team when a finding opens? | [The observer](./automation/observer.md#alerts), with the optional alerts. Palette has no event for a published version that changed, so the observer sends one. |
| Can Palette build this cluster? | `strata preflight`. It checks the body of the create and asks Palette, before the build starts. |

[Where each part belongs](./boundary.md) gives each feature of Palette, and what
strata adds beside it. [Bring your own pipeline](./internals/sample-pipeline.md)
says where strata fits in the pipeline that your team has, with one sample
pipeline and its versions.

**Your own tooling stays.** Your pipeline still publishes, and your scripts still
decide what happens next. Strata gives them an exit code: `0` for the same, `1`
for a difference, and `2` for no answer.

**You can write this check yourself, but a comparison of names is not enough.**
A name and a number stay when the content changes. So the check must mask each
password, put the content in one order, hash it, and keep the copy. It must also
say what changed, and what the change does to a cluster. Strata is that part of a
pipeline.

## Start from nothing

Add one part at a time. Each part works alone, so you can stop after any step
and keep what you have. The order starts with the part that you see on each change.

<div class="playground-demo">

`just playground quickstart` serves a tenant on this machine, with no key and no
cost. It holds the names below, so you can do steps 1 and 2 with no Palette account.

</div>

### 1. Connect to your tenant

```bash
export PALETTE_API_KEY=...        # the API key
strata list projects              # test the key: the projects that it can see
export PALETTE_PROJECT=dev        # one name from that list
strata list                       # the refs in that project
```

[Install](./install.md) and [Configure](./configure.md) give the rest.

### 2. Record what you run today

Use your own names from `strata list`.

```bash
mkdir -p releases
strata snapshot aws-stack@1.1.0 -o releases/dev.json
git add releases && git commit -m "Record the release of dev"

strata verify releases/dev.json                     # exit 1 when somebody changes it
strata compare releases/dev.json cluster:payments   # does the cluster run it?
```

The record is a file with the content of the version and its root, which is one
digest of that content. These commands change nothing in Palette. You now hold a
copy of what you run, in Git.

A cluster can run more profiles than the record, such as the add-on profiles of
each team. The comparison then holds only the profile of the record, so exit
`0` says that the cluster runs it. See
[Compare a profile version with a cluster](./commands/compare/clusters.md#compare-a-profile-version-with-a-cluster).

### 3. Comment on each pull request

Add this job first. It shows what a change does when a person reviews it. It reads
Palette and writes one comment, and it blocks nothing.

1. Copy
   [`examples/ci/github/strata.yml`](https://github.com/como-technologies/strata/blob/main/examples/ci/github/strata.yml)
   to `.github/workflows/strata.yml`. For GitLab, see [GitLab CI](./automation/gitlab.md).
2. Keep the job `comment`, and delete the other jobs. Each step below puts a job back.
3. Set `a` to the record of your default branch, and `b` to what the pull request
   proposes. `b` is a candidate file. It is a version in Palette when your pipeline
   publishes each pull request as a version, as
   [the sample pipeline](./internals/sample-pipeline.md#the-candidate-merge-request)
   does.
4. Set the secrets that the top of the file names, such as `PALETTE_API_KEY`.

To see the comment first, run the script with `--dry-run`. It reads Palette and
posts nothing:

```bash
scripts/ci/pr-comment.sh releases/dev.json aws-stack@1.1.1 --dry-run
```

Each pull request then gets one comment, with each pack that changes and its
impact. See [Pull request comments](./automation/pr-comments.md).

### 4. Check each night

A pull request shows the changes that go through a review. This job finds the
others, such as an edit in place. Keep the job `verify` and the `schedule` in the
same file.

To see the result first, run the script with `--dry-run`. It reads Palette and
opens no issue:

```bash
scripts/ci/verify-report.sh 'releases/**/*.json' --dry-run
```

The job keeps one open issue for each record that drifted. It closes the issue
when the record passes again. See [Drift issues](./automation/drift-issues.md).

### 5. Add the gates that you need

Add each row when you need it. Each job is in the same file.

| Add this when | The job or the command | Read |
| --- | --- | --- |
| A merge must stop when Palette changed after the review, or when a pin moves back. | the job `gate`, as a required check | [The merge gate](./automation/merge-gate.md) |
| A publish must hold the content that the reviewer read. | `strata same-release`, after the publish | [Review a change before you publish it](./use-cases/review.md) |
| You copy a release to `prod`. | the job `promote-gate` | [Promote a release to another project](./use-cases/promote.md) |
| You delete old versions. | the job `retire-gate` | [Retire a version](./use-cases/retire.md) |
| You want an issue when a pack that you pin has a newer version. | the job `upstream` | [Upstream issues](./automation/upstream-issues.md) |
| A build fails late, after the work in the cloud starts. | the job `preflight` | [Can Palette build this cluster?](./use-cases/stop-a-build.md) |
| You want the answers all day on a page, between two night jobs. | the observer, and `observer-records.sh` in the publish step | [The observer](./automation/observer.md) |

## A day of work

When the jobs run, most days are quiet. This section follows one day, in the
order that a team meets each answer.

### In the morning

The night job has run. No open issue with the label `strata-drift` means that
each record passed.

This morning, one record drifted:

```text
{{#include output/loop-nightly.txt}}
```

Somebody edited `aws-stack@1.1.0` in `prod`, outside a review. Ask who:

```text
{{#include output/loop-who.txt}}
```

Then decide. [What to do](./use-cases/edited-in-place.md#what-to-do) gives the
choices.

A second night job opens an issue when a pack that you pin has a newer version.
That is a decision for a person, not a fault. See
[Upstream issues](./automation/upstream-issues.md).

### You change a profile

You write the new version as a candidate on a branch, and you open a pull
request. The job posts one comment. This is its result line:

```text
{{#include output/loop-review.txt}}
```

The comment names each pack and its impact. The maximum impact here is `repave`,
so the job adds the label `repave`. Your rules can then ask for a second approver.

### You merge

The merge gate checks the records after the merge, not the branch as it was when
you opened it. It stops the merge when Palette changed after the review, or when
the merge moves a pin back:

```text
{{#include output/loop-merge.txt}}
```

### The pipeline publishes

Your pipeline publishes the candidate. Then it records the published version, and
asks if that is what the reviewer read:

```text
{{#include output/loop-publish.txt}}
```

`IDENTICAL` means that the content is the same, by its digest, and not by a tag or
a name. A difference fails the job.

When your team runs [the observer](./automation/observer.md), the publish step also
writes the records into the cluster. Name the records of the project of the cluster, one for each
version. Here the observer watches the cluster `payments` of `dev`, so these are the
records `releases/*/dev.json`. The observer then compares Palette with the records
that the step wrote:

```bash
scripts/ci/observer-records.sh --revision "$GIT_SHA" releases/*/dev.json \
  | kubectl replace -f -
```

A replace removes each record that the step does not write, also a first-seen record
of the seed.

The script stops when two records are for one version. So the glob must not match
the records of `prod`, which hold the same versions. It stops for a record of a
cluster too, such as `fleet/prod/payments.json`. This layout keeps the records of `dev`
and of `prod` in each version folder. So the glob names `dev`, and it differs from the
glob `releases/*.json` of [Install](./automation/observer.md#install), which fits a
folder with the records of one project.

The observer can also read the pods of the cluster, to show the digest that each tag
runs. This read is optional, and it needs its own role and token: see [The pods of the
cluster](./automation/observer.md#the-pods-of-the-cluster). A team that does not add it
keeps the observer with no role and no token.

The observer can also send a message to a webhook when a finding opens, and one when it
clears. This send is optional too. It needs a Secret with the address of each webhook:
see [Alerts](./automation/observer.md#alerts). Palette has no event for a published
version that changed, so the observer sends one until Palette has it.

### You move a cluster

Before the move, ask Palette if the move replaces the nodes:

```text
{{#include output/loop-ask-repave.txt}}
```

The `palette` line is the answer of Palette. Your team decides what to do with it.

### Some days

| What happens | What you run | Read |
| --- | --- | --- |
| You copy the release to `prod`. | `strata same-release releases/dev.json releases/prod.json` | [Promote a release to another project](./use-cases/promote.md) |
| You ask if `dev` and `prod` hold one release now, with no record. | `strata same-release <name>@<version> <name>@<version> --project-name-a dev --project-name-b prod` | [Environments in the projects of one tenant](./use-cases/projects.md) |
| Your pipeline publishes the same content under its final numbers. | `strata same-release --allow renumber releases/rc.json releases/final.json` | [A release gets new version numbers](./use-cases/renumber.md) |
| You write a new cluster. | `strata preflight cluster.json --cloud eks` | [Can Palette build this cluster?](./use-cases/stop-a-build.md) |
| A pack has a security fault. | `strata list --pack <name>`, then `strata consumers <name>@<version>` | [Find the profiles that carry a pack](./use-cases/find-a-pack.md) |
| You delete an old version. | `strata consumers <name>@<version>` | [Retire a version](./use-cases/retire.md) |
| A job writes a note about a value that strata does not know. | `strata report` | [Palette gives a value that strata does not know](./use-cases/unknown-value.md) |

## See all of it

[One change, start to end](./walkthrough.md) follows one change through each step,
with the output of each command and a playground for each step. It also has the
table of each job, for the person who writes the pipeline.
