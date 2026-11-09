#!/usr/bin/env bash
# Write the records of the observer as Kubernetes Secrets, on standard output. It calls no Kubernetes
# API, so the pipeline gives the output to kubectl. To make the Secrets, use
# `kubectl apply --server-side -f -`. To change the Secrets, use `kubectl replace -f -`: a replace
# removes each key that the output does not hold, also a key that the seed of the chart wrote.
# A client-side apply fails, because it copies each Secret to an annotation with a limit of 256 KiB.
#
#   scripts/ci/observer-records.sh --revision REV [--namespace NAME] <record.json>...
#   scripts/ci/observer-records.sh --first-seen <cluster uid> [--namespace NAME]
#
# --revision names the Git revision of records that a review approved. --first-seen records what each
# profile of the cluster holds now. No person approved that content, and each screen of the observer
# says so. Each record goes gzipped in one of four Secrets. An empty Secret removes the old content.
# Needs strata (or STRATA_BIN) for --first-seen, gzip, base64, and jq.
set -euo pipefail
strata=${STRATA_BIN:-strata}
namespace=strata-observer
revision=
first_seen=
limit=${OBSERVER_SECRET_LIMIT:-900000}
while [ $# -gt 0 ]; do
  case $1 in
    --revision) revision=${2:?--revision needs a value}; shift 2 ;;
    --first-seen) first_seen=${2:?--first-seen needs a cluster uid}; shift 2 ;;
    --namespace) namespace=${2:?--namespace needs a value}; shift 2 ;;
    -h|--help) sed -n '2,/^set -euo/{/^#/p}' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --) shift; break ;;
    -*) echo "observer-records: unknown option $1" >&2; exit 2 ;;
    *) break ;;
  esac
done
if { [ -n "$revision" ] && [ -n "$first_seen" ]; } || { [ -z "$revision" ] && [ -z "$first_seen" ]; }; then
  echo "observer-records: give --revision or --first-seen, and not both" >&2
  exit 2
fi
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
files=("$@")
if [ -n "$first_seen" ]; then
  [ $# -eq 0 ] || { echo "observer-records: --first-seen takes no record file" >&2; exit 2; }
  "$strata" snapshot "cluster:$first_seen" -o "$work/cluster.json" -q
  files=()
  while IFS= read -r ref; do
    file=$work/$(printf %s "$ref" | tr -c 'A-Za-z0-9._-' _).json
    "$strata" snapshot "$ref" -o "$file" -q
    files+=("$file")
  done < <(jq -r '.source.cluster.profiles[] | "\(.name)@\(.version)"' "$work/cluster.json")
  rm -f "$work/cluster.json"
fi
[ "${#files[@]}" -gt 0 ] || { echo "observer-records: no record" >&2; exit 2; }

baseline=approved
[ -z "$first_seen" ] || baseline=first-seen
meta=$(jq -cn --arg revision "$revision" --arg baseline "$baseline" --arg cluster "$first_seen" \
  '{baseline: $baseline} + (if $revision != "" then {revision: $revision} else {} end) + (if $cluster != "" then {firstSeenFrom: $cluster} else {} end)')

declare -a shard_size=(0 0 0 0) shard=("" "" "" "")
keys=
shard[0]="  meta.json: $(printf %s "$meta" | base64 | tr -d '\n')"$'\n'
for file in "${files[@]}"; do
  jq -e '.schema == "strata-snapshot/1" and (.source.cluster == null)' "$file" >/dev/null \
    || { echo "observer-records: $file is not a profile record" >&2; exit 2; }
  key=$(jq -r '"\(.source.name)_\(.source.version).json.gz"' "$file" | tr -c 'A-Za-z0-9._\n-' _)
  # A key is a file of the mounted volume, so two records of one profile version collide.
  case " $keys " in *" $key "*) echo "observer-records: two records are for $key" >&2; exit 2 ;; esac
  keys+=" $key"
  encoded=$(gzip -9nc "$file" | base64 | tr -d '\n')
  placed=
  for i in 0 1 2 3; do
    if [ $((shard_size[i] + ${#encoded})) -le "$limit" ]; then
      shard[i]+="  $key: $encoded"$'\n'
      shard_size[i]=$((shard_size[i] + ${#encoded}))
      placed=1
      break
    fi
  done
  [ -n "$placed" ] || { echo "observer-records: the records do not fit in four Secrets: $key" >&2; exit 2; }
done
for i in 0 1 2 3; do
  cat <<YAML
---
apiVersion: v1
kind: Secret
metadata:
  name: strata-observer-records-$i
  namespace: $namespace
  labels:
    app.kubernetes.io/name: strata-observer
type: Opaque
YAML
  if [ -n "${shard[i]}" ]; then
    printf 'data:\n%s' "${shard[i]}"
  else
    printf 'data: {}\n'
  fi
done
