#!/usr/bin/env bash
# Install or remove the weekly contract run as a systemd user timer.
#
#   deploy/contract/timer.sh install   write the units for this checkout, and start the timer
#   deploy/contract/timer.sh remove    stop the timer, and delete the units
#   deploy/contract/timer.sh status    show the timer, and the last result of the run
#
# The units are strata-contract.service and strata-contract.timer, in ~/.config/systemd/user/. The
# service runs deploy/contract/weekly.sh of this checkout, so install from a clone of main that
# nothing else uses: the run updates it with git pull. The run keeps its results in
# ~/.local/share/strata-contract/, or in STRATA_CONTRACT_DIR. remove keeps them.
#
# A user timer runs when the user has no session only if the user has linger: loginctl enable-linger.
# install tells when it is off. No secret goes into a unit: the run reads ~/.palette.env.
set -euo pipefail

script=timer
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$here/../.." && pwd)
# shellcheck source=examples/live/common.sh
. "$repo/examples/live/common.sh"
units=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user
# shellcheck source=deploy/contract/folder.sh
. "$here/folder.sh"
names=(strata-contract.service strata-contract.timer)

case "${1:-}" in
  install)
    command -v systemctl > /dev/null || die "systemctl is not installed"
    mkdir -p "$units"
    for name in "${names[@]}"; do
      sed "s#@REPO@#$repo#g" "$here/$name" > "$units/$name"
    done
    systemctl --user daemon-reload
    systemctl --user enable --now strata-contract.timer
    echo "$script: the timer runs deploy/contract/weekly.sh of $repo once a week"
    if [ "$(loginctl show-user "$(id -un)" --property=Linger --value 2> /dev/null)" != yes ]; then
      echo "$script: the user has no linger, so the timer runs only while the user has a session: loginctl enable-linger $(id -un)" >&2
    fi
    ;;
  remove)
    command -v systemctl > /dev/null || die "systemctl is not installed"
    systemctl --user disable --now strata-contract.timer || true
    for name in "${names[@]}"; do
      rm -f "$units/$name"
    done
    systemctl --user daemon-reload
    echo "$script: the timer is removed; the results stay in $data"
    ;;
  status)
    command -v systemctl > /dev/null || die "systemctl is not installed"
    systemctl --user list-timers strata-contract.timer --no-pager || true
    if [ -f "$data/last.json" ] && command -v jq > /dev/null; then
      echo "the last run: $(jq -r .checkedAt "$data/last.json"), $(jq -r -f "$repo/examples/live/contract-summary.jq" "$data/last.json")"
    else
      echo "$script: no result in $data yet"
    fi
    ;;
  -h|--help) sed -n '2,/^set -euo/{/^#/p}' "$0" | sed 's/^# \{0,1\}//' ;;
  *) sed -n '2,/^set -euo/{/^#/p}' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2 ;;
esac
