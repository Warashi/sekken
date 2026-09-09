#!/usr/bin/env bash
# lisp/ の ERT テストをバッチで実行する。
set -euo pipefail
cd "$(dirname "$0")/.."
args=()
for f in test/*-test.el; do
  args+=(-l "$f")
done
emacs -Q --batch -L . -L test "${args[@]}" -f ert-run-tests-batch-and-exit
