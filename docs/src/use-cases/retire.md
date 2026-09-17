# Retire a version

## The problem

A team wants to delete old profile versions. A version that a cluster uses must
stay until the cluster moves to a newer version.

In this example, `dev` has `aws-pxk@1.0.0`, which no cluster uses, and
`aws-pxk@1.1.0`, which `payments` and `search` run.

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

Put the delete after `&&`. It runs only on exit `0`. A cluster that strata
cannot read still counts, so the delete does not run.

## 4. The clusters in a script

```text
{{#include ../output/retire-json.txt}}
```

The third column is the version that the copy of each cluster names.

## What to do

- After you delete a version, `verify` reports `ERROR` for its record. Keep the
  record for the history, but move it out of the pattern of the scheduled check.
- `consumers` asks Palette at the time of the command. Run it immediately before
  the delete.
