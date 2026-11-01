# The watch

`--watch cluster:UID` reads the cluster, its summary, and its events every
`--every` seconds, and judges each watch rule. It ends at the first counted
failure, with the event that shows the fault. It also ends when the cluster is
`Running` with each counted rule passed, or at `--limit`. A counted failure is a
rule of severity `fail` at the preflight level, or any failed probe with
`--strict`. Any other failed probe, or a warning, is in the report, and the
watch continues.

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
