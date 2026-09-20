# Configure

Strata needs an API key. A project sets the scope of a command. With no project,
strata reads at tenant scope.

```bash
export PALETTE_API_KEY=...        # the API key
strata list projects              # test the key: the projects that it can see
export PALETTE_PROJECT=<name>     # one name from that list
```

Set the project after the listing, not before it. Strata resolves the project
name before it runs any command, so a name your key cannot see stops
`strata list projects` too.

A key that is wrong or expired answers HTTP 401 or 403. Check
`PALETTE_API_KEY`, `SPECTROCLOUD_APIKEY`, and `~/.palette.env`.

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

Set a uid and a name together and the uid decides, so the name changes nothing.
When the two name two projects, strata answers for the uid and writes a warning
to standard error first:

```text
strata: warning: PALETTE_PROJECT_UID is 64f2…a0d1 and PALETTE_PROJECT is 'prod',
which is 64f2…a0f1. A uid has priority, so this answers for 64f2…a0d1. Clear one
of the two: `PALETTE_PROJECT_UID= PALETTE_PROJECT=prod` asks for the project you
named.
```

The warning costs one project listing, and only when both are set. It says
nothing when the two agree, which is the common setting: the playground prints
both lines. A name that no project matches is a warning too, and not an error,
because a stale variable that decides nothing must not fail a command.

`-q` and `STRATA_QUIET` hide the warning, and skip the listing that finds it.
The warning is for the person at the terminal. With `--json` strata writes the
error as one JSON object on standard error, and a caller that reads that stream
passes `-q` to keep it clean, so a warning there would break the caller instead
of helping it.

A name in `~/.palette.env` under a uid in the environment is the fallback doing
its job, so that pair is silent. The pair that warns loudest is the other way
round: a name you type for one command, under a uid that file has held for
months.

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
