#!/usr/bin/env bash
# Build the book with mdbook, and fail when mdbook prints a warning. mdbook exits 0 on a warning,
# for example a tag in a table cell that nothing closes, so CI passes the page unless a script
# reads the log. A note of a preprocessor does not count: only the WARN and ERROR lines of mdbook.
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
