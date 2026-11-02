# Confidence and promotion

```text
{{#include ../../output/preflight-promotion.txt}}
```

`--promotion` compares the level of each rule with the runs of the ledger:
which probes meet the bar, and which preflight checks no longer do. Exit `1`
when a level disagrees with the ledger. `--ledger FILE` adds the runs of a
company to the runs of the lab. See
[Confidence](../../internals/preflight.md#confidence).

## The runs of one release

`--release RELEASE` makes `--promotion` count only the runs of that release. A
release is its first two numbers, so a run on `4.11.2` is a run on `4.11`. With
no `--release`, each run counts.

A run of an older release counts for nothing. A run with no release is a run of
an older release, because the ledger does not say which release it proved. A run
of a later release counts for nothing, and it is not an older run.

A rule that no run proved on the release is a probe. When its level is
`preflight`, the advice is `demote`, and the exit code is `1`. A rule whose
`releases` range does not include the release has no entry, because no run can
prove it.

The report gives the number of older runs for each rule and engine, as
`older runs` in the text and `olderRuns` in the JSON. The runs of the lab in the
ledger of the binary have no release. So they are older runs for each release
that you name:

```text
{{#include ../../output/preflight-promotion-release.txt}}
```

## After an upgrade of Palette

A company keeps its own probes and its own ledger. Runs on `4.11` promoted the
rule `example-region`, so it is a preflight check on `4.11`. After Palette
upgrades to `4.12`, no run proves the rule on `4.12`. The rule is a probe, and
the level `preflight` is `demote`:

```text
{{#include ../../output/preflight-promotion-upgrade.txt}}
```

The gate stays at exit `1` until a person acts. A person changes the field
`level` of the rule to `probe`, or runs the test plan on `4.12`. Add the run to
the ledger, with `release: "4.12.0"`. The rule is then a preflight check on
`4.12`, and the report still gives the older run of `4.11`:

```text
{{#include ../../output/preflight-promotion-run.txt}}
```

[The rule format](../../reference/preflight-rules.md#the-ledger) states the
ledger.
