#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/coverage.sh [summary|html|lcov]

Environment:
  COVERAGE_MIN_LINES  Minimum line coverage percentage. Default: 90.
  COVERAGE_IGNORE_REGEX
                      Optional cargo-llvm-cov filename regex to exclude files
                      from the measured scope.
  LLVM_COV            Path to llvm-cov matching the active rustc, if needed.
  LLVM_PROFDATA       Path to llvm-profdata matching the active rustc, if needed.
USAGE
}

mode="${1:-summary}"
min_lines="${COVERAGE_MIN_LINES:-90}"

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
  local args=(llvm-cov)
  if [ -n "${COVERAGE_IGNORE_REGEX:-}" ]; then
    args+=(--ignore-filename-regex "$COVERAGE_IGNORE_REGEX")
  fi

  case "$mode" in
    summary)
      mkdir -p target/coverage
      args+=(--json --summary-only --output-path target/coverage/summary.json)
      ;;
    html)
      args+=(--html --fail-under-lines "$min_lines")
      ;;
    lcov)
      mkdir -p target/coverage
      args+=(--lcov --output-path target/coverage/lcov.info --fail-under-lines "$min_lines")
      ;;
  esac

  cargo "${args[@]}" || return $?
  if [ "$mode" = "summary" ]; then
    scripts/check_coverage_json.py target/coverage/summary.json "$min_lines"
  fi
}

resolve_rustup_llvm_tools() {
  local host
  local sysroot
  local tools_dir
  local llvm_version
  local llvm_major
  local candidate_dir
  local brew_prefix

  host="$(rustc -vV | sed -n 's/^host: //p')"
  sysroot="$(rustc --print sysroot)"
  tools_dir="${sysroot}/lib/rustlib/${host}/bin"

  if [ -z "${LLVM_COV:-}" ] && [ -x "${tools_dir}/llvm-cov" ]; then
    export LLVM_COV="${tools_dir}/llvm-cov"
  fi
  if [ -z "${LLVM_PROFDATA:-}" ] && [ -x "${tools_dir}/llvm-profdata" ]; then
    export LLVM_PROFDATA="${tools_dir}/llvm-profdata"
  fi
  if [ -n "${LLVM_COV:-}" ] && [ -n "${LLVM_PROFDATA:-}" ]; then
    export PATH="${tools_dir}:${PATH}"
    echo "Using LLVM_COV=${LLVM_COV}" >&2
    echo "Using LLVM_PROFDATA=${LLVM_PROFDATA}" >&2
    return
  fi

  llvm_version="$(rustc -vV | sed -n 's/^LLVM version: //p')"
  llvm_major="${llvm_version%%.*}"
  if [ -n "$llvm_major" ]; then
    for candidate_dir in \
      "/opt/homebrew/opt/llvm@${llvm_major}/bin" \
      "/usr/local/opt/llvm@${llvm_major}/bin" \
      "/opt/homebrew/opt/llvm/bin" \
      "/usr/local/opt/llvm/bin"; do
      if [ -x "${candidate_dir}/llvm-cov" ] && [ -x "${candidate_dir}/llvm-profdata" ]; then
        export LLVM_COV="${LLVM_COV:-${candidate_dir}/llvm-cov}"
        export LLVM_PROFDATA="${LLVM_PROFDATA:-${candidate_dir}/llvm-profdata}"
        export PATH="${candidate_dir}:${PATH}"
        echo "Using LLVM_COV=${LLVM_COV}" >&2
        echo "Using LLVM_PROFDATA=${LLVM_PROFDATA}" >&2
        return
      fi
    done

    if command -v brew >/dev/null 2>&1; then
      brew_prefix="$(brew --prefix "llvm@${llvm_major}" 2>/dev/null || true)"
      if [ -n "$brew_prefix" ] \
        && [ -x "${brew_prefix}/bin/llvm-cov" ] \
        && [ -x "${brew_prefix}/bin/llvm-profdata" ]; then
        export LLVM_COV="${LLVM_COV:-${brew_prefix}/bin/llvm-cov}"
        export LLVM_PROFDATA="${LLVM_PROFDATA:-${brew_prefix}/bin/llvm-profdata}"
        export PATH="${brew_prefix}/bin:${PATH}"
        echo "Using LLVM_COV=${LLVM_COV}" >&2
        echo "Using LLVM_PROFDATA=${LLVM_PROFDATA}" >&2
      fi
    fi
  fi
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
  resolve_rustup_llvm_tools
  log_file="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/cargo-llvm-cov.log"
  set +e
  run_llvm_cov 2>&1 | tee "$log_file"
  status=${PIPESTATUS[0]}
  set -e
  if [ "$status" -eq 0 ]; then
    exit 0
  fi
  cat >&2 <<'HINT'

cargo llvm-cov failed. See the cargo-llvm-cov output above for the root cause.
If the error mentions llvm-tools-preview, install matching LLVM tools:
  rustup component add llvm-tools-preview

If rustc comes from Homebrew or another distribution, set LLVM_COV and LLVM_PROFDATA
to the matching tool binaries before running this script.
HINT
  if [ -f "$log_file" ]; then
    echo >&2
    echo "Last cargo-llvm-cov output lines:" >&2
    tail -n 120 "$log_file" >&2
  fi
  exit "$status"
fi

if cargo tarpaulin --version >/dev/null 2>&1; then
  if [ -n "${COVERAGE_IGNORE_REGEX:-}" ]; then
    echo "COVERAGE_IGNORE_REGEX is only supported by cargo llvm-cov." >&2
    exit 2
  fi
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
