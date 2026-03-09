#!/usr/bin/env bash
set -euo pipefail

args=()
for arg in "$@"; do
  if [[ "$arg" == "-fwasm-exceptions" ]]; then
    continue
  fi
  args+=("$arg")
done

exec emcc "${args[@]}"
