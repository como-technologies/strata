#!/usr/bin/env bash
# The profile export test (#156): Palette exports a profile version as one file, and
# `snapshot --from-candidate` reads that file as a candidate. Stage 4 reads the download of each
# version, the file that `same-release --allow renumber` reads (#751). It builds no cluster, creates no
# profile, and needs no OpenTofu: each request is a GET. So it runs whenever the key works.
#
#   examples/live/export.sh [STAGE]
#
#   1  for each version: the record of the version, the export with format=yaml, the record of
#      the export, and the two roots. A version with a manifest pack differs only by the pack uid
#      that Palette mints.
#   2  the export with format=json gives the same record as the export with format=yaml
#   3  what the export route answers: a request that accepts JSON gets no file; and three
#      exports at once, whose refusals the run records
#   4  the download of each version: the layout of the file, the archive of each pack, the
#      packs with no archive, the archive of each chart of a Helm registry (#817), and what the
#      route answers to a request that accepts JSON and to two downloads at once
#
# PROFILES holds the versions, by <name>@<version>, in the project of the key. The default is the
# three versions that `just live-up` publishes. Palette can refuse an export with HTTP 429 and
# the message "only 1 request in 10 seconds is allowed", so the script waits 11 seconds before
# each export. Palette takes one download of a version in 10 seconds too, so the script waits 11
# seconds before each download.
#
# Results go under runs/<UTC time>/export/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=export
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

stage=${1:-all}
profiles=${PROFILES:-strata-live-profile@1.0.0 strata-live-registries@1.0.0 strata-live-addon@1.0.0}

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/export}
mkdir -p "$run" && cd "$run" || exit 2

summary() {
  write_summary "profile export test" ": \`$profiles\`."
}
trap summary EXIT

# export_file ID WHAT UID FORMAT: one export, after the wait that the limit of Palette needs
export_file() {
  sleep 11
  check "$1" 0 "$2" -- "$live/api.sh" export "$3" "$4"
}

# download_layout FILE RECORD LIST: the one metadata.json at the top of the download FILE names the
# list of the packs, which goes to LIST. The list names each pack of RECORD with a registry at its
# version, and no manifest pack by its name
download_layout() {
  local top path
  top=$(tar -tzf "$1" | sed -n 's|^\([^/]*\)/metadata\.json$|\1|p')
  [ "$(printf '%s\n' "$top" | grep -c .)" = 1 ] || { echo "no one metadata.json at the top" >&2; return 1; }
  path=$(tar -xzOf "$1" "$top/metadata.json" | jq -r '.profiles[0].packsMetaFilePath') || return 1
  tar -xzOf "$1" "$top/$path" > "$3" || return 1
  echo "$top/$path" > "$3.path"
  jq -e --slurpfile record "$2" '.packs as $list | $record[0].packs
    | all(if .type == "manifest" then $list[.name] == null else $list[.name].version == .version end)' "$3"
}

# download_archives FILE LIST: each archive of a pack that the list LIST of the download FILE names
# has one top folder, and its pack.json names the version of its entry. A chart of a Helm registry
# has no pack.json, so the check reads the archives of the other packs, and download_charts reads
# the charts
download_archives() {
  local folder name path version top
  folder=$(dirname "$(cat "$2.path")")
  while IFS=$'\t' read -r name path version; do
    top=$(tar -xzOf "$1" "$folder/$path" | tar -tz | cut -d/ -f1 | sort -u)
    [ "$(printf '%s\n' "$top" | grep -c .)" = 1 ] || { echo "$name: no one top folder" >&2; return 1; }
    [ "$(tar -xzOf "$1" "$folder/$path" | tar -xzO "$top/pack.json" | jq -r .version)" = "$version" ] \
      || { echo "$name: pack.json does not name $version" >&2; return 1; }
    echo "$name $version: $top/pack.json"
  done < <(jq -r '.packs[] | select(.filePath != "" and (.manifestName // "") == "" and .type != "helm")
    | [.name, .filePath, .version] | @tsv' "$2")
}

# download_charts LIST...: each chart of a Helm registry with an archive in the lists LIST has one
# top folder and no pack.json. The line version of its Chart.yaml names the version of its entry.
# The download of LIST is the file NAME.tgz beside NAME.list.json. With no such chart, it fails
download_charts() {
  local list file folder name path version files top charts=0
  for list in "$@"; do
    file=${list%.list.json}.tgz
    folder=$(dirname "$(cat "$list.path")")
    while IFS=$'\t' read -r name path version; do
      files=$(tar -xzOf "$file" "$folder/$path" | tar -tz) || { echo "$name: no archive" >&2; return 1; }
      top=$(printf '%s\n' "$files" | cut -d/ -f1 | sort -u)
      [ "$(printf '%s\n' "$top" | grep -c .)" = 1 ] || { echo "$name: no one top folder" >&2; return 1; }
      if printf '%s\n' "$files" | grep -qx "$top/pack.json"; then
        echo "$name: the chart holds pack.json" >&2
        return 1
      fi
      [ "$(tar -xzOf "$file" "$folder/$path" | tar -xzO "$top/Chart.yaml" \
        | sed -n 's/^version:[[:space:]]*//p' | tr -d "\"'\r")" = "$version" ] \
        || { echo "$name: Chart.yaml does not name $version" >&2; return 1; }
      echo "$name $version: $top/Chart.yaml"
      charts=$((charts + 1))
    done < <(jq -r '.packs[] | select(.type == "helm" and .filePath != "") | [.name, .filePath, .version] | @tsv' "$list")
  done
  [ "$charts" -gt 0 ] || { echo "no list names a chart of a Helm registry with an archive" >&2; return 1; }
}

# the record of each version gives its uid, and the root that the export must give
for ref in $profiles; do
  name=${ref%@*}
  "$strata" snapshot "$ref" -q -o "$name.version.json" || die "no record of $ref"
done

if [ "$stage" = all ] || [ "$stage" = 1 ]; then
  echo "== the export of each version, read as a candidate (#156)" | tee -a steps.log
  for ref in $profiles; do
    name=${ref%@*}
    uid=$(jq -r .source.uid "$name.version.json")
    export_file "export-$name" "#156: GET /v1/clusterprofiles/{uid}/export?format=yaml gives the file of $ref" "$uid" yaml
    cp "export-$name.out" "$name.yaml"
    check "export-record-$name" 0 "#156: snapshot --from-candidate reads the export of $ref" -- \
      "$strata" snapshot --from-candidate "$name.yaml" -o "$name.export.json"
    check "export-same-$name" 0 "#156: same-release passes the record of the export and the record of $ref" -- \
      "$strata" same-release "$name.export.json" "$name.version.json"
    if jq -e 'any(.packs[]; .type == "manifest")' "$name.version.json" > /dev/null; then
      check "export-minted-$name" 0 "#156: the roots differ only by the uid that Palette mints for the manifest pack" -- \
        bash -c '"$1" same-release "$2" "$3" --json | jq -e ".pairs[0].mintedUidOnly | length > 0"' _ \
        "$strata" "$name.export.json" "$name.version.json"
    else
      check "export-root-$name" 0 "#156: the record of the export has the root of $ref" -- \
        test "$(jq -r .root "$name.export.json")" = "$(jq -r .root "$name.version.json")"
    fi
  done
fi

if [ "$stage" = all ] || [ "$stage" = 2 ]; then
  echo "== the export as JSON (#156)" | tee -a steps.log
  ref=${profiles%% *}
  name=${ref%@*}
  uid=$(jq -r .source.uid "$name.version.json")
  export_file "export-json-$name" "#156: format=json gives the same file as JSON" "$uid" json
  cp "export-json-$name.out" "$name.export-file.json"
  check "export-json-record-$name" 0 "#156: snapshot --from-candidate reads the JSON export" -- \
    "$strata" snapshot --from-candidate "$name.export-file.json" -o "$name.export-json.json"
  check "export-json-root-$name" 0 "#156: the JSON export gives the root of $ref" -- \
    test "$(jq -r .root "$name.export-json.json")" = "$(jq -r .root "$name.version.json")"
fi

if [ "$stage" = all ] || [ "$stage" = 3 ]; then
  echo "== what the export route answers (#156)" | tee -a steps.log
  ref=${profiles%% *}
  name=${ref%@*}
  uid=$(jq -r .source.uid "$name.version.json")
  sleep 11
  check export-accepts-json 0 '#156: a request that accepts JSON gets an answer' -- \
    "$live/api.sh" get "/v1/clusterprofiles/$uid/export?format=yaml"
  check export-accepts-json-no-file 0 '#156: the answer is {"Reader":{}}, and not the file' -- \
    grep -qx '{"Reader":{}}' export-accepts-json.out
  sleep 11
  # three exports at once: some runs see HTTP 429 TooManyRequests, and some do not, so the check
  # records the exit code and the answers, and does not judge them
  check export-limit '?' '#156: three exports at once; exit 2 when Palette refused one or more' -- \
    bash -c 'refused=0; for i in 1 2 3; do "$1" export "$2" yaml > /dev/null || refused=2; done; exit $refused' _ \
    "$live/api.sh" "$uid"
fi

if [ "$stage" = all ] || [ "$stage" = 4 ]; then
  echo "== the download of each version (#751)" | tee -a steps.log
  for ref in $profiles; do
    name=${ref%@*}
    uid=$(jq -r .source.uid "$name.version.json")
    sleep 11
    check "download-$name" 0 "#751: GET /v1/clusterprofiles/{uid}/spc/download gives the file of $ref" -- \
      "$live/api.sh" download "$uid"
    cp "download-$name.out" "$name.tgz"
    check "download-layout-$name" 0 "#751: metadata.json at the top names the list of the packs, and the list names each pack of $ref with a registry at its version" -- \
      download_layout "$name.tgz" "$name.version.json" "$name.list.json"
    check "download-archive-$name" 0 "#751: each archive of a pack of $ref has one top folder, and its pack.json names the version of its entry" -- \
      download_archives "$name.tgz" "$name.list.json"
  done
  # which packs have no archive, and why: the records name the registry of each pack, and the
  # check records the entries and does not judge them
  check download-no-archive '?' '#751: the entry of each pack with no archive: its name, its type, and isRegistryPrivate' -- \
    bash -c 'for f in "$@"; do jq -r --arg f "$f" ".packs[] | select(.filePath == \"\" and (.manifestName // \"\") == \"\") | [\$f, .name, .type, .isRegistryPrivate] | @tsv" "$f"; done' _ ./*.list.json
  # Palette puts the chart of a Helm registry that is not private in the file as the registry
  # serves it. same-release does not compare the line version of its Chart.yaml (#817)
  check download-chart 0 '#817: each chart of a Helm registry has one top folder and no pack.json, and its Chart.yaml names the version of its entry' -- \
    download_charts ./*.list.json
  ref=${profiles%% *}
  name=${ref%@*}
  uid=$(jq -r .source.uid "$name.version.json")
  sleep 11
  check download-accepts-json 0 '#751: a request that accepts JSON gets an answer' -- \
    "$live/api.sh" get "/v1/clusterprofiles/$uid/spc/download"
  check download-accepts-json-no-file 0 '#751: the answer is {"Reader":{}}, and not the file, as for the export' -- \
    grep -qx '{"Reader":{}}' download-accepts-json.out
  sleep 11
  # two downloads of one version at once: Palette takes one in 10 seconds, and can refuse the
  # second with HTTP 429 TooManyRequests. The check records the exit code, and does not judge it
  check download-limit '?' '#751: two downloads of one version at once; exit 2 when Palette refused the second' -- \
    bash -c 'refused=0; for i in 1 2; do "$1" download "$2" > /dev/null || refused=2; done; exit $refused' _ \
    "$live/api.sh" "$uid"
fi

echo
echo "$script: $(grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
