# The functions that the scripts of the observer share. A script sources this file, after it sets:
#   root  the root of the repository
#   strata  the strata binary
#   work  a directory that the script removes at its end
#   pg    empty; start sets it to the process group of the stand-in, and stop empties it
#   records_revision  the revision that meta.json of records_dir names
# This file has no shebang and no `set`: the script that sources it owns both.

# start NAME serves the tenant of the playground state NAME, as `just playground NAME` does, and sets
# the variables that the book prints for it. It fails when the stand-in does not start in 5 minutes.
start() {
  local log=$work/$1.log
  (cd "$root" && exec setsid just playground "$1") >"$log" 2>&1 &
  pg=$!
  for _ in $(seq 300); do
    grep -q 'export PALETTE_API=' "$log" && break
    sleep 1
  done
  grep -q 'export PALETTE_API=' "$log" || { cat "$log" >&2; echo "the stand-in did not start: $1" >&2; exit 2; }
  sleep 1
  # shellcheck disable=SC1090
  source <(sed 's/\x1b\[[0-9;]*m//g' "$log" | grep -E '^export [A-Z_]+=')
  export STRATA_REPORT=0 XDG_STATE_HOME=$work/state STRATA_QUIET=1
}

# stop ends the stand-in that start made.
stop() {
  kill -- "-$pg" 2>/dev/null || true
  wait "$pg" 2>/dev/null || true
  pg=
  sleep 1
}

# records_dir NAME FILE... writes the records of a baseline as *.json.gz, with a meta.json, in the
# directory records-NAME of the work directory. The pod reads four Secrets in the same shape.
records_dir() {
  local dir=$work/records-$1
  shift
  mkdir -p "$dir"
  for file in "$@"; do gzip -c "$file" >"$dir/$(basename "$file" .json).json.gz"; done
  echo "{\"baseline\":\"approved\",\"revision\":\"$records_revision\"}" >"$dir/meta.json"
}

# cluster_uid NAME gives the uid of the cluster NAME in the tenant of the stand-in that runs now. It
# gives the first cluster with that name, and an empty line when there is none.
cluster_uid() {
  local uids
  uids=$("$strata" list | sed -n "s/.*cluster:\\([0-9a-f]\\{24\\}\\) *$1 .*/\\1/p")
  echo "${uids%%$'\n'*}"
}
