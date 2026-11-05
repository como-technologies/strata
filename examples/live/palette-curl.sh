# The one call of curl that sends the key of Palette: palette_curl. Source it. It runs nothing and calls no host.
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
