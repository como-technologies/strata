#!/usr/bin/env bash
# tofu for examples/live. The Palette API key comes from SPECTROCLOUD_APIKEY, PALETTE_API_KEY,
# or ~/.palette.env, in that order, and is never printed. For a self-hosted Palette, PALETTE_API
# (the address strata uses, from the environment or ~/.palette.env) becomes the provider's
# SPECTROCLOUD_HOST unless that is already set, so strata and OpenTofu talk to the same Palette.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)

command -v tofu >/dev/null || { echo "tf.sh: tofu is not installed. Run: just tofu-install" >&2; exit 2; }

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

# Palette refuses one read of a profile with OperationForbidden now and then, and the refusals
# came under the provider's parallel refresh of the profiles: a serial refresh did not see one.
# So a plan, an apply, and a destroy run one resource at a time. The example is small.
case "${1:-}" in
  plan | apply | destroy) set -- "$1" -parallelism=1 "${@:2}" ;;
esac

exec tofu -chdir="$here/terraform" "$@"
