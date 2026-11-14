# Retire a version

## The problem

A team wants to delete old profile versions. A version that a cluster uses must
stay until the cluster moves to a newer version. Palette refuses the delete of a
version that a cluster, an edge host, or a cluster template uses.

In this example, `dev` has `aws-stack@1.0.0`, which no cluster uses,
`aws-stack@1.1.0`, which `payments` and `search` run, and `aws-stack@1.1.1`,
which no cluster runs and the cluster template `aws-standard` links.

**Who runs this.** A person at a terminal, or the retire gate, which runs
`consumers` for a job: see [The promotion gate and the retire gate](../automation/gates.md).
Every step reads Palette, so each one needs a key.

<div class="playground-demo">

`just playground retire` serves this tenant on this machine,
with no key and no cost. Nothing on this page changes the tenant, so one seed
serves steps 1 to 5. Step 6 names its own seed.

</div>

## 1. A version in use

```text
{{#include ../output/retire-in-use.txt}}
```

Exit `1`, and a row for each cluster: `payments` and `search` both run 1.1.0.
Move them first.

## 2. A version that no cluster uses

```text
{{#include ../output/retire-free.txt}}
```

`no consumers` and exit `0`: no cluster, edge host, or cluster template uses
the version, and Palette will accept the delete. The rows under `searched:` say
what strata read: the cluster search names two clusters that run other versions
of `aws-stack`. The line `scope:` says that only the clusters of `dev` can use
the version. When strata finds no consumer and a reading failed, the first line
says so, and the exit is `2`. A consumer can be missing: do not delete.

## 3. A version that a cluster template links

```text
{{#include ../output/retire-template.txt}}
```

Exit `1`. No cluster runs 1.1.1, but the template `aws-standard` links it, and
Palette refuses the delete while it does. Link the template to a newer version,
or delete the template, first.

## 4. Gate the delete

```text
{{#include ../output/retire-gate.txt}}
```

Strata deletes nothing. The delete is your own command, from the Spectro Cloud
provider, the Palette API, or the console. Put it after `&&`, so it runs only on
exit `0`. A cluster that strata cannot read still counts, so the delete does not
run.

## 5. The clusters in a script

```text
{{#include ../output/retire-json.txt}}
```

The third column is the version that the copy of each cluster names, which is
not always the version the uid holds now: see
[A version bump overwrites a version](./overwritten.md).

A pipe hides the exit code of step 4. `strata consumers` exits `1` here, and the
shell reports the exit of `jq`, which is `0`. A script that reads the rows and
gates on the exit must set `pipefail`, or run the two commands apart.

## 6. A version that each project can use

<div class="playground-demo">

`just playground retire-tenant-2` serves the tenant of this step on this machine, with
no key and no cost.

</div>

Each project of the tenant can use a profile of the tenant. Here
`aws-stack@2.0.0` is such a profile, and the key cannot read the project
`staging`:

```text
{{#include ../output/retire-tenant-denied.txt}}
```

Exit `2`: strata searched `dev` and `prod`, and Palette refused the search of
`staging`. A cluster of `staging` that runs the version can be missing from the
answer. Palette still refuses the delete while such a cluster runs it. Run the
retire with a key that reads each project of the tenant, so the line
`NOT CHECKED` goes. See
[A profile of the tenant](../commands/consumers.md#a-profile-of-the-tenant). The checks
`tenant-consumers` and `tenant-consumers-projects` of
[scope.sh](../development/live-runs.md#scopesh) search each project of a live
tenant.

## What to do

- After you delete a version, its record reports `DRIFT`: the version no
  longer exists, and each of its packs is `REMOVED`. `verify` exits `1`, and
  the scheduled job opens an issue for the record. Keep the record for the
  history, and move it out of the pattern that the scheduled check reads, in
  the same change that records the delete.
- `consumers` asks Palette at the time of the command. Run it immediately before
  the delete.
- Give the key a role that reads each project of the tenant. A key that cannot
  read a project leaves it `NOT CHECKED`, and the gate stays closed.
- To run this as a job, see
  [The promotion gate and the retire gate](../automation/gates.md).
