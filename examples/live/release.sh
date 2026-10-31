#!/usr/bin/env bash
# The release test (#146): which call gets the release of Palette from GET /v1/mgmt/app/version.
# A call with no key must answer 200 with the release. A call with the ApiKey header must be
# refused, so no tenant key can read the release, and `strata preflight` takes it from --release.
#
#   examples/live/release.sh
#
# It calls the route twice, with `api.sh app-version`: once with no key, and once with the key.
# It reads the answer of each, and writes nothing to the tenant.
#
# Needs the key as verify.sh needs it, and nothing else: no profile, no cluster, no cost. Results
# go under runs/<UTC time>/release/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=release
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "${1:-}" in "") ;; *) usage; exit 2 ;; esac

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/release}
mkdir -p "$run" && cd "$run" || exit 2

summary() {
  write_summary "release test" ": the route \`GET /v1/mgmt/app/version\`."
}
trap summary EXIT

echo "== the route that gives the release (#146)" | tee -a steps.log
check release-no-key 0 '#146: a call with no key answers 200' -- "$live/api.sh" app-version --no-key
check release-no-key-says 0 '#146: the answer holds the installed release' -- \
  jq -e '.version.installedVersion | strings | length > 0' release-no-key.out
check release-with-key 2 '#146: a call with the ApiKey header is refused' -- "$live/api.sh" app-version
check release-with-key-says 0 '#146: the refusal is HTTP 500' -- \
  grep -q 'answered HTTP 500' release-with-key.err

echo "release: $failures check(s) failed; results in $run"
[ "$failures" = 0 ]
