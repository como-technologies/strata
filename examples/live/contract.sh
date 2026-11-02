#!/usr/bin/env bash
# The contract run: ask a real tenant the requests of the facts of Palette, and compare each answer
# with the stand-in. A weekly timer on kadomony runs it (deploy/contract/).
#
#   just live-contract [--state FILE] [--stand-in DIR] [--github URL] [--no-cloud]
#
# For each fact of the kind request that a GET can show, the run finds an object in the tenant. It
# sends the request, and compares the answer with the documents that the stand-in serves for the
# fact. It compares the status and the type of each field that strata reads. It also lists each
# value of a closed set that neither the fact nor the stand-in names: strata does not know it. The
# run sends GET requests only, and builds nothing. A fact that needs a body or a cluster names its
# live script in the result.
#
# For each fact of the kind cloud, the run also sends a read-only call of the tool of a cloud (aws,
# gcloud) in the lab account, and compares the answer with the answers that the fixtures of the rules
# hold. It checks that the answer lists each value of a table that a rule holds. STRATA_CONTRACT_NAME
# gives the value of {name} in a call, such as the project. A tool that is not on the PATH, or a value
# that the lab lacks, skips the call. --no-cloud sends no call of a cloud.
#
# With --state FILE, the run also reads the latest release of the Palette SDK and of the Terraform
# provider from GitHub. It notes a tag that the last run did not see. FILE keeps the tags.
# --stand-in DIR reads the documents of the stand-in from DIR, one directory for each chapter of
# the book. Without it, the run builds the tenant of every chapter, which needs just examples.
#
# The tenant is the one that PALETTE_API, PALETTE_API_KEY, and ~/.palette.env name. The result is
# the document strata-contract/1 in contract.json under runs/<UTC time>/contract/, or OUT.
# Exit 0 when each fact the run checked is the same and no value is unknown, 1 when a fact differs
# or a value is unknown, and 2 when the run could not check. contract-report.sh turns the result
# into issues.
set -uo pipefail

live=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
script=contract
die() { echo "$script: $*" >&2; exit 2; }
usage() { sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; }

xtask=${XTASK_BIN:-$live/../../target/debug/xtask}
args=()
while [ $# -gt 0 ]; do
  case "$1" in
    --state|--stand-in|--github) [ $# -ge 2 ] || die "$1 needs a value"; args+=("$1" "$2"); shift 2 ;;
    --no-cloud) args+=("$1"); shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; die "unknown argument $1" ;;
  esac
done
[ -x "$xtask" ] || die "no xtask binary at $xtask; run just stand-in"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/contract}
mkdir -p "$run" || exit 2

code=0
"$xtask" contract --out "$run/contract.json" "${args[@]+"${args[@]}"}" 2> "$run/contract.log" || code=$?
cat "$run/contract.log" >&2

# what the person reads first: each fact that differs, each unknown value, each fact that the run
# could not check, and each release that is new
if command -v jq > /dev/null && [ -s "$run/contract.json" ]; then
  jq -r '
    (.facts[] | select(.outcome == "differs") | (if .kind == "cloud" then ["the fixtures have", "the cloud has"] else ["the stand-in has", "the tenant has"] end) as $who
      | "differs: \(.id)",
      (.differences[] | if .kind == "name" then "  name \(.path): the answer lacks \(.expected | join(", ")); \($who[1]) \(.found | join(", "))" else "  \(.kind) \(.path): \($who[0]) \(.expected | join(", ")); \($who[1]) \(.found | join(", "))" end)),
    (.facts[] | .id as $id | .unknownValues[]? | "new value: \($id) \(.set): \(.values | join(", ")); the fact and the stand-in name \(.known | join(", "))"),
    (.facts[] | select(.outcome == "error") | "could not check: \(.id): \(.reason)"),
    (.releases[] | select(.new) | "new release: \(.repo) \(.tag), the last run saw \(.seen)"),
    (.releases[] | select(.error != null) | "could not read the release of \(.repo): \(.error)")
  ' "$run/contract.json" >&2
fi
echo "$script: the result is $run/contract.json" >&2
exit "$code"
