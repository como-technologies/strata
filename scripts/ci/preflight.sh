#!/usr/bin/env bash
# Run `strata preflight` in a pipeline, with the options in variables, and post the report.
#
#   scripts/ci/preflight.sh [--offline]
#
# The reusable workflow preflight.yml and the job strata-preflight of the GitLab drop-in call this
# script, so one place builds the list of arguments. The script reads one variable for each option
# of the command. It runs `strata preflight` and prints the markdown report. It exits with the
# status of the command: 0, 1, or 2. An empty variable gives no option, and STRATA_PREFLIGHT_SOURCE
# is required.
#
#   STRATA_PREFLIGHT_SOURCE   the body of the create as a file, or cluster:UID
#   STRATA_PREFLIGHT_CHANGE   a change of the cluster, as a file
#   STRATA_PREFLIGHT_CLOUD    --cloud
#   STRATA_PREFLIGHT_TIER     --tier
#   STRATA_PREFLIGHT_PROBES   --probes
#   STRATA_PREFLIGHT_RELEASE  --release
#   STRATA_PREFLIGHT_READY    --ready
#   STRATA_PREFLIGHT_EVERY    --every
#   STRATA_PREFLIGHT_LIMIT    --limit
#   STRATA_PREFLIGHT_RULE     rule ids separated by spaces: one --rule for each
#   STRATA_PREFLIGHT_RECORDS  record patterns separated by spaces: one --records for each
#   STRATA_PREFLIGHT_STRICT   true gives --strict
#   STRATA_PREFLIGHT_NO_EXEC  true gives --no-exec
#   STRATA_PREFLIGHT_WATCH    true gives --watch
#   STRATA_PREFLIGHT_VERBOSE  true gives --verbose; any other value gives --quiet
#   STRATA_PREFLIGHT_OUT      the directory for --out; preflight by default
#   STRATA_PREFLIGHT_COMMENT  true posts OUT/report.md with pr-comment.sh --preflight
#
# --offline reads nothing from Palette: the static tier alone. The script gives --offline too when
# palette-key.sh finds no key where strata looks for one: PALETTE_API_KEY, SPECTROCLOUD_APIKEY, or
# ~/.palette.env. In both cases, a line on standard error says that the run is offline, and why.
# The report says offline after the tiers. The script does not expand a record pattern. Strata
# expands it, because bash reads ** as * and drops the files that do not match.
#
# The script posts the comment also after a failed preflight, because a person must read a failed
# report. It needs pr-comment.sh beside it. A comment that fails turns exit 0 into exit 2. The
# status of the command has priority when it is not 0.
#
# Needs strata (or STRATA_BIN), with strata-preflight beside it, and palette-key.sh beside this
# script.
set -euo pipefail

die() { echo "preflight: $*" >&2; exit 2; }
note() { echo "preflight: $*" >&2; }

offline=""
while [ $# -gt 0 ]; do
  case "$1" in
    --offline) offline=--offline; shift ;;
    -h|--help) sed -n '2,/^set -euo/{/^#/p}' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option $1" ;;
  esac
done

ref=${STRATA_PREFLIGHT_SOURCE:-}
[ -n "$ref" ] || die "set STRATA_PREFLIGHT_SOURCE to the body of the create, or to cluster:UID"
strata=${STRATA_BIN:-strata}
command -v "$strata" > /dev/null || die "no strata at $strata; set STRATA_BIN"
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# the key where strata looks for it; with none, palette-key.sh prints where it looked
if [ -z "$offline" ]; then
  offline=$("$here/palette-key.sh") || [ $? -eq 1 ] \
    || die "palette-key.sh beside this script did not answer"
fi
[ -z "$offline" ] || note "the run is offline, so it reads nothing from Palette: $offline"

args=("$ref")
[ -z "${STRATA_PREFLIGHT_CHANGE:-}" ] || args+=("$STRATA_PREFLIGHT_CHANGE")
# valued VALUE OPTION: the option with its value, when the value is not empty
valued() { [ -z "$1" ] || args+=("$2" "$1"); }
# switch VALUE OPTION: the option, when the value is true
switch() { [ "$1" != true ] || args+=("$2"); }
valued "${STRATA_PREFLIGHT_CLOUD:-}" --cloud
valued "${STRATA_PREFLIGHT_TIER:-}" --tier
valued "${STRATA_PREFLIGHT_PROBES:-}" --probes
valued "${STRATA_PREFLIGHT_RELEASE:-}" --release
valued "${STRATA_PREFLIGHT_READY:-}" --ready
valued "${STRATA_PREFLIGHT_EVERY:-}" --every
valued "${STRATA_PREFLIGHT_LIMIT:-}" --limit
switch "${STRATA_PREFLIGHT_STRICT:-}" --strict
switch "${STRATA_PREFLIGHT_NO_EXEC:-}" --no-exec
switch "${STRATA_PREFLIGHT_WATCH:-}" --watch
# --quiet has priority over --verbose, so a verbose run omits it
if [ "${STRATA_PREFLIGHT_VERBOSE:-}" = true ]; then args+=(--verbose); else args+=(--quiet); fi
# each rule id and each record pattern is one argument; read -ra splits and expands nothing
read -ra rules <<< "${STRATA_PREFLIGHT_RULE:-}"
for r in ${rules[@]+"${rules[@]}"}; do args+=(--rule "$r"); done
[ -z "$offline" ] || args+=(--offline)
read -ra records <<< "${STRATA_PREFLIGHT_RECORDS:-}"
for r in ${records[@]+"${records[@]}"}; do args+=(--records "$r"); done
out=${STRATA_PREFLIGHT_OUT:-preflight}

code=0
"$strata" preflight "${args[@]}" --out "$out" --format markdown || code=$?

if [ "${STRATA_PREFLIGHT_COMMENT:-}" = true ]; then
  if ! "$here/pr-comment.sh" --preflight "$out/report.md" && [ "$code" -eq 0 ]; then code=2; fi
fi
exit "$code"
