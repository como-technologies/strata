# Retire a version

## The problem

A team wants to delete old profile versions. A version that a cluster uses must
stay until the cluster moves to a newer version.

In this example, `dev` has `aws-stack@1.0.0`, which no cluster uses, and
`aws-stack@1.1.0`, which `payments` and `search` run.

**Who runs this.** A person at a terminal, or the retire gate, which runs
`consumers` for a job: see [The promotion gate and the retire gate](../automation/gates.md).

**Follow along.** `just playground retire` serves this tenant on this machine,
with no key and no cost.

## 1. A version in use

```text
{{#include ../output/retire-in-use.txt}}
```

Exit `1`. Move each cluster first.

## 2. A version that no cluster uses

```text
{{#include ../output/retire-free.txt}}
```

Exit `0`.

## 3. Gate the delete

```text
{{#include ../output/retire-gate.txt}}
```

Strata deletes nothing. The delete is your own command, from the Spectro Cloud
provider, the Palette API, or the console. Put it after `&&`, so it runs only on
exit `0`. A cluster that strata cannot read still counts, so the delete does not
run.

## 4. The clusters in a script

```text
{{#include ../output/retire-json.txt}}
```

The third column is the version that the copy of each cluster names.

## What to do

- After you delete a version, its record no longer verifies: `verify` of that
  one record exits `2`, and `verify --summary` over many records counts it as
  `ERROR`. Keep the record for the history, and move it out of the pattern that
  the scheduled check reads.
- `consumers` asks Palette at the time of the command. Run it immediately before
  the delete.
- To run this as a job, see
  [The promotion gate and the retire gate](../automation/gates.md).
