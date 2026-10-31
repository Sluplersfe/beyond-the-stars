#!/usr/bin/env bash
# Builds and runs Beyond the Stars.
#   ./run.sh            optimised build (default)
#   ./run.sh --dev      faster compile, slower game
#   ./run.sh --no-html  skip the HTML main-menu background (no GTK/WebKit needed)
# Anything after `--` is passed to the game.
set -euo pipefail
cd "$(dirname "$0")"

profile=(--release)
features=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dev) profile=() ;;
    --no-html) features=(--no-default-features) ;;
    --) shift; break ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

exec cargo run -p game "${profile[@]}" "${features[@]}" -- "$@"
