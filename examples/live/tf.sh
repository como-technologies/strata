#!/usr/bin/env bash
# terraform for examples/live. The Palette API key comes from SPECTROCLOUD_APIKEY, PALETTE_API_KEY,
# or ~/.palette.env, in that order, and is never printed. For a self-hosted Palette, PALETTE_API
# (the address strata uses, from the environment or ~/.palette.env) becomes the provider's
# SPECTROCLOUD_HOST unless that is already set, so strata and Terraform talk to the same Palette.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)

from_env_file() { # name -> value from ~/.palette.env, or empty
  [ -f "$HOME/.palette.env" ] || return 0
  (set -a; . "$HOME/.palette.env" >/dev/null 2>&1; printf %s "${!1:-}")
}

if [ -z "${SPECTROCLOUD_APIKEY:-}" ]; then
  key=${PALETTE_API_KEY:-$(from_env_file PALETTE_API_KEY)}
  [ -n "$key" ] || { echo "tf.sh: set PALETTE_API_KEY or put it in ~/.palette.env" >&2; exit 2; }
  export SPECTROCLOUD_APIKEY=$key
fi

if [ -z "${SPECTROCLOUD_HOST:-}" ]; then
  api=${PALETTE_API:-$(from_env_file PALETTE_API)}
  if [ -n "$api" ]; then
    host=${api#https://}
    host=${host#http://}
    export SPECTROCLOUD_HOST=${host%/}
  fi
fi

exec terraform -chdir="$here/terraform" "$@"
