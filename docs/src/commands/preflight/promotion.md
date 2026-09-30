# Confidence and promotion

```text
{{#include ../../output/preflight-promotion.txt}}
```

`--promotion` compares the level of each rule with the runs of the ledger:
which probes meet the bar, and which preflight checks no longer do. Exit `1`
when a level disagrees with the ledger. `--ledger FILE` adds the runs of a
company to the runs of the lab. See
[Confidence](../../internals/preflight.md#confidence).
