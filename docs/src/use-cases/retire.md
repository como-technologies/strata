# Retire a version

## The problem

A team wants to delete old profile versions. A version that a cluster uses must
stay until the cluster moves to a newer version.

In this example, `dev` has `aws-stack@1.0.0`, which no cluster uses, and
`aws-stack@1.1.0`, which `payments` and `search` run.

**Who runs this.** A person at a terminal, or the retire gate, which runs
`consumers` for a job: see [The promotion gate and the retire gate](../automation/gates.md).
Every step reads Palette, so each one needs a key.

**Follow along.** `just playground retire` serves this tenant on this machine,
with no key and no cost. Nothing on this page changes the tenant, so one seed
serves every step.

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

`no consumers` and exit `0`: Palette will accept the delete.

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

The third column is the version that the copy of each cluster names, which is
not always the version the uid holds now: see
[A version bump overwrites a version](./overwritten.md).

A pipe hides the exit code of step 3. `strata consumers` exits `1` here, and the
shell reports the exit of `jq`, which is `0`. A script that reads the rows and
gates on the exit must set `pipefail`, or run the two commands apart.

## What to do

- After you delete a version, its record no longer verifies. Palette answers
  404, and strata reports the error rather than a verdict: `verify` of that one
  record exits `2`, and `verify` over many records gives the record the label
  `ERROR`, counts it in `errors`, and exits `2` as well. An error outranks a
  drift, so one deleted version turns the whole run red. Keep the record for the
  history, and move it out of the pattern that the scheduled check reads.
- `consumers` asks Palette at the time of the command. Run it immediately before
  the delete.
- To run this as a job, see
  [The promotion gate and the retire gate](../automation/gates.md).
