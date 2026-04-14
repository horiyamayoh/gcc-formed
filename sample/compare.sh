#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
# shellcheck source=sample/common.sh
source "${SCRIPT_DIR}/common.sh"

usage() {
    cat <<'EOF'
usage: ./sample/compare.sh <case> [config] [-- extra wrapper/compiler args]

examples:
  ./sample/compare.sh lambda_capture
  ./sample/compare.sh duplicate_symbol legacy_v1
EOF
}

if [ $# -eq 0 ] || [ "${1:-}" = "--help" ]; then
    usage
    printf '\n'
    print_cases
    printf '\n'
    print_configs
    exit 0
fi

case "${1:-}" in
    --list|--list-cases)
        print_cases
        exit 0
        ;;
    --list-configs)
        print_configs
        exit 0
        ;;
esac

CASE_ID="$1"
shift

CONFIG_ID="default"
if [ $# -gt 0 ] && [ "${1}" != "--" ]; then
    CONFIG_ID="$1"
    shift
fi

if [ "${1:-}" = "--" ]; then
    shift
fi

EXTRA_ARGS=("$@")
resolve_case "${CASE_ID}"
resolve_config "${CONFIG_ID}"

OUT_DIR="$(case_output_dir "${CASE_ID}")"
RAW_FILE="${OUT_DIR}/raw.gpp.txt"
FORMED_FILE="${OUT_DIR}/formed.${CONFIG_ID}.txt"
JSON_FILE="${OUT_DIR}/public.${CONFIG_ID}.json"

printf 'case: %s (%s)\n' "${CASE_ID}" "${CASE_DESC}"
printf 'config: %s (%s)\n' "${CONFIG_ID}" "${CONFIG_PATH}"
printf 'saved raw output   : %s\n' "${RAW_FILE}"
printf 'saved formed output: %s\n' "${FORMED_FILE}"
printf 'saved public JSON  : %s\n' "${JSON_FILE}"

printf '\n== raw g++ ==\n'
print_raw_command "${CASE_ID}" "${EXTRA_ARGS[@]}"
set +e
run_raw_stream "${CASE_ID}" "${RAW_FILE}" "${EXTRA_ARGS[@]}"
RAW_STATUS=$?
set -e

printf '\n== g++-formed ==\n'
print_formed_command "${CASE_ID}" "${CONFIG_ID}" "${JSON_FILE}" "${EXTRA_ARGS[@]}"
set +e
run_formed_stream "${CASE_ID}" "${CONFIG_ID}" "${FORMED_FILE}" "${JSON_FILE}" "${EXTRA_ARGS[@]}"
FORMED_STATUS=$?
set -e

printf '\nraw exit status: %s\n' "${RAW_STATUS}"
printf 'formed exit status: %s\n' "${FORMED_STATUS}"
exit 0
