#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
# shellcheck source=sample/common.sh
source "${SCRIPT_DIR}/common.sh"

usage() {
    cat <<'EOF'
usage: ./sample/run-matrix.sh <case> [-- extra wrapper/compiler args]

This saves:
  - raw g++ output
  - formed output for default / concise / verbose / subject_blocks_v1 / legacy_v1 / dedicated_location
  - public JSON for each formed run
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
ensure_wrapper

OUT_DIR="$(case_output_dir "${CASE_ID}")"
printf 'case: %s (%s)\n' "${CASE_ID}" "${CASE_DESC}"
printf 'output directory: %s\n' "${OUT_DIR}"

RAW_FILE="${OUT_DIR}/raw.gpp.txt"
set +e
run_raw_quiet "${CASE_ID}" "${RAW_FILE}" "${EXTRA_ARGS[@]}"
RAW_STATUS=$?
set -e
printf '  raw.gpp.txt                 exit=%s\n' "${RAW_STATUS}"

for CONFIG_ID in default concise verbose subject_blocks_v1 legacy_v1 dedicated_location; do
    FORMED_FILE="${OUT_DIR}/formed.${CONFIG_ID}.txt"
    JSON_FILE="${OUT_DIR}/public.${CONFIG_ID}.json"
    set +e
    run_formed_quiet "${CASE_ID}" "${CONFIG_ID}" "${FORMED_FILE}" "${JSON_FILE}" "${EXTRA_ARGS[@]}"
    STATUS=$?
    set -e
    printf '  formed.%-20s exit=%s\n' "${CONFIG_ID}.txt" "${STATUS}"
done

printf '\ninspect files under %s\n' "${OUT_DIR}"
exit 0
