#!/usr/bin/env bash
# Scan the text and the path of each tracked file of the repository for each entry of the deny
# list, and fail on a hit. The variable STEP14_DENY holds the list, with one entry on each line.
# The job `deny list` of the ci workflow gives it the value of the secret of the same name. The
# recipe `just deny-list` runs this script. "The deny list" in docs/src/development/build-test.md
# says why the list is a secret.
#
# An entry is a fixed string, and the case does not matter. For each hit in the text of a file, the
# script prints the path and the line number, or the path alone in a binary file. For the paths
# that hold an entry, it prints only their number, and it prints no such path. It never prints an
# entry or the text of a line. When the list has no entry, it gives a notice that it did not scan,
# and exits with 0.
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

# The same match for paths. Each path ends with a NUL, so a path can hold a newline.
match_paths() {
  grep --null-data -F -i -f "$list" "$@"
}

# A path is public too. So the script counts the tracked paths that hold an entry, and it prints
# no such path. grep exits with 1 when no path matches, and with a higher code on an error.
status=0
named=$(git ls-files -z | match_paths -c) || status=$?
[ "$status" -le 1 ] || exit "$status"

# git grep exits with 1 when no line matches, and with a higher code on an error.
status=0
search -l -z > "$found" || status=$?
[ "$status" -le 1 ] || exit "$status"

# Print the place of each hit in the text of the file at the path $2, with $1 in place of the path.
places() {
  local numbers number
  # With -h, each line of the output starts with the line number and a colon. cut keeps the number
  # alone, so the text of the line goes nowhere. -I skips a binary file, which gives no number.
  numbers=$(search -h -n -I -- "$2" | cut -d: -f1) || true
  if [ -z "$numbers" ]; then
    printf '%s\n' "$1"
    return
  fi
  while IFS= read -r number; do
    printf '%s:%s\n' "$1" "$number"
  done <<< "$numbers"
}

hits=0
hidden=''
while IFS= read -r -d '' path; do
  hits=$((hits + 1))
  # The script shows the path only when grep finds no entry in it. On an error, it hides the path.
  status=0
  printf '%s\0' "$path" | match_paths -q || status=$?
  if [ "$status" -eq 1 ]; then
    places "$path" "$path"
  else
    # git grep gives the paths in their order. So the hits of a hidden path follow the others, and
    # their place in the output does not show the start of the path.
    hidden+=$(places '<a path that holds an entry>' "$path")$'\n'
  fi
done < "$found"
printf '%s' "$hidden"

if [ "$hits" -eq 0 ] && [ "$named" -eq 0 ]; then
  echo "deny-list: no hit"
  exit 0
fi
if [ "$hits" -gt 0 ]; then
  echo "::error title=deny list::An entry of the deny list is in $hits of the tracked files. Remove it at each path and line above."
fi
if [ "$named" -gt 0 ]; then
  echo "::error title=deny list::An entry of the deny list is in $named of the tracked paths. The scan prints no such path. Change each one."
fi
exit 1
