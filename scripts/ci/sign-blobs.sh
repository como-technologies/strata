#!/usr/bin/env bash
# Sign each file with cosign, and write the bundle FILE.sigstore.json beside it. The release signs
# with no key of its own: each try makes a new key and asks the transparency log of Sigstore for an
# entry. The log can refuse one try, for example with 409 "an equivalent entry already exists".
# So the script tries each file up to 5 times, with 30 seconds between two tries. It removes a
# partial bundle before each try. After the last try of a file, it prints the last error of cosign
# and stops. It does not go on to the next file.
#
#   sign-blobs.sh FILE...
#
# SIGN_TRIES and SIGN_WAIT change the 5 tries and the 30 seconds. The release workflow runs this
# script, and no release carries it. Needs cosign 3 or later.
set -euo pipefail
tries=${SIGN_TRIES:-5}
pause=${SIGN_WAIT:-30}
files=()
while [ $# -gt 0 ]; do
  case $1 in
    -h|--help) sed -n '2,/^set -euo/{/^#/p}' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --) shift; files+=("$@"); break ;;
    -*) echo "sign-blobs: unknown option $1" >&2; exit 2 ;;
    *) files+=("$1"); shift ;;
  esac
done
[ "${#files[@]}" -gt 0 ] || { echo "sign-blobs: no file to sign" >&2; exit 2; }
case $tries in ''|*[!0-9]*|0) echo "sign-blobs: SIGN_TRIES must be a number above 0" >&2; exit 2 ;; esac
case $pause in ''|*[!0-9]*) echo "sign-blobs: SIGN_WAIT must be a number of seconds" >&2; exit 2 ;; esac
error=$(mktemp)
trap 'rm -f "$error"' EXIT

for file in "${files[@]}"; do
  bundle=$file.sigstore.json
  for ((try = 1; try <= tries; try++)); do
    echo "sign-blobs: $file, try $try of $tries"
    rm -f "$bundle"
    if cosign sign-blob --yes "$file" --bundle "$bundle" 2> "$error"; then
      cat "$error" >&2
      continue 2
    fi
    cat "$error" >&2
    rm -f "$bundle"
    [ "$try" -lt "$tries" ] || break
    echo "sign-blobs: $file failed, next try in $pause seconds" >&2
    sleep "$pause"
  done
  echo "sign-blobs: cosign did not sign $file in $tries tries" >&2
  exit 1
done
