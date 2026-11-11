#!/usr/bin/env bash
# Scan each tracked file of the repository for each entry of the deny list, and fail on a hit.
# The variable STEP14_DENY holds the list, with one entry on each line. The job `deny list` of the
# ci workflow gives it the value of the secret of the same name. The recipe `just deny-list` runs
# this script. "The deny list" in docs/src/development/build-test.md says why the list is a secret.
#
# An entry is a fixed string, and the case does not matter. For each hit, the script prints the
# path and the line number, or the path alone in a binary file. It never prints an entry or the
# text of a line. When the list has no entry, it gives a notice that it did not scan, and exits
# with 0.
set -euo pipefail
# A trace prints the value of each variable that a line expands, so it can print the list.
set +x

top=$(git rev-parse --show-toplevel)
cd "$top"

temp=${RUNNER_TEMP:-${TMPDIR:-/tmp}}
list=$(mktemp "$temp/deny-list.XXXXXX")
found=$(mktemp "$temp/deny-list.XXXXXX")
trap 'rm -f "$list" "$found"' EXIT

# An empty pattern matches each line. So the script removes each empty line, and the spaces at the
# two ends of an entry. The carriage return of a line from Windows is a space too.
printf '%s\n' "${STEP14_DENY-}" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' -e '/^$/d' > "$list"
if [ ! -s "$list" ]; then
  echo "::notice title=deny list::STEP14_DENY holds no entry, so deny-list did not scan the tree."
  exit 0
fi

echo "deny-list: scanned $(git ls-files | wc -l) tracked files"

search() {
  git --literal-pathspecs grep --no-color -F -i -f "$list" "$@"
}

# git grep exits with 1 when no line matches, and with a higher code on an error.
status=0
search -l -z > "$found" || status=$?
[ "$status" -le 1 ] || exit "$status"

hits=0
while IFS= read -r -d '' path; do
  hits=$((hits + 1))
  # With -h, each line of the output starts with the line number and a colon. cut keeps the number
  # alone, so the text of the line goes nowhere. -I skips a binary file, which gives no number.
  numbers=$(search -h -n -I -- "$path" | cut -d: -f1) || true
  if [ -z "$numbers" ]; then
    printf '%s\n' "$path"
    continue
  fi
  while IFS= read -r number; do
    printf '%s:%s\n' "$path" "$number"
  done <<< "$numbers"
done < "$found"

if [ "$hits" -eq 0 ]; then
  echo "deny-list: no hit"
  exit 0
fi
echo "::error title=deny list::An entry of the deny list is in $hits of the tracked files. Remove it at each path and line above."
exit 1
