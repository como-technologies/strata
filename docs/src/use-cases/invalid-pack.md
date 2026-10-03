# A registry deletes the tag of a pack

## The problem

A published version pins each pack by its tag in a registry. When the registry
deletes a tag that the version uses, Palette marks the pack reference of the
version invalid. The pack reference then has `isInvalid` `true`, and
`inValidReason` gives the message of the registry event, such as
`Cert Manager 1.21.1 tag is deleted`.

The values and the digest of the pack stay the same. So the root does not move,
and a record of the version still verifies as unchanged. The content is the
recorded content, but the registry no longer has the tag.

A record keeps the flag and the reason of each pack, outside the root.
`snapshot` warns about the pack, and `verify` and `compare` name it.

In this example, `dev` has `aws-stack@1.1.9`, the content of 1.1.1. After the
record, the registry deletes the tag `1.21.1` of `cert-manager`.

**Who runs this.** A person who runs `strata verify` on a record finds this.
A job finds it in `verify.invalidPacks` of `strata verify --json`, or in
`records[].invalidPacks` with `--summary`. The scheduled job of
[Drift issues](../automation/drift-issues.md) does not find it: the status of
the record is `unchanged`, so the job opens no issue.

**Follow along.** `just playground invalid-pack-1` serves this tenant on this
machine, with no key and no cost. `just playground invalid-pack-2` serves it
after the registry deleted the tag.

## 1. Record the version

```text
{{#include ../output/invalid-pack-record.txt}}
```

Each pack is valid, so the record has no `invalid` field.

## 2. The registry deletes the tag

**Follow along.** `just playground invalid-pack-2` is the same tenant after the
registry deleted the tag. Stop the first seed, and export what this one prints.
Keep the record from step 1.

```text
{{#include ../output/invalid-pack-verify.txt}}
```

`verify` exits `0`: the content is the recorded content. The line of the pack
says `INVALID on side B`, with the reason. Side B is the version in Palette now.
The line under the verdict names the pack again.

A new record of the version warns on standard error:

```text
{{#include ../output/invalid-pack-snapshot.txt}}
{{#include ../output/invalid-pack-fields.txt}}
```

The two records have the same root. `compare` says `same`, and counts the
invalid pack in `summary.packsInvalid`:

```text
{{#include ../output/invalid-pack-compare-json.txt}}
```

## What to do

- Read the `INVALID` lines of `strata verify`. In a job, read
  `verify.invalidPacks` of `strata verify --json`. A record that verifies can
  name a pack that the registry no longer has.
- When the delete of the tag was a mistake, push the tag to the registry again.
  No live run shows that Palette then clears the flag.
- When the delete was correct, publish a new profile version with a pack
  version that the registry has, and record the new version.

## Limits

- The flag does not change the status of `verify` or the exit code of
  `compare`. The flag is a statement of Palette about the registry, and not a
  change of the content.
- A record written before strata kept the flag has none. `verify` of that
  record reads the flag of Palette now.
- No live run deletes a tag. See
  [What no run covers](../internals/palette/limits.md#what-no-run-covers).
