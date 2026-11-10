#!/usr/bin/env bash
# Say whether a Palette key is in a place where strata looks for one, before a job asks Palette.
#
#   scripts/ci/palette-key.sh
#
# Strata reads the key from PALETTE_API_KEY or SPECTROCLOUD_APIKEY. When the environment gives
# neither, strata reads the two names from ~/.palette.env. This script looks in the same places,
# and it reads the file as strata does. A line can start with `export `, and quotes can hold the
# value. A line that starts with # is a comment. An empty value is no key.
#
# Exit 0 when the script finds a key. It then prints nothing. Exit 1 when it finds no key. It
# then prints the places where it looked, in one line. Exit 2 for an unknown option. The script
# never prints a key.
#
# merge-gate.sh and preflight.sh call this script, so it ships beside them. A job can call it too,
# and choose a step that needs the key.
set -euo pipefail

die() { echo "palette-key: $*" >&2; exit 2; }

while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) sed -n '2,/^set -euo/{/^#/p}' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option $1" ;;
  esac
done

names=(PALETTE_API_KEY SPECTROCLOUD_APIKEY)

# trim NAME: remove the white space at the start and at the end of the variable NAME
trim() {
  local v=${!1}
  v=${v#"${v%%[![:space:]]*}"}
  v=${v%"${v##*[![:space:]]}"}
  printf -v "$1" %s "$v"
}

# in_file NAME: 0 when a line of the file on standard input gives NAME a value. Strata reads each
# line in this order: it removes the white space at the two ends, then `export `, and it splits the
# line at the first =. It removes the white space and then each quote at the two ends of the
# value: first the double quotes, then the single quotes. A line with an empty value gives no key,
# and the next lines still count.
in_file() {
  local line name value
  while IFS= read -r line || [ -n "$line" ]; do
    trim line
    case $line in ''|'#'*) continue ;; esac
    line=${line#export }
    case $line in *=*) ;; *) continue ;; esac
    name=${line%%=*}
    trim name
    [ "$name" = "$1" ] || continue
    value=${line#*=}
    trim value
    while [ "${value#\"}" != "$value" ]; do value=${value#\"}; done
    while [ "${value%\"}" != "$value" ]; do value=${value%\"}; done
    while [ "${value#\'}" != "$value" ]; do value=${value#\'}; done
    while [ "${value%\'}" != "$value" ]; do value=${value%\'}; done
    [ -z "$value" ] || return 0
  done
  return 1
}

for name in "${names[@]}"; do
  [ -z "${!name:-}" ] || exit 0
done
file="${HOME:-}/.palette.env"
if [ -n "${HOME:-}" ] && [ -f "$file" ] && [ -r "$file" ]; then
  for name in "${names[@]}"; do
    if in_file "$name" < "$file"; then exit 0; fi
  done
fi
echo "no Palette key in PALETTE_API_KEY, SPECTROCLOUD_APIKEY, or ~/.palette.env"
exit 1
