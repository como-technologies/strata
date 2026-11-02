# The summary line of a document strata-contract/1: the count of the facts, and of each outcome. It
# reads `summary` and counts nothing again. contract-report.sh, weekly.sh, and timer.sh print it,
# and `xtask contract` prints the same text: a test of xtask makes sure of it.
.summary
| "\(.same + .differs + .skipped + .error) facts: \(.same) same, \(.differs) differ, \(.skipped) skipped, \(.error) could not be checked, \(.unknownValues // 0) unknown values"
