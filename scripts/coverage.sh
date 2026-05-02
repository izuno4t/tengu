#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/coverage.sh [summary|html|lcov]

Environment:
  COVERAGE_MIN_LINES  Minimum line coverage percentage. Default: 80.
  LLVM_COV            Path to llvm-cov matching the active rustc, if needed.
  LLVM_PROFDATA       Path to llvm-profdata matching the active rustc, if needed.
USAGE
}

mode="${1:-summary}"
min_lines="${COVERAGE_MIN_LINES:-80}"

case "$mode" in
  summary | html | lcov) ;;
  -h | --help)
    usage
    exit 0
    ;;
  *)
    usage
    exit 2
    ;;
esac

run_llvm_cov() {
  local args=(llvm-cov --fail-under-lines "$min_lines")

  case "$mode" in
    summary)
      args+=(--summary-only)
      ;;
    html)
      args+=(--html)
      ;;
    lcov)
      mkdir -p target/coverage
      args+=(--lcov --output-path target/coverage/lcov.info)
      ;;
  esac

  cargo "${args[@]}"
}

run_tarpaulin() {
  local args=(tarpaulin --fail-under "$min_lines")

  case "$mode" in
    summary)
      args+=(--print-summary)
      ;;
    html)
      args+=(--out Html)
      ;;
    lcov)
      mkdir -p target/coverage
      args+=(--out Lcov --output-dir target/coverage)
      ;;
  esac

  cargo "${args[@]}"
}

if cargo llvm-cov --version >/dev/null 2>&1; then
  set +e
  run_llvm_cov
  status=$?
  set -e
  if [ "$status" -eq 0 ]; then
    exit 0
  fi
  cat >&2 <<'HINT'

cargo llvm-cov failed. Confirm that llvm-cov and llvm-profdata match the active rustc.
For rustup toolchains, run:
  rustup component add llvm-tools-preview

If rustc comes from Homebrew or another distribution, set LLVM_COV and LLVM_PROFDATA
to the matching tool binaries before running this script.
HINT
  exit "$status"
fi

if cargo tarpaulin --version >/dev/null 2>&1; then
  run_tarpaulin
  exit 0
fi

cat >&2 <<'HINT'
No supported Rust coverage tool was found.
Install one of:
  cargo install cargo-llvm-cov
  cargo install cargo-tarpaulin
HINT
exit 1
