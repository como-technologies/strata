# A secret changes in a pack value

## The problem

The schema of a pack can give a value the format `password`. Palette then
serves `********` in place of that value: see
[Secrets in the values](../internals/palette/profiles.md#secrets-in-the-values).
A record holds the mask, and not the secret. So the root covers the mask, and
a change of the secret does not move the root.

In this example, `dev` and `prod` each have `aws-stack@1.1.10`. Its
`cert-manager` pack has the value `registryPassword`, and the schema of the pack
gives it the format `password`. The secret of `prod` is not the secret of
`dev`. Palette serves the same mask in the two projects.

**Who runs this.** A person who promotes a release, and the scheduled verify
job. The job is [Drift issues](../automation/drift-issues.md).

**Follow along.** `just playground masked-value` serves this tenant on this
machine, with no key and no cost.

## 1. The record names each masked value

```text
{{#include ../output/masked-record.txt}}
{{#include ../output/masked-paths.txt}}
```

`packs[].masked` gives the path of each value that holds `********`. The root
does not cover the list: the values are in the root already, with the mask.

## 2. same-release passes the pair, and names the pack

```text
{{#include ../output/masked-same-release.txt}}
```

The two records have one root, so the pair is `IDENTICAL`, and `same-release`
exits `0`. The line under the pair tells you that the root does not show the
secret. The mask is what Palette serves, so the line does not stop the pair.
The JSON report gives the same fact in `pairs[].masked`:

```text
{{#include ../output/masked-same-release-json.txt}}
```

## 3. compare and verify name the pack

```text
{{#include ../output/masked-compare.txt}}
```

`compare` gives one `note` for each pack with a masked value. `verify` gives
the same note, and a line under the pack:

```text
{{#include ../output/masked-verify.txt}}
```

The verdict is `OK`: Palette serves the content of the record. A change of the
secret in Palette does not change this verdict.

## What to do

- Do not use a record to check a secret. Check the secret where you keep it,
  for example in a secret store with its own version history.
- Keep the secret out of the pack values. Give the pack the name of a
  Kubernetes Secret, and give each new secret a new name. Then a change of the
  secret is a change of the name, and the root shows it.
- Read `packs[].masked` before you keep a record in a repository. A masked value
  holds no secret, but each other value of the pack is in clear.

## Palette behavior

Palette masks a value of the format `password` when it serves the published
packs of a profile. It does not mask an empty value, or a value that holds a
variable. Strata masks the packs of a draft in the same way. See
[Secrets in the values](../internals/palette/profiles.md#secrets-in-the-values).
The rule for each hash is at
[What the root does not cover](../internals/hashes.md#what-the-root-does-not-cover).
