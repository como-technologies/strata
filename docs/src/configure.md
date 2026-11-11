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

On HTTP 401 or HTTP 403, strata names the key in the error. Check
`PALETTE_API_KEY`, `SPECTROCLOUD_APIKEY`, and `~/.palette.env`. A 403 with the code
`ProjectUidIsNotEmpty` is the exception. The key is right: the route is of the tenant scope, and the
request named a project. The error says so.

Put the values in `~/.palette.env` to keep them out of the shell history.
Strata reads a variable from that file when the environment does not set it:

```bash
PALETTE_API_KEY=...
PALETTE_PROJECT=dev
```

## The project

A profile version and a cluster belong to a project. Strata uses the project in
this order:

1. `--project <uid>` or `--project-name <name>`, on each command that reads
   Palette. `compare` and `same-release` also take a project for each side:
   see [Environments in the projects of one tenant](./use-cases/projects.md).
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
error as one JSON object on standard error. A caller that reads that stream
passes `-q` to keep it clean. A warning there breaks the caller.

A name in `~/.palette.env` under a uid in the environment is the fallback at
work, so that pair is silent. The pair that gets the warning is the other way
round: a name you type for one command, under a uid that file has held for
months.

A snapshot file records the project uid. `strata verify` checks each record in
its own project, so one scheduled job checks the records of all projects. A
record from tenant scope has no project uid, so `verify` checks it in the
configured project. `behind` and `consumers` read a record in its project in
the same way. `verify --project` is the one exception: it checks each record in
the project of the flag.

`strata preflight` runs `strata-preflight`, which takes no project flag. It
reads the project from `PALETTE_PROJECT_UID` and `PALETTE_PROJECT`.

## A self-hosted Palette

```bash
export PALETTE_API=https://palette.example.com
```

The default is `https://api.spectrocloud.com`.

## Retries

Strata sends a request again after HTTP 5xx, HTTP 429, or a transport error.
A 5xx that is a
[refusal of Palette](./reference/errors.md#refusals-that-strata-does-not-send-again)
gets no second request. A `Retry-After` that is longer than the limit stops the
command at once. A scheduled job then stops early, and can run again at a later
time.

[Settings](./reference/settings.md) gives `STRATA_RETRIES`,
`STRATA_MAX_RETRY_AFTER`, the wait between attempts, and each other variable
with its default.

Next: [Quick start](./quickstart.md).
