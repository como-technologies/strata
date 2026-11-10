# What the scripts in examples/live read about GitHub: the project of a checkout, the repository of
# the workspace, and the first line of the body of an issue. Source it. It runs nothing and calls no
# host.
#
# The scripts in scripts/ci/ do not source it. A user ships those scripts, so each one stands alone.

# remote_repo [DIR]: print OWNER/REPO of the origin remote of the checkout in DIR (default: the
# current directory) when the remote is on GitHub. Otherwise it prints nothing. It reads a local
# file, so a dry run still calls no host. The first pattern reads a remote with a scheme, such as
# `https://user@github.com/o/r`. The second reads the form of scp, `git@github.com:o/r`.
remote_repo() {
  git -C "${1:-.}" remote get-url origin 2> /dev/null \
    | sed -nE 's#^(https?|ssh|git)://([^/@]+@)?github\.com/([^/]+/[^/]+)$#\3#p; s#^[^/@:]+@github\.com:([^/]+/[^/]+)$#\1#p' \
    | sed -E 's#\.git$##' || true
}

# workspace_repo: print OWNER/REPO of the repository that the Cargo.toml of the root names. It reads
# a local file and calls no host.
workspace_repo() {
  sed -n 's|^repository = "https://github.com/\(.*\)"$|\1|p' \
    "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../../Cargo.toml"
}

# The jq function `first_line` gives the first line of the body of an issue, without a carriage
# return. This is the line that holds the marker of an issue. A body with no text gives an empty
# string, and never null, because `rtrimstr` fails on a null in some versions of jq. A program puts
# the definition before its own text, as in:
#
#   jq "$first_line_jq"'.[] | select(first_line == $marker)'
#
# The script exports the variable, so a `bash -c` of a check has it too.
export first_line_jq='def first_line: (.body // "") | (split("\n")[0] // "") | rtrimstr("\r");'
