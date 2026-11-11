#!/usr/bin/env bash
# The public repository (#65): a reader with no login opens the book, installs a release, and pulls
# the image and the chart of the observer. A job of the GitLab drop-in installs its release with no
# token (#483). The repository shows a description, topics, and the private advisory form.
#
#   examples/live/public.sh
#
# A request of a reader goes with no token and no login. The script removes STRATA_TOKEN,
# GITHUB_TOKEN, and GH_TOKEN from the install, and it asks the registry for an anonymous token
# only. One check reads with the login of gh: GitHub shows the setting of the advisory form to a
# maintainer only. The script writes nothing to GitHub, and it reads no Palette tenant.
#
# Needs curl, jq, gh, and the strata binary of this checkout: just build. REPO is OWNER/REPO
# (default: the repository that the Cargo.toml of the root names). Run it after the repository and
# its packages are public. Results go under runs/<UTC time>/public/, or OUT. Exit 0 only when no
# check fails.
set -uo pipefail
script=public
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
. "$live/github.sh"
case "${1:-}" in "") ;; *) usage; exit 2 ;; esac

repo=${REPO:-$(workspace_repo)}
[ -n "$repo" ] || die "Cargo.toml of the root names no repository"
owner=${repo%%/*}

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/public}
mkdir -p "$run" && cd "$run" || exit 2

summary() {
  write_summary "public repository" ": what a reader with no login gets from \`$repo\`."
}
trap summary EXIT

# book_opens: the address of the book that `strata --help` prints answers 200 with no login. The
# site of a private repository answers a redirect to a login.
book_opens() {
  local url
  url=$("$strata" --help | sed -n 's/^The book: //p')
  [ -n "$url" ] || return 1
  echo "$url"
  [ "$(curl -s -o book.html -w '%{http_code}' "$url")" = 200 ]
}

# install_no_token: the install command of the README as it is, with the folder of the run in place
# of ~/.local/bin, and with no token in the environment. A failed download gives bash no input, and
# bash then exits with 0. So the pipe fails when curl fails.
install_no_token() {
  local command
  command=$(awk '/^## Install$/ { section = 1; next }
    section && /^```bash$/ { block = 1; next }
    block && /^```$/ { exit }
    block' "$live/../../README.md" | sed "s#--dir ~/.local/bin#--dir '$run/bin'#")
  case "$command" in *"--dir '$run/bin'"*) ;; *) echo "the README gives no install into ~/.local/bin" >&2; return 1 ;; esac
  echo "$command"
  env -u STRATA_TOKEN -u GITHUB_TOKEN -u GH_TOKEN bash -c "set -o pipefail
$command"
}

# gitlab_install_no_token: the lines of the template .strata of the GitLab drop-in, as a job runs
# them, with no token (#483). They install the STRATA_VERSION of the drop-in from REPO. The lines of
# apt-get need the image of the job, so the check omits them. Then the job runs STRATA_BIN.
gitlab_install_no_token() {
  local drop_in=$live/../ci/gitlab/strata.yml lines version
  version=$(sed -n 's/^  STRATA_VERSION: //p' "$drop_in")
  lines=$(awk '/^\.strata:$/ { job = 1; next }
    job && /^[^ ]/ { exit }
    job && /^  before_script:$/ { steps = 1; next }
    steps && /^  [^ ]/ { exit }
    steps && /^    - \|$/ { next }
    steps && /^    - / { sub(/^    - /, ""); if ($0 !~ /^apt-get /) print; next }
    steps { sub(/^      /, ""); print }' "$drop_in")
  [ -n "$version" ] && [ -n "$lines" ] || { echo "the drop-in has no STRATA_VERSION or no .strata" >&2; return 1; }
  echo "$lines"
  mkdir -p "$run/gitlab"
  (cd "$run/gitlab" && env -u STRATA_TOKEN -u GITHUB_TOKEN -u GH_TOKEN \
    STRATA_REPO="$repo" STRATA_VERSION="$version" CI_PROJECT_DIR="$run/gitlab" \
    bash -eo pipefail -c "$lines
\"\$STRATA_BIN\" --version")
}

# anonymous_manifest NAME TAG: the registry gives the manifest of NAME:TAG for an anonymous token,
# as a pull with no login asks for it. A private package refuses the read.
anonymous_manifest() {
  local token
  token=$(curl -fsS "https://ghcr.io/token?scope=repository:$1:pull" | jq -r '.token // empty')
  [ -n "$token" ] || return 1
  curl -fsS -o /dev/null -H "Authorization: Bearer $token" \
    -H "Accept: application/vnd.oci.image.index.v1+json, application/vnd.oci.image.manifest.v1+json" \
    "https://ghcr.io/v2/$1/manifests/$2"
}

echo "== what a reader with no login gets (#65)" | tee -a steps.log
check book-opens 0 '#65: the address of the book in strata --help answers 200 with no login' -- book_opens
check install-no-token 0 '#65: the install command of the README, as it is, installs the latest release with no token' -- \
  install_no_token
check install-runs 0 '#65: the two binaries of the install run' -- \
  bash -c "'$run/bin/strata' --version && '$run/bin/strata-preflight' --version"
check gitlab-install-no-token 0 '#483: the install lines of the GitLab drop-in install its STRATA_VERSION with no token, and the binary runs' -- \
  gitlab_install_no_token
tag=$(sed -n 's/^strata \(.*\)$/v\1/p' install-runs.out | head -n 1)
check image-no-login 0 '#65: the registry gives the image of the observer of that release with no login' -- \
  anonymous_manifest "$owner/strata-observer" "$tag"
check chart-no-login 0 '#65: the registry gives the chart of the observer of that release with no login' -- \
  anonymous_manifest "$owner/charts/strata-observer" "${tag#v}"
check repository-public 0 '#65: the API of GitHub shows the repository with no token: public, with a description and topics' -- \
  bash -c "curl -fsS https://api.github.com/repos/$repo \
    | jq -e '.private == false and (.description // \"\" | length > 0) and (.topics | length > 0)'"
check advisory-form 0 '#65: private vulnerability reporting is on' -- \
  bash -c "gh api repos/$repo/private-vulnerability-reporting | jq -e '.enabled == true'"

echo "public: $failures check(s) failed; results in $run"
[ "$failures" = 0 ]
