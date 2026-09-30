#!/usr/bin/env bash
# Watch a build of the live example on the terminal: `strata preflight cluster:UID --watch --verbose`
# from the moment Palette lists the cluster until it is Running or a stuck state, with one line
# for each rule at each reading. The preflight plan runs the same watch and writes it to a file;
# this is the watch a person sees.
#
#   examples/live/watch.sh [NAME] [--build] [--down] [--out DIR] [--every SECONDS] [--limit SECONDS]
#
# NAME is the cluster to watch, by its name in Palette; the default is barney, cluster B of the
# example. --build makes cluster B beside cluster A with `tofu apply -var second_cluster=true` in
# the background; cluster A stays as it is, and the profiles of the example must be there (just
# live-profiles, or just live-up). Without --build the script waits, up to 20 minutes, for a build
# started elsewhere: a tofu apply, the console, or the API. --down removes cluster B after the
# watch, whatever the watch answered. The report of the watch goes to runs/<UTC time>/watch/out/,
# or to DIR/out/ with --out. --every and --limit go to the watch: a reading every 30 seconds, and
# 90 minutes at most, by default. The lines are verbose; --verbose is accepted and changes nothing.
# The exit is the exit of the watch: 0 when the cluster is Running and each counted rule passed.
set -uo pipefail
script=watch
name=barney
build=""
down=""
every=30
limit=5400
usage() { sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; }
while [ $# -gt 0 ]; do
  case "$1" in
    --build) build=1 ;;
    --down) down=1 ;;
    --verbose | -v) ;;
    --out) [ $# -ge 2 ] || { echo "watch.sh: --out needs a directory" >&2; exit 2; }; OUT=$2; shift ;;
    --every) [ $# -ge 2 ] || { echo "watch.sh: --every needs seconds" >&2; exit 2; }; every=$2; shift ;;
    --limit) [ $# -ge 2 ] || { echo "watch.sh: --limit needs seconds" >&2; exit 2; }; limit=$2; shift ;;
    -h | --help) usage; exit 0 ;;
    -*) echo "watch.sh: unknown option $1" >&2; usage >&2; exit 2 ;;
    *) name=$1 ;;
  esac
  shift
done
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/watch}
mkdir -p "$run" && cd "$run" || exit 2
echo "watch: the report goes to $run"
load_outputs
export PALETTE_PROJECT_UID=$(out project_uid)

# cluster A stays as the state holds it: an apply names it, or OpenTofu would make or remove it
has_a=false
"$live/tf.sh" state list 2> /dev/null | command grep -q 'cluster\["a"\]' && has_a=true

apply_pid=""
if [ -n "$build" ]; then
  echo "tofu apply -var second_cluster=true -var cluster=$has_a (in the background)"
  "$live/tf.sh" apply -input=false -auto-approve -no-color -var second_cluster=true -var "cluster=$has_a" \
    > apply.txt 2>&1 &
  apply_pid=$!
fi

# the watch is about the build, not the apply: it starts when Palette lists the cluster
U=""
for _ in $(seq 1 120); do
  U=$("$strata" list clusters --json 2> /dev/null | jq -r --arg n "$name" '.clusters[] | select(.name == $n) | .uid' | head -n 1)
  [ -n "$U" ] && break
  if [ -n "$apply_pid" ] && ! kill -0 "$apply_pid" 2> /dev/null; then break; fi
  sleep 10
done
if [ -z "$U" ]; then
  [ -z "$apply_pid" ] || wait "$apply_pid"
  die "Palette lists no cluster named $name after 20 minutes${build:+; see $run/apply.txt}"
fi
echo "watch: Palette lists cluster $name as $U; the watch starts"
"$strata" preflight "cluster:$U" --watch --every "$every" --limit "$limit" --verbose --no-color --out out
code=$?
echo "watch: exit $code; the report is in $run/out: report.html, report.md, report.json, world.json"

if [ -n "$apply_pid" ]; then
  wait "$apply_pid"
  command grep -E '^(Plan|Apply complete|Error|Warning)' apply.txt
fi
if [ -n "$down" ]; then
  echo "tofu apply -var second_cluster=false -var cluster=$has_a: cluster B removed"
  if "$live/tf.sh" apply -input=false -auto-approve -no-color -var second_cluster=false -var "cluster=$has_a" \
    > down.txt 2>&1; then
    command grep -E '^Apply complete' down.txt
  else
    command grep -E '^Error' down.txt >&2
    echo "watch: the removal of cluster B failed; see $run/down.txt" >&2
  fi
fi
exit "$code"
