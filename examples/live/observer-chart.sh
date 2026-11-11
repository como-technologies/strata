#!/usr/bin/env bash
# The chart of the observer in a registry of Palette with no login (#732). Step 1 of "Install with a
# profile in Palette" in docs/src/automation/observer.md adds the registry of the chart with a token.
# The chart is on a public package, so this script asks whether Palette syncs it with the auth type
# noAuth.
#
#   examples/live/observer-chart.sh
#
# The script makes an OCI registry of the type basic, with the provider helm and the auth type
# noAuth. It starts a sync of the registry, and it reads the packs of the registry for the chart
# strata-observer. Then it deletes the registry, also after a failed check or an error. A check
# with ? records what Palette does, and it judges nothing. 0 says that the step works with no
# login, and 1 says that it does not. The output of each check holds the answer of Palette.
#
# The registry is of the tenant, so the key needs a role that adds a registry at tenant scope. The
# script needs no cluster, and it changes nothing else in the tenant. The name of the registry holds
# a random suffix. Needs curl, jq, and the key of verify.sh. ENDPOINT is the address of the registry
# (default: https://ghcr.io/orgs/OWNER, with the owner that the Cargo.toml of the root names).
# BASE_PATH is the folder of the charts on that address (default: charts). Results go under
# runs/<UTC time>/observer-chart/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=observer-chart
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
. "$live/github.sh"
case "${1:-}" in "") ;; *) usage; exit 2 ;; esac
for tool in curl jq; do
  command -v "$tool" > /dev/null || die "$tool is needed"
done
palette_settings

repo=$(workspace_repo)
[ -n "$repo" ] || die "Cargo.toml of the root names no repository"
endpoint=${ENDPOINT:-https://ghcr.io/orgs/${repo%%/*}}
base_path=${BASE_PATH:-charts}
chart=strata-observer
name=strata-live-observer-chart-$(od -An -N3 -tx1 /dev/urandom | tr -d ' \n')

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/observer-chart}
mkdir -p "$run" && cd "$run" || exit 2

# The uid of the registry. registry.uid keeps it, so that the end deletes it after an error too.
registry=

# registry_create: the registry with no login. Palette reads the address at the create, so a refusal
# of the read fails the create, and the error shows the code of Palette.
registry_create() {
  registry=$("$live/api.sh" oci-registry-create "$name" "$endpoint" "$base_path") && [ -n "$registry" ] || return 1
  echo "$registry" > registry.uid
  echo "$name: $registry, for $endpoint and $base_path, with the auth type noAuth"
}
# registry_sync: a sync of the registry ends with the status Completed. The output is the status of
# the sync, with its message.
registry_sync() {
  "$live/api.sh" sync oci-basic "$registry" > sync.json || return 1
  jq '{status, message, lastSyncedTime}' sync.json
  jq -e '.status == "Completed"' sync.json > /dev/null
}
# chart_listed: the packs of the registry hold the chart. A status of Completed alone does not show
# that Palette read the chart, so only this check shows it. The output is the first pack.
chart_listed() {
  "$live/api.sh" packs-where --name "$chart" --arg r "$registry" '.spec.registryUid == $r' > chart.json || return 1
  jq '{name: .spec.name, version: .spec.version, registryUid: .spec.registryUid}' chart.json
}
# registry_gone: the registry of the run is deleted, and a read of it answers HTTP 404. A run whose
# create failed has no registry.
registry_gone() {
  local uid path
  [ -f registry.uid ] || { echo "the run made no registry"; return 0; }
  uid=$(cat registry.uid)
  "$live/api.sh" registry-delete oci-basic "$uid" || return 1
  path=$("$live/api.sh" registry-path oci-basic "$uid") || return 1
  [ "$("$live/api.sh" get-status "$path" | head -n 1)" = "HTTP 404" ] && rm -f registry.uid
}

summary() {
  write_summary "the chart of the observer with no login" ", registry \`$name\` for \`$endpoint\` and \`$base_path\`."
}
# the registry goes at the end, also when a check stops the script
finish() {
  [ ! -f registry.uid ] || registry_gone >> steps.log 2>&1
  summary
}
trap finish EXIT

echo "== the chart of the observer with no login (#732)" | tee -a steps.log
check registry-no-login ? '#732: Palette makes the OCI registry with the provider helm and the auth type noAuth: 1 says that it refuses it, with the code' -- \
  registry_create
if [ -n "$registry" ]; then
  check sync-no-login ? '#732: a sync of the registry ends with the status Completed: 1 says that it does not, with the message' -- registry_sync
  check chart-no-login ? "#732: the packs of the registry hold the chart $chart: 1 says that Palette read no chart" -- chart_listed
fi
check registry-gone 0 '#732: the registry of the run is gone from the tenant' -- registry_gone

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
