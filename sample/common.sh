#!/usr/bin/env bash
set -euo pipefail

SAMPLE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd "${SAMPLE_ROOT}/.." && pwd -P)"
OUT_ROOT="${SAMPLE_ROOT}/out"
WRAPPER_BIN="${REPO_ROOT}/target/debug/g++-formed"

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

print_cases() {
    cat <<'EOF'
cases:
  lambda_capture         missing lambda capture; good first demo on the default GCC 13 path
  template_instantiation template mismatch; script forces single_sink_structured
  ranges_views           C++20 ranges/views failure; script adds -std=c++20 and single_sink_structured
  duplicate_symbol       linker multiple-definition failure across two translation units
EOF
}

print_configs() {
    cat <<'EOF'
configs:
  default             subject_blocks_v2 + default profile
  concise             subject_blocks_v2 + concise profile
  verbose             subject_blocks_v2 + verbose profile
  subject_blocks_v1   previous beta default preset
  legacy_v1           legacy wording / session preset
  dedicated_location  custom overlay with dedicated location lines and fixed-width labels
EOF
}

resolve_case() {
    local case_id="$1"

    CASE_ID="$case_id"
    CASE_DESC=""
    CASE_WRAPPER_ARGS=()
    CASE_COMPILER_ARGS=()

    case "$case_id" in
        lambda_capture)
            CASE_DESC="missing lambda capture"
            CASE_COMPILER_ARGS=(-c "${SAMPLE_ROOT}/lambda_capture.cpp")
            ;;
        template_instantiation)
            CASE_DESC="template mismatch"
            CASE_WRAPPER_ARGS=(--formed-processing-path=single_sink_structured)
            CASE_COMPILER_ARGS=(-c "${SAMPLE_ROOT}/template_instantiation.cpp")
            ;;
        ranges_views)
            CASE_DESC="C++20 ranges/views failure"
            CASE_WRAPPER_ARGS=(--formed-processing-path=single_sink_structured)
            CASE_COMPILER_ARGS=(-std=c++20 -c "${SAMPLE_ROOT}/ranges_views.cpp")
            ;;
        duplicate_symbol)
            CASE_DESC="linker multiple definition"
            CASE_COMPILER_ARGS=("${SAMPLE_ROOT}/linker_main.cpp" "${SAMPLE_ROOT}/linker_helper.cpp")
            ;;
        *)
            die "unknown case '${case_id}' (use --list)"
            ;;
    esac
}

resolve_config() {
    local config_id="$1"

    case "$config_id" in
        default|concise|verbose|subject_blocks_v1|legacy_v1|dedicated_location)
            CONFIG_ID="$config_id"
            CONFIG_PATH="${SAMPLE_ROOT}/config/${config_id}.toml"
            ;;
        *)
            die "unknown config '${config_id}' (use --list-configs)"
            ;;
    esac

    [ -f "${CONFIG_PATH}" ] || die "config file not found: ${CONFIG_PATH}"
}

ensure_wrapper() {
    if [ ! -x "${WRAPPER_BIN}" ]; then
        (
            cd "${REPO_ROOT}"
            cargo build -q -p diag_cli_front --bin gcc-formed
        )
    fi

    if [ ! -e "${WRAPPER_BIN}" ] && [ -x "${REPO_ROOT}/target/debug/gcc-formed" ]; then
        (
            cd "${REPO_ROOT}/target/debug"
            ln -sf gcc-formed 'g++-formed'
        )
    fi

    [ -x "${WRAPPER_BIN}" ] || die "wrapper binary not found: ${WRAPPER_BIN}"
}

case_output_dir() {
    printf '%s/%s\n' "${OUT_ROOT}" "$1"
}

command_preview() {
    printf '%q ' "$@"
    printf '\n'
}

capture_stream() {
    local outfile="$1"
    shift

    set +e
    "$@" 2>&1 | tee "${outfile}"
    local status=${PIPESTATUS[0]}
    set -e
    return "${status}"
}

capture_quiet() {
    local outfile="$1"
    shift

    set +e
    "$@" >"${outfile}" 2>&1
    local status=$?
    set -e
    return "${status}"
}

print_raw_command() {
    local case_id="$1"
    shift

    resolve_case "${case_id}"
    printf 'command: '
    command_preview g++ "$@" "${CASE_COMPILER_ARGS[@]}"
}

print_formed_command() {
    local case_id="$1"
    local config_id="$2"
    local jsonfile="$3"
    shift 3

    resolve_case "${case_id}"
    resolve_config "${config_id}"
    ensure_wrapper

    printf 'command: FORMED_CONFIG_FILE=%q ' "${CONFIG_PATH}"
    command_preview "${WRAPPER_BIN}" "${CASE_WRAPPER_ARGS[@]}" "--formed-public-json=${jsonfile}" "$@" "${CASE_COMPILER_ARGS[@]}"
}

run_raw_stream() {
    local case_id="$1"
    local outfile="$2"
    shift 2

    resolve_case "${case_id}"
    mkdir -p "$(dirname "${outfile}")"

    (
        cd "${REPO_ROOT}"
        capture_stream "${outfile}" g++ "$@" "${CASE_COMPILER_ARGS[@]}"
    )
}

run_raw_quiet() {
    local case_id="$1"
    local outfile="$2"
    shift 2

    resolve_case "${case_id}"
    mkdir -p "$(dirname "${outfile}")"

    (
        cd "${REPO_ROOT}"
        capture_quiet "${outfile}" g++ "$@" "${CASE_COMPILER_ARGS[@]}"
    )
}

run_formed_stream() {
    local case_id="$1"
    local config_id="$2"
    local outfile="$3"
    local jsonfile="$4"
    shift 4

    resolve_case "${case_id}"
    resolve_config "${config_id}"
    ensure_wrapper
    mkdir -p "$(dirname "${outfile}")"
    rm -f "${jsonfile}"

    (
        cd "${REPO_ROOT}"
        capture_stream "${outfile}" env "FORMED_CONFIG_FILE=${CONFIG_PATH}" "${WRAPPER_BIN}" "${CASE_WRAPPER_ARGS[@]}" "--formed-public-json=${jsonfile}" "$@" "${CASE_COMPILER_ARGS[@]}"
    )
}

run_formed_quiet() {
    local case_id="$1"
    local config_id="$2"
    local outfile="$3"
    local jsonfile="$4"
    shift 4

    resolve_case "${case_id}"
    resolve_config "${config_id}"
    ensure_wrapper
    mkdir -p "$(dirname "${outfile}")"
    rm -f "${jsonfile}"

    (
        cd "${REPO_ROOT}"
        capture_quiet "${outfile}" env "FORMED_CONFIG_FILE=${CONFIG_PATH}" "${WRAPPER_BIN}" "${CASE_WRAPPER_ARGS[@]}" "--formed-public-json=${jsonfile}" "$@" "${CASE_COMPILER_ARGS[@]}"
    )
}
