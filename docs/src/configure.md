# Configure

Strata needs an API key. A project sets the scope of a command. With no project,
strata reads at tenant scope.

```bash
export PALETTE_API_KEY=...        # the API key
export PALETTE_PROJECT=dev        # the project name
strata list projects              # test the key: the projects that it can see
```

Put the values in `~/.palette.env` to keep them out of the shell history.
Strata reads a variable from that file when the environment does not set it:

```bash
PALETTE_API_KEY=...
PALETTE_PROJECT=dev
```

## The project

A profile version and a cluster belong to a project. Strata uses the project in
this order:

1. `--project <uid>` or `--project-name <name>`, on `list` and `verify`.
2. `PALETTE_PROJECT_UID`.
3. `PALETTE_PROJECT`. Strata finds its uid at tenant scope, one time for each
   command. An unknown name is an error that lists the projects that the key can
   see.
4. No project: tenant scope.

A snapshot file records the project uid. `strata verify` checks each record in
its own project, so one scheduled job checks the records of all projects. A
record from tenant scope has no project uid, so `verify` checks it in the
configured project.

## A self-hosted Palette

```bash
export PALETTE_API=https://palette.example.com
```

The default is `https://api.spectrocloud.com`.

## Retries

Strata sends a request again after HTTP 5xx, HTTP 429, or a transport error, and
after no other error. The wait starts at one second, doubles, and stops at 30
seconds. `STRATA_RETRIES` sets the number of retries, and `0` stops them.

Strata obeys a `Retry-After` of up to `STRATA_MAX_RETRY_AFTER` seconds, 300 by
default. A longer one stops the command at once with exit `2`. The error is
`retry-after-too-long` while a retry remains, and `api` on the last attempt, so
also with `STRATA_RETRIES=0`. A scheduled job then stops early, and can run again at a
later time.

[Settings](./reference/settings.md) lists each variable and its default.

Next: [Quick start](./quickstart.md).
