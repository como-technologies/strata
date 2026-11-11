# The two functions of a request to Palette that the scripts share. palette_curl is the one call of curl that sends
# the key of Palette, and registry_base gives the path of a registry. Source it. It runs nothing and calls no host.
# Source common.sh first, or define a function `die` before the first call.

# palette_curl ARGUMENT...: run curl with the arguments and with the header `ApiKey: $KEY`. The header goes to curl as a
# config on standard input (`curl --config -`). So the key reaches no argument of the process, and /proc/<pid>/cmdline does not
# show it. An empty KEY sends no ApiKey header. A key with a line break stops the script, because it could add a line to the
# config. A caller sets the other headers with `-H` as before, and it never puts the key in an argument.
palette_curl() {
  [ -n "${KEY:-}" ] || { curl "$@"; return; }
  case $KEY in *$'\n'* | *$'\r'*) die "the key of Palette holds a line break" ;; esac
  local header=${KEY//\\/\\\\}
  header=${header//\"/\\\"}
  curl --config - "$@" <<< "header = \"ApiKey: $header\""
}

# The kinds of registry_base, for a usage line.
REGISTRY_KINDS="oci-ecr, oci-basic, helm, or pack"

# registry_base KIND UID: the path of the registry UID of KIND: oci-ecr, oci-basic (an OCI registry of the type basic),
# helm, or pack (a Spectro pack registry). It fails for another KIND. A test makes sure that no other script builds
# such a path.
registry_base() {
  case "$1" in
    oci-ecr) echo "/v1/registries/oci/$2/ecr" ;;
    oci-basic) echo "/v1/registries/oci/$2/basic" ;;
    helm) echo "/v1/registries/helm/$2" ;;
    pack) echo "/v1/registries/pack/$2" ;;
    *) return 1 ;;
  esac
}
