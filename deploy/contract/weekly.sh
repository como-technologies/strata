#!/usr/bin/env bash
# The weekly contract run on kadomony. It updates the checkout, builds what the run needs, and runs
# the contract run against the real tenant. Then it writes the result as issues, and keeps it.
#
#   deploy/contract/weekly.sh
#
# The timer strata-contract.timer starts it (deploy/contract/timer.sh installs it). The tenant is
# the one that ~/.palette.env names. The job keeps its files in ~/.local/share/strata-contract/, or
# in STRATA_CONTRACT_DIR. The result of each run is in runs/<UTC time>/. The newest result is in
# last.json. state.json holds the tags of the releases that the last run saw. The run writes the
# new tags to runs/<UTC time>/state.json. The script moves that copy onto state.json when the
# report handled the issues, also when the run could not check a fact. A report that fails leaves
# the old tags. The script keeps the 12 newest runs.
#
# Exit 0 when the run and the issues are done, also when a fact differs or a value is unknown: the
# issue is the report.
# Exit 2 when the run could not check a fact or a release, or an issue could not be written.
# The service shows that as a failure.
set -uo pipefail

script=weekly
note() { echo "$script: $*" >&2; }
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$here/../.." && pwd)
# shellcheck source=deploy/contract/folder.sh
. "$here/folder.sh"
keep=12
run=$data/runs/$(date -u +%Y%m%dT%H%M%SZ)
mkdir -p "$run" || exit 2
cd "$repo" || exit 2

# the facts of main are the facts to prove; a failed update runs the checkout as it is
git pull --ff-only > "$run/git.log" 2>&1 || note "git pull failed; the run uses the checkout as it is (see $run/git.log)"

# the run builds the tenant of every chapter of the book, which needs the examples
just examples stand-in > "$run/build.log" 2>&1 || { note "just examples stand-in failed; see $run/build.log"; exit 2; }

# The run writes the new tags of the releases to a copy of state.json. The job keeps the copy when
# the report is done: a report that fails leaves the old tags, so the next run notes the new tag
# again, and the report opens the issue then. A person can close the issue after the report is
# done, so the job must keep the tags then too.
state=$run/state.json
if [ -f "$data/state.json" ]; then
  cp "$data/state.json" "$state" || { note "could not copy $data/state.json"; exit 2; }
fi

code=0
OUT=$run examples/live/contract.sh --state "$state" || code=$?

report=0
# the count of same in the last run: a fall of it says that the cover fell
before=$(jq -r '.summary.same // empty' "$data/last.json" 2> /dev/null || true)
if [ -s "$run/contract.json" ]; then
  examples/live/contract-report.sh "$run/contract.json" || report=$?
  cp "$run/contract.json" "$data/last.json"
  # the report exits 0, 1, or 3 when it handled the issues (3 when a fact could not be checked),
  # and 2 when it did not
  case "$report" in
    0|1|3)
      if [ -f "$state" ]; then
        mv "$state" "$data/state.json" || { note "could not keep the tags of the releases in $data/state.json"; report=2; }
      fi ;;
  esac
else
  note "the run wrote no result"
  exit 2
fi

# keep the newest runs: the names are UTC times, so the name order is the time order
ls -1d "$data"/runs/*/ 2> /dev/null | sort | head -n -"$keep" | while read -r old; do rm -rf "$old"; done

summary=$(jq -r -f examples/live/contract-summary.jq "$run/contract.json" 2> /dev/null || echo "no summary in the result")
note "the contract run exited $code and the report exited $report: $summary"
same=$(jq -r '.summary.same // empty' "$run/contract.json" 2> /dev/null || true)
if [ -n "$before" ] && [ -n "$same" ] && [ "$same" -lt "$before" ]; then
  note "the cover fell: $same facts are the same, and the last run had $before"
fi
if [ "$code" -eq 2 ] || [ "$report" -eq 2 ]; then exit 2; fi
exit 0
