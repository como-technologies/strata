# Examples

Real pack data from the Public Repo registry, and the script that builds
profiles, a cluster, and reports from it.

```bash
just examples          # build offline, no API key
just examples-fetch    # fetch the pack data again, needs PALETTE_API_KEY
```

- [The offline examples](../docs/src/development/examples.md): what is real,
  what is built, and the profiles.
- [The live example](../docs/src/development/live-example.md): `live/`.
- [The Rust library](../docs/src/automation/library.md): `library/`, and
  `library-preflight/` for `strata-preflight`.
