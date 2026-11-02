# The two functions that each script in examples/live has: `die` and `usage`. Source it after you
# set `script` to the name of the script. It runs nothing and checks nothing, so a script that does
# not run strata can source it. lib.sh sources it too, and adds its check for a stale binary.

# die MESSAGE: print the message with the name of the script, and stop with exit code 2
die() { echo "$script: $*" >&2; exit 2; }

# usage: print the comment at the top of the script: from its second line to the line before
# `set -uo pipefail`, without the leading `# `. The script sources this file before it checks its
# arguments.
usage() { sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; }
