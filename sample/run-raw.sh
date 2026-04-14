#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
# shellcheck source=sample/common.sh
source "${SCRIPT_DIR}/common.sh"

usage() {
    cat <<'EOF'
usage: ./sample/run-raw.sh <case> [-- extra g++ args]

examples:
  ./sample/run-raw.sh lambda_capture
  ./sample/run-raw.sh ranges_views -- -Wall
EOF
}

if [ $# -eq 0 ] || [ "${1:-}" = "--help" ]; then
    usage
    printf '\n'
    print_cases
    exit 0
fi

if [ "${1:-}" = "--list" ]; then
    print_cases
    exit 0
fi

CASE_ID="$1"
shift

if [ "${1:-}" = "--" ]; then
    shift
fi

EXTRA_ARGS=("$@")
resolve_case "${CASE_ID}"

OUT_DIR="$(case_output_dir "${CASE_ID}")"
OUT_FILE="${OUT_DIR}/raw.gpp.txt"

printf 'case: %s (%s)\n' "${CASE_ID}" "${CASE_DESC}"
printf 'saved terminal output: %s\n' "${OUT_FILE}"
print_raw_command "${CASE_ID}" "${EXTRA_ARGS[@]}"
printf '\n'

set +e
run_raw_stream "${CASE_ID}" "${OUT_FILE}" "${EXTRA_ARGS[@]}"
STATUS=$?
set -e

printf '\ng++ exit status: %s\n' "${STATUS}"
exit 0
