# The static tier and the live tier

The checks of the body of a create, or of a cluster as Palette holds it now:
first without a key, then with the answers of Palette.
[strata preflight](../preflight.md) gives the exit codes and the options.

## The static tier: no key, no network

```text
{{#include ../../output/preflight-static.txt}}
```

`--verbose` prints, after each check, what it ran, the answer, the time, and
the verdict: the request to Palette with its HTTP status, the command with its
exit code, or `jq over the world` for a static check. It is the live form of
the reproduce lines. It wins over `STRATA_QUIET`; `--quiet` wins over both.

```text
{{#include ../../output/preflight-verbose.txt}}
```

Each rule is one line: the result, the id, what the rule checks, and the time.
A failed rule adds its messages, a `reproduce:` line that shows the same thing
with `jq`, and the fix for good with its owner. `NOT CHECKED` is a rule that
strata cannot run here, with the commands to run by hand.

The result names the level of the rule. `PROBE PASS` and `PROBE FAIL` are
probes: strata knows the fault, and the runs do not yet prove the check. `PASS`
and `FAIL` are preflight checks: the runs prove them. A failed probe does not
change the exit code; a failed preflight check makes it `1`. See
[Two levels: probe and preflight check](../../internals/preflight.md#two-levels-probe-and-preflight-check).

## The live tier: the answers of Palette

With a key, strata records each profile of the entity, reads the cloud account
and the pack of each layer, and calls the validate routes of Palette:

```text
{{#include ../../output/preflight-live.txt}}
```

`POST /v1/spectroclusters/aws/validate` answered that the workers have too few
cores for the packs. The console calls this route in its create wizard. A
create through the provider or the API goes to the create route, so the same
fault shows after a full apply. See
[The validate routes](../../internals/palette/validate-routes.md).
