# The watch

`--watch cluster:UID` reads the cluster, its summary, and its events every
`--every` seconds, and judges each watch rule. It ends at the first counted
failure (a rule of severity `fail` at the preflight level, or any failed probe
with `--strict`), with the event that shows the fault, or when the cluster is
`Running` with each counted rule passed, or at `--limit`. A failed probe or a
warning is in the report, and the watch reads on. With `--out`, `world.json`
holds the last reading: the conditions and the events the rules saw. A build
that cannot finish stops in minutes, not at the timeout of the client.
`--verbose` prints, at each reading, the three requests the reading made and
what they gave, each condition that changed, each new event, and one line for
each rule; and before the first reading, the program of each rule. On a
terminal the lines carry the colors of the report; `--no-color` or `NO_COLOR`
turns them off, and a stream to a file has none. `--color always` keeps them
through a pipe, for a `tee` to a log and a terminal; `--color never` is
`--no-color`. To see a
watch on a real build, `just live-watch --build --down` in the live example
([The preflight checks](../../development/live-example.md#the-preflight-checks)).
