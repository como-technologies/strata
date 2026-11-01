#!/usr/bin/env bash
# Name the fuzz targets of fuzz/, or run one of them under libFuzzer. The recipes `just fuzz-list`
# and `just fuzz` run this script. "The fuzzing" in docs/src/development/build-test.md says what
# a target is and where its corpus lives.
#
# Usage: fuzz.sh list
#        fuzz.sh run TARGET [TIME]
#
# TARGET is a name that `list` prints, or the start of one name. TIME is a duration for
# cargo-bolero, such as 10m, and the default is 60s. `run` needs cargo-bolero and a nightly
# toolchain: FUZZ_TOOLCHAIN names another rustup toolchain. A crash goes to the `crashes`
# directory of the target, and the script exits with a code that is not zero.
set -euo pipefail

package=strata-fuzz
# The libFuzzer default of 4096 bytes cuts a record of the corpus, which is longer.
max_input=65536

targets() {
  cargo bolero list -p "$package" | jq -r '.test' | LC_ALL=C sort
}

die() {
  echo "fuzz: $*" >&2
  exit 2
}

case "${1:-}" in
  list)
    targets
    ;;
  run)
    wanted=${2:-}
    [ -n "$wanted" ] || die "name a target: fuzz.sh run TARGET [TIME]; just fuzz-list names them"
    time=${3:-60s}
    all=$(targets)
    # a name that is whole wins over the names that it starts
    if printf '%s\n' "$all" | grep -Fxq -- "$wanted"; then
      matches=$wanted
    else
      matches=$(printf '%s\n' "$all" | awk -v start="$wanted" 'index($0, start) == 1')
    fi
    case $(printf '%s' "$matches" | grep -c .) in
      0) die "no target starts with '$wanted'; just fuzz-list names them" ;;
      1) ;;
      *) die "more than one target starts with '$wanted':
$matches" ;;
    esac
    exec cargo bolero test "$matches" -p "$package" --toolchain "${FUZZ_TOOLCHAIN:-nightly}" \
      -T "$time" -l "$max_input"
    ;;
  *)
    die "usage: fuzz.sh list | run TARGET [TIME]"
    ;;
esac
