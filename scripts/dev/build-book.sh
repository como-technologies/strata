#!/usr/bin/env bash
# Build the book with mdbook, and fail when mdbook prints a warning. mdbook exits with code 0 on
# a warning, so CI passes a page that it warns about. The script reads the log, and fails on a
# WARN or ERROR line of mdbook. A note of a preprocessor is not such a line.
# Usage: build-book.sh [BOOK_DIR]    (default: docs)
set -euo pipefail

book=${1:-docs}
log=$(mktemp)
trap 'rm -f "$log"' EXIT

status=0
mdbook build "$book" > "$log" 2>&1 || status=$?
cat "$log" >&2
[ "$status" -eq 0 ] || exit "$status"

# mdbook colors its log level on a terminal, and the color comes before the level.
esc=$(printf '\033')
if sed "s/${esc}\[[0-9;]*m//g" "$log" | grep -Eq '^ *(WARN|ERROR) '; then
  echo "build-book: mdbook printed a warning; fix the page, because mdbook exits 0 on it" >&2
  exit 1
fi
