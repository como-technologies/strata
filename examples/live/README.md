# The live example

A real cluster and profile versions in a Palette tenant, and each strata check
that needs them. The cluster costs money while it exists.

```bash
just tofu-install      # once
just live-up           # billable until live-down
just live-verify
just live-down
```

- [The live example](../../docs/src/development/live-example.md): prerequisites,
  steps, checks, failures, and teardown.
- [Live runs](../../docs/src/development/live-runs.md): the recorded results.
- [What Palette does](../../docs/src/internals/palette.md): what the runs show.
