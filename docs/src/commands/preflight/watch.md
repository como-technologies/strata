# The watch

`--watch cluster:UID` reads the cluster, its summary, and its events every
`--every` seconds, and judges each watch rule. It ends at the first counted
failure, with the event that shows the fault. It also ends when the cluster is
`Running` with each counted rule passed, or at `--limit`. It ends too when the
cluster is in the state `Error`, or when Palette deletes it: the state is `Deleted`,
or `metadata.isDeleted` is true. A counted failure is a
rule of severity `fail` at the preflight level, or any failed probe with
`--strict`. Any other failed probe, or a warning, is in the report, and the
watch continues.

A deleted cluster gives exit `0` when no counted rule failed. It ends the watch at the first
reading, also when its state is not `Deleted`. The last line of the report says that the
cluster is gone:

```text
{{#include ../../output/preflight-watch-gone.txt}}
```

The same holds when `metadata.isDeleted` is true and the state is still `Running`:

```text
{{#include ../../output/preflight-watch-is-deleted.txt}}
```

A cluster in the state `Error` ends the watch too. This run has no `--limit`:

```text
{{#include ../../output/preflight-watch-error.txt}}
```

A failed rule or an error keeps its own words, also for a deleted cluster. A cluster that is not
gone gives `OK: each counted check passed`.

With `--out`, `world.json` holds the last reading: the conditions and the events
the rules saw. A build that cannot finish stops in minutes, not at the timeout
of the client.

At each reading, `--verbose` prints the three requests of the reading and what
they gave. It also prints each condition that changed, each new event, and one
line for each rule. Before the first reading, it prints the program of each
rule.

On a terminal, the lines have the colors of the report. `--no-color`, or
`NO_COLOR` with a value, removes them, and a stream to a file has none.
`--color always` keeps them through a pipe, for a `tee` to a log and a terminal.
`--color never` is `--no-color`.
[Colors](../../reference/formats.md#colors) gives the order.

To see a watch on a real build, `just live-watch --build --down` in the live
example
([The preflight checks](../../development/live-example.md#the-preflight-checks)).
