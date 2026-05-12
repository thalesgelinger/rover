#!/bin/sh
set -e

if [ "$1" != "${1#-}" ]; then
  exec rover "$@"
fi

case "$1" in
  build | check | db | fmt | help | lsp | repl | run)
    exec rover "$@"
    ;;
esac

exec "$@"
