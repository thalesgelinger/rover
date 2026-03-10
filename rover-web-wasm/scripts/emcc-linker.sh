#!/usr/bin/env bash
set -euo pipefail

args=()
for arg in "$@"; do
  if [[ "$arg" == "-fwasm-exceptions" ]]; then
    continue
  fi
  args+=("$arg")
done

args+=("-sNO_DISABLE_EXCEPTION_CATCHING")
args+=("-sASSERTIONS=1")

exec emcc "${args[@]}"
