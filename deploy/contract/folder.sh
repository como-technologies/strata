# The folder that keeps the results of the weekly contract run: runs/, last.json, and state.json. It
# is ~/.local/share/strata-contract/, or STRATA_CONTRACT_DIR. weekly.sh and timer.sh source this file,
# so that they read one folder. The file sets $data and nothing else.
data=${STRATA_CONTRACT_DIR:-$HOME/.local/share/strata-contract}
