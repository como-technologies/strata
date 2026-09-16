#!/usr/bin/env bash
# terraform for examples/live, with the Palette API key taken from SPECTROCLOUD_APIKEY,
# PALETTE_API_KEY, or ~/.palette.env, in that order. The key is never printed.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
if [ -z "${SPECTROCLOUD_APIKEY:-}" ]; then
  key=${PALETTE_API_KEY:-}
  if [ -z "$key" ] && [ -f "$HOME/.palette.env" ]; then
    key=$(set -a; . "$HOME/.palette.env" >/dev/null 2>&1; printf %s "${PALETTE_API_KEY:-}")
  fi
  [ -n "$key" ] || { echo "tf.sh: set PALETTE_API_KEY or put it in ~/.palette.env" >&2; exit 2; }
  export SPECTROCLOUD_APIKEY=$key
fi
exec terraform -chdir="$here/terraform" "$@"
