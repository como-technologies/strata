#!/usr/bin/env bash
# The test of the rule rust/cleartext-logging of strata (#843). The job `the test of the rule` of
# the workflow codeql runs this script before and after an analysis of CodeQL. "CodeQL" in
# docs/src/development/build-test.md says what the rule omits.
#
# Usage: codeql-rule.sh crate DIR
#        codeql-rule.sh check SARIF DIR
#
# `crate` writes a small crate to DIR. Its main function sends a uid, the name of an account, a
# field `uid`, and the result of `name_or_uid` to `println!`. It also sends the API key of Palette.
# Each line of src/main.rs that sends a value ends with `// alert` or `// no alert`.
#
# `check` reads SARIF, the results of CodeQL on the crate in DIR. The results must hold one rule
# with the id rust/cleartext-logging, and that rule must have the tag `strata`. The rule must give
# an alert on each line with `// alert`, and on no other line. Exit 0 when it does, 1 when it does
# not, and 2 for a wrong use.
set -euo pipefail

rule=rust/cleartext-logging

die() {
  echo "codeql-rule: $*" >&2
  exit 2
}

# The lines of the file $1 that end with the mark $2, by their numbers, on one line.
marked() {
  grep -n -e "$2\$" "$1" | cut -d: -f1 | sort -n | paste -sd ' ' -
}

crate() {
  mkdir -p "$1/src"
  # the table `workspace` makes the crate a workspace of its own
  cat > "$1/Cargo.toml" << 'EOF'
[package]
name = "codeql-rule"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]
EOF
  cat > "$1/src/main.rs" << 'EOF'
//! The crate of the test of the rule rust/cleartext-logging of strata.

struct Cluster {
    uid: String,
}

fn name_or_uid(name: &str, uid: &str) -> String {
    if name.is_empty() { uid.to_string() } else { name.to_string() }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let project_uid = args.next().unwrap_or_default();
    let account_name = args.next().unwrap_or_default();
    let cluster = Cluster { uid: args.next().unwrap_or_default() };
    let api_key = std::env::var("PALETTE_API_KEY").unwrap_or_default();
    println!("project {}", project_uid); // no alert
    println!("account {}", account_name); // no alert
    println!("cluster {}", &cluster.uid); // no alert
    println!("profile {}", name_or_uid(&account_name, &project_uid)); // no alert
    println!("key {}", api_key); // alert
}
EOF
}

check() {
  local sarif=$1 main=$2/src/main.rs rules tags want got
  [ -f "$sarif" ] || die "no results at $sarif"
  [ -f "$main" ] || die "no crate at $2"
  # CodeQL lists the rules of the queries of a pack in an extension of the tool
  rules=$(jq --arg r "$rule" '[.runs[].tool | .driver, .extensions[]? | .rules[]? | select(.id == $r)]' "$sarif")
  [ "$(jq length <<< "$rules")" = 1 ] || {
    echo "codeql-rule: the results hold $(jq length <<< "$rules") rules with the id $rule, not 1" >&2
    exit 1
  }
  tags=$(jq -r '.[0].properties.tags // [] | join(" ")' <<< "$rules")
  jq -e 'any(.[0].properties.tags[]?; . == "strata")' <<< "$rules" > /dev/null || {
    echo "codeql-rule: the rule $rule has no tag strata, so it is the rule of CodeQL: $tags" >&2
    exit 1
  }
  want=$(marked "$main" '// alert')
  got=$(jq -r --arg r "$rule" '.runs[].results[]? | select(.ruleId == $r)
      | .locations[0].physicalLocation | select(.artifactLocation.uri == "src/main.rs")
      | .region.startLine' "$sarif" | sort -n -u | paste -sd ' ' -)
  [ "$got" = "$want" ] || {
    echo "codeql-rule: the rule $rule gives an alert on the lines '$got' of src/main.rs; the lines with '// alert' are '$want'" >&2
    exit 1
  }
  echo "codeql-rule: the rule $rule gives an alert on the lines '$got' of src/main.rs, and on no line with '// no alert'"
}

case "${1:-}" in
  crate)
    [ $# = 2 ] || die "usage: codeql-rule.sh crate DIR"
    crate "$2"
    ;;
  check)
    [ $# = 3 ] || die "usage: codeql-rule.sh check SARIF DIR"
    check "$2" "$3"
    ;;
  *) die "usage: codeql-rule.sh crate DIR | check SARIF DIR" ;;
esac
