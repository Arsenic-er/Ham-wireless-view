#!/usr/bin/env bash
set -euo pipefail

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
readonly PLATFORM_SCRIPT="$SCRIPT_DIR/validation-platform.sh"
readonly BASE_URL="http://127.0.0.1:1421"
readonly RUNTIME_ROOT="$PROJECT_ROOT/.runtime/validation-platform"
readonly CURL_PATH="$(readlink -f -- "$(command -v curl)")"
readonly UNKNOWN_OPERATION_ID="00000000-0000-4000-8000-000000000000"

work_dir=""
operation_command_file=""
calculation_curl_pid=""
calculation_curl_start=""
calculation_operation_id=""
reserved_operation_id=""
issued_operation_id=""
observed_progress_sequence=""
terminal_sequence=""

process_start_time() {
  local pid=$1
  local stat_line=""
  local remainder=""
  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
  IFS= read -r stat_line <"/proc/$pid/stat" || return 1
  [[ "$stat_line" == *") "* ]] || return 1
  remainder="${stat_line##*) }"
  set -- $remainder
  (( $# >= 20 )) || return 1
  [[ "${20}" =~ ^[0-9]+$ ]] || return 1
  printf '%s\n' "${20}"
}

process_parent_pid() {
  local pid=$1
  local stat_line=""
  local remainder=""
  IFS= read -r stat_line <"/proc/$pid/stat" || return 1
  [[ "$stat_line" == *") "* ]] || return 1
  remainder="${stat_line##*) }"
  set -- $remainder
  [[ "${2:-}" =~ ^[1-9][0-9]*$ ]] || return 1
  printf '%s\n' "${2}"
}

is_uuid_v4() {
  local value=$1
  [[ "$value" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$ ]]
}

work_dir_is_safe() {
  local name=""
  [[ -n "${work_dir:-}" && -d "$work_dir" && ! -L "$work_dir" ]] || return 1
  [[ "$(dirname -- "$work_dir")" == "$RUNTIME_ROOT" ]] || return 1
  name="$(basename -- "$work_dir")"
  [[ "$name" == validation-recovery-smoke.* && "$name" != */* ]]
}

curl_job_is_owned_and_running() {
  local job_pid=""
  [[ -n "${calculation_curl_pid:-}" && -n "${calculation_curl_start:-}" ]] || return 1
  [[ "$(process_start_time "$calculation_curl_pid" 2>/dev/null)" == "$calculation_curl_start" ]] ||
    return 1
  [[ "$(process_parent_pid "$calculation_curl_pid" 2>/dev/null)" == "$BASHPID" ]] || return 1
  [[ "$(readlink -f -- "/proc/$calculation_curl_pid/exe" 2>/dev/null)" == "$CURL_PATH" ]] ||
    return 1
  while IFS= read -r job_pid; do
    [[ "$job_pid" == "$calculation_curl_pid" ]] && return 0
  done < <(jobs -pr)
  return 1
}

fail() {
  printf 'validation recovery smoke failed: %s\n' "$*" >&2
  exit 1
}

cleanup() {
  local exit_status=$?
  local cleanup_request=""
  local cleanup_id=""
  trap - EXIT

  if [[ -n "${calculation_curl_pid:-}" ]]; then
    if is_uuid_v4 "${calculation_operation_id:-}" && work_dir_is_safe; then
      cleanup_request="$work_dir/cleanup-operation.json"
      printf '{"operationId":"%s"}\n' "$calculation_operation_id" >"$cleanup_request"
      curl --silent --show-error --connect-timeout 2 --max-time 5 \
        --request POST --header 'Content-Type: application/json' \
        --data-binary "@$cleanup_request" \
        "$BASE_URL/api/cancel-calculation" >/dev/null 2>&1 || true
    fi
    if curl_job_is_owned_and_running; then
      kill -TERM "$calculation_curl_pid" 2>/dev/null || true
    fi
    set +e
    wait "$calculation_curl_pid" >/dev/null 2>&1
    set -e
    calculation_curl_pid=""
    calculation_curl_start=""
  fi

  if work_dir_is_safe; then
    for cleanup_id in "${calculation_operation_id:-}" "${reserved_operation_id:-}"; do
      if is_uuid_v4 "$cleanup_id"; then
        cleanup_request="$work_dir/cleanup-operation.json"
        printf '{"operationId":"%s"}\n' "$cleanup_id" >"$cleanup_request"
        curl --silent --show-error --connect-timeout 2 --max-time 5 \
          --request POST --header 'Content-Type: application/json' \
          --data-binary "@$cleanup_request" \
          "$BASE_URL/api/operation-ack" >/dev/null 2>&1 || true
      fi
    done
  fi
  calculation_operation_id=""
  reserved_operation_id=""

  if [[ -n "${work_dir:-}" ]]; then
    case "$work_dir" in
      "$RUNTIME_ROOT"/validation-recovery-smoke.*)
        if [[ -L "$work_dir" || -e "$work_dir" && ! -d "$work_dir" ]]; then
          printf 'refusing to clean unsafe smoke workspace: %s\n' "$work_dir" >&2
          exit_status=1
        elif [[ -d "$work_dir" ]]; then
          if ! find "$work_dir" -depth -mindepth 1 -delete 2>/dev/null; then
            printf 'could not clean smoke workspace contents: %s\n' "$work_dir" >&2
            exit_status=1
          fi
          if [[ -d "$work_dir" ]] && ! rmdir -- "$work_dir" 2>/dev/null; then
            printf 'could not remove smoke workspace: %s\n' "$work_dir" >&2
            exit_status=1
          fi
        fi
        ;;
      *)
        printf 'refusing to clean unexpected smoke directory: %s\n' "$work_dir" >&2
        exit_status=1
        ;;
    esac
  fi

  exit "$exit_status"
}

trap cleanup EXIT

read_status() {
  local status_file=$1
  local status=""
  if [[ -f "$status_file" ]]; then
    IFS= read -r status <"$status_file" || true
  fi
  printf '%s' "$status"
}

require_status() {
  local status_file=$1
  local expected=$2
  local label=$3
  local actual=""
  actual="$(read_status "$status_file")"
  [[ "$actual" == "$expected" ]] ||
    fail "$label returned HTTP ${actual:-<empty>}, expected $expected"
}

require_contains() {
  local file=$1
  local pattern=$2
  local label=$3
  grep -Eq -- "$pattern" "$file" || fail "$label did not match the expected response"
}

require_absent() {
  local file=$1
  local literal=$2
  local label=$3
  if grep -Fq -- "$literal" "$file"; then
    fail "$label unexpectedly contained $literal"
  fi
}

extract_json_string() {
  local file=$1
  local key=$2
  local match=""
  match="$(
    grep -Eo -- "\"$key\"[[:space:]]*:[[:space:]]*\"[^\"]*\"" "$file" |
      head -n 1 || true
  )"
  [[ -n "$match" ]] || return 1
  printf '%s\n' "$match" | sed -E 's/^[^:]+:[[:space:]]*"([^"]*)"$/\1/'
}

extract_json_integer() {
  local file=$1
  local key=$2
  local match=""
  match="$(
    grep -Eo -- "\"$key\"[[:space:]]*:[[:space:]]*[0-9]+" "$file" |
      head -n 1 || true
  )"
  [[ -n "$match" ]] || return 1
  printf '%s\n' "$match" | sed -E 's/^.*:[[:space:]]*([0-9]+)$/\1/'
}

post_operation_id() {
  local endpoint=$1
  local operation_id=$2
  local response_file=$3
  local status_file=$4
  is_uuid_v4 "$operation_id" || fail "$endpoint received an unsafe operation id"
  printf '{"operationId":"%s"}\n' "$operation_id" >"$operation_command_file"
  curl --silent --show-error --connect-timeout 5 --max-time 10 \
    --request POST --header 'Content-Type: application/json' \
    --data-binary "@$operation_command_file" \
    --output "$response_file" --write-out '%{http_code}' \
    "$BASE_URL$endpoint" >"$status_file"
}

issue_calculation_ticket() {
  local response_file=$1
  local status_file=$2
  local operation_id=""
  curl --silent --show-error --connect-timeout 5 --max-time 10 \
    --request POST --header 'Content-Type: application/json' \
    --data-binary '{"kind":"calculation"}' \
    --output "$response_file" --write-out '%{http_code}' \
    "$BASE_URL/api/operation-ticket" >"$status_file"
  require_status "$status_file" "200" "calculation ticket"
  require_contains "$response_file" '"schemaVersion"[[:space:]]*:[[:space:]]*1([,}])' \
    "calculation ticket schema"
  require_contains "$response_file" '"kind"[[:space:]]*:[[:space:]]*"calculation"' \
    "calculation ticket kind"
  require_contains "$response_file" '"state"[[:space:]]*:[[:space:]]*"reserved"' \
    "calculation ticket state"
  operation_id="$(extract_json_string "$response_file" "operationId")" ||
    fail "calculation ticket did not contain operationId"
  is_uuid_v4 "$operation_id" || fail "server did not issue a canonical lowercase UUIDv4"
  issued_operation_id="$operation_id"
}

write_calculation_request() {
  local operation_id=$1
  local request_file=$2
  is_uuid_v4 "$operation_id" || fail "cannot write calculation request with an unsafe id"
  printf '%s\n' \
    "{\"operationId\":\"$operation_id\",\"request\":{\"center\":{\"lat\":30.5,\"lon\":103.5},\"band\":\"vhf-144\",\"frequencyMhz\":145.0,\"powerValue\":25.0,\"powerUnit\":\"watt\",\"txGainValue\":6.0,\"txGainUnit\":\"dbi\",\"txHeightM\":20.0,\"txGroundElevationOverrideM\":null,\"rxGainValue\":-3.0,\"rxGainUnit\":\"dbi\",\"rxHeightM\":1.5,\"polarization\":\"vertical\"}}" \
    >"$request_file"
}

start_calculation() {
  local operation_id=$1
  local request_file=$2
  local response_file=$3
  local status_file=$4
  [[ -z "${calculation_curl_pid:-}" ]] || fail "a calculation curl is already tracked"
  write_calculation_request "$operation_id" "$request_file"
  calculation_operation_id="$operation_id"
  curl --silent --show-error --connect-timeout 5 --max-time 180 \
    --header 'Content-Type: application/json' \
    --data-binary "@$request_file" \
    --output "$response_file" --write-out '%{http_code}' \
    "$BASE_URL/api/calculate" >"$status_file" &
  calculation_curl_pid=$!
  calculation_curl_start="$(process_start_time "$calculation_curl_pid")" ||
    fail "could not identify the calculation curl child"
}

wait_for_calculation_progress() {
  local operation_id=$1
  local response_file=$2
  local status_file=$3
  local calculation_status_file=$4
  local label=$5
  local state=""
  local sequence=""
  local response_operation_id=""
  observed_progress_sequence=""

  for _ in $(seq 1 200); do
    post_operation_id "/api/operation-status" "$operation_id" "$response_file" "$status_file"
    require_status "$status_file" "200" "$label status"
    response_operation_id="$(extract_json_string "$response_file" "operationId")" ||
      fail "$label status omitted operationId"
    [[ "$response_operation_id" == "$operation_id" ]] ||
      fail "$label status returned another operation id"
    require_contains "$response_file" '"kind"[[:space:]]*:[[:space:]]*"calculation"' \
      "$label status kind"
    state="$(extract_json_string "$response_file" "state")" ||
      fail "$label status omitted state"
    sequence="$(extract_json_integer "$response_file" "sequence")" ||
      fail "$label status omitted sequence"
    if [[ "$state" == "running" ]] &&
      (( sequence >= 2 )) &&
      grep -Eq -- '"progress"[[:space:]]*:[[:space:]]*\{' "$response_file" &&
      grep -Eq -- '"type"[[:space:]]*:[[:space:]]*"calculation"' "$response_file" &&
      grep -Eq -- '"phase"[[:space:]]*:[[:space:]]*"(loading-data|computing|encoding|complete)"' "$response_file" &&
      grep -Eq -- '"percent"[[:space:]]*:[[:space:]]*[0-9]+([.][0-9]+)?' "$response_file"; then
      curl_job_is_owned_and_running ||
        fail "$label progress appeared after its curl stopped being our child job"
      [[ ! -s "$calculation_status_file" ]] ||
        fail "$label calculation completed before progress ownership was confirmed"
      observed_progress_sequence="$sequence"
      return
    fi
    [[ "$state" == "reserved" || "$state" == "running" ]] ||
      fail "$label reached unexpected state $state before progress"
    [[ ! -s "$calculation_status_file" ]] ||
      fail "$label calculation completed before real progress was observed"
    sleep 0.05
  done
  fail "$label did not expose real calculation progress"
}

require_terminal_status() {
  local operation_id=$1
  local expected_state=$2
  local minimum_sequence=$3
  local response_file=$4
  local status_file=$5
  local label=$6
  local response_operation_id=""
  local state=""
  local sequence=""

  post_operation_id "/api/operation-status" "$operation_id" "$response_file" "$status_file"
  require_status "$status_file" "200" "$label status"
  response_operation_id="$(extract_json_string "$response_file" "operationId")" ||
    fail "$label status omitted operationId"
  [[ "$response_operation_id" == "$operation_id" ]] ||
    fail "$label status returned another operation id"
  require_contains "$response_file" '"kind"[[:space:]]*:[[:space:]]*"calculation"' \
    "$label status kind"
  state="$(extract_json_string "$response_file" "state")" ||
    fail "$label status omitted state"
  [[ "$state" == "$expected_state" ]] ||
    fail "$label state was $state, expected $expected_state"
  sequence="$(extract_json_integer "$response_file" "sequence")" ||
    fail "$label status omitted sequence"
  (( sequence > minimum_sequence )) ||
    fail "$label terminal sequence $sequence did not advance beyond $minimum_sequence"
  require_absent "$response_file" '"heatmapPngDataUrl"' "$label status"
  require_absent "$response_file" '"mapOverlayPngDataUrl"' "$label status"
  require_absent "$response_file" 'data:image/png' "$label status"
  terminal_sequence="$sequence"
}

cancel_operation() {
  local operation_id=$1
  local expected=$2
  local response_file=$3
  local status_file=$4
  local label=$5
  post_operation_id "/api/cancel-calculation" "$operation_id" "$response_file" "$status_file"
  require_status "$status_file" "200" "$label"
  require_contains "$response_file" \
    "\"cancelled\"[[:space:]]*:[[:space:]]*$expected([,}])" "$label"
}

cancel_download_operation() {
  local operation_id=$1
  local expected=$2
  local response_file=$3
  local status_file=$4
  local label=$5
  post_operation_id "/api/cancel-download" "$operation_id" "$response_file" "$status_file"
  require_status "$status_file" "200" "$label"
  require_contains "$response_file" \
    "\"cancelled\"[[:space:]]*:[[:space:]]*$expected([,}])" "$label"
}

require_reserved_status() {
  local operation_id=$1
  local response_file=$2
  local status_file=$3
  local label=$4
  local response_operation_id=""
  local state=""
  local sequence=""

  post_operation_id "/api/operation-status" "$operation_id" "$response_file" "$status_file"
  require_status "$status_file" "200" "$label"
  response_operation_id="$(extract_json_string "$response_file" "operationId")" ||
    fail "$label omitted operationId"
  [[ "$response_operation_id" == "$operation_id" ]] ||
    fail "$label returned another operation id"
  require_contains "$response_file" '"kind"[[:space:]]*:[[:space:]]*"calculation"' \
    "$label kind"
  state="$(extract_json_string "$response_file" "state")" ||
    fail "$label omitted state"
  [[ "$state" == "reserved" ]] || fail "$label state was $state, expected reserved"
  sequence="$(extract_json_integer "$response_file" "sequence")" ||
    fail "$label omitted sequence"
  [[ "$sequence" == "0" ]] || fail "$label sequence was $sequence, expected 0"
  require_contains "$response_file" '"progress"[[:space:]]*:[[:space:]]*null' "$label progress"
}

ack_operation() {
  local operation_id=$1
  local expected=$2
  local response_file=$3
  local status_file=$4
  local label=$5
  post_operation_id "/api/operation-ack" "$operation_id" "$response_file" "$status_file"
  require_status "$status_file" "200" "$label"
  require_contains "$response_file" \
    "\"acknowledged\"[[:space:]]*:[[:space:]]*$expected([,}])" "$label"
}

require_status_not_found() {
  local operation_id=$1
  local response_file=$2
  local status_file=$3
  local label=$4
  post_operation_id "/api/operation-status" "$operation_id" "$response_file" "$status_file"
  require_status "$status_file" "404" "$label"
  require_contains "$response_file" '"message"[[:space:]]*:[[:space:]]*"operation not found"' \
    "$label"
}

wait_for_tracked_calculation() {
  local label=$1
  local curl_status=0
  set +e
  wait "$calculation_curl_pid"
  curl_status=$?
  set -e
  calculation_curl_pid=""
  calculation_curl_start=""
  [[ "$curl_status" -eq 0 ]] || fail "$label curl exited with status $curl_status"
}

validate_png_field() {
  local response_file=$1
  local key=$2
  local match_file=$3
  local output_file=$4
  local match_count=""
  local header=""

  if ! grep -Eo -- "\"$key\":\"data:image/png;base64,[A-Za-z0-9+/=]+\"" \
    "$response_file" >"$match_file"; then
    fail "$key is missing or has an empty/invalid base64 payload"
  fi
  match_count="$(wc -l <"$match_file" | tr -d ' ')"
  [[ "$match_count" == "1" ]] || fail "$key occurred $match_count times"
  if ! cut -d, -f2- "$match_file" | tr -d '"' | base64 --decode >"$output_file"; then
    fail "$key did not decode as base64"
  fi
  header="$(od -An -tx1 -N24 "$output_file" | tr -d ' \n')"
  [[ "$header" == "89504e470d0a1a0a0000000d494844520000019100000191" ]] ||
    fail "$key is not a non-empty 401x401 PNG"
}

[[ -x "$PLATFORM_SCRIPT" ]] || fail "validation platform manager is unavailable"
[[ -d "$RUNTIME_ROOT" && ! -L "$RUNTIME_ROOT" ]] ||
  fail "managed validation runtime directory is missing or is a symlink"

"$PLATFORM_SCRIPT" status >/dev/null ||
  fail "managed validation platform is not running with its expected identity"

work_dir="$(mktemp -d -- "$RUNTIME_ROOT/validation-recovery-smoke.XXXXXXXX")"
[[ -d "$work_dir" && ! -L "$work_dir" ]] || fail "could not create safe runtime workspace"
operation_command_file="$work_dir/operation-command.json"

readonly ticket_a_body="$work_dir/ticket-a.json"
readonly ticket_a_status="$work_dir/ticket-a.status"
readonly request_a="$work_dir/calculate-a.json"
readonly calculation_a_body="$work_dir/calculation-a.json"
readonly calculation_a_status="$work_dir/calculation-a.status"
readonly progress_a_body="$work_dir/progress-a.json"
readonly progress_a_status="$work_dir/progress-a.status"
readonly cancel_unknown_body="$work_dir/cancel-unknown.json"
readonly cancel_unknown_status="$work_dir/cancel-unknown.status"
readonly cancel_wrong_family_body="$work_dir/cancel-wrong-family.json"
readonly cancel_wrong_family_status="$work_dir/cancel-wrong-family.status"
readonly cancel_a_body="$work_dir/cancel-a.json"
readonly cancel_a_status="$work_dir/cancel-a.status"
readonly terminal_a_body="$work_dir/terminal-a.json"
readonly terminal_a_status="$work_dir/terminal-a.status"
readonly ack_a_body="$work_dir/ack-a.json"
readonly ack_a_status="$work_dir/ack-a.status"
readonly ack_a_again_body="$work_dir/ack-a-again.json"
readonly ack_a_again_status="$work_dir/ack-a-again.status"
readonly missing_a_body="$work_dir/missing-a.json"
readonly missing_a_status="$work_dir/missing-a.status"
readonly ticket_b_body="$work_dir/ticket-b.json"
readonly ticket_b_status="$work_dir/ticket-b.status"
readonly busy_b_request="$work_dir/calculate-b-busy.json"
readonly busy_b_body="$work_dir/calculation-b-busy.json"
readonly busy_b_status="$work_dir/calculation-b-busy.status"
readonly reserved_b_body="$work_dir/reserved-b.json"
readonly reserved_b_status="$work_dir/reserved-b.status"
readonly request_b="$work_dir/calculate-b.json"
readonly calculation_b_body="$work_dir/calculation-b.json"
readonly calculation_b_status="$work_dir/calculation-b.status"
readonly progress_b_body="$work_dir/progress-b.json"
readonly progress_b_status="$work_dir/progress-b.status"
readonly old_cancel_body="$work_dir/old-cancel.json"
readonly old_cancel_status="$work_dir/old-cancel.status"
readonly terminal_b_body="$work_dir/terminal-b.json"
readonly terminal_b_status="$work_dir/terminal-b.status"
readonly ack_b_body="$work_dir/ack-b.json"
readonly ack_b_status="$work_dir/ack-b.status"
readonly missing_b_body="$work_dir/missing-b.json"
readonly missing_b_status="$work_dir/missing-b.status"
readonly probe_body="$work_dir/gate-probe.json"
readonly final_probe_body="$work_dir/final-gate-probe.json"
readonly health_body="$work_dir/health.json"
readonly heatmap_match="$work_dir/heatmap-field.txt"
readonly heatmap_png="$work_dir/heatmap.png"
readonly overlay_match="$work_dir/overlay-field.txt"
readonly overlay_png="$work_dir/overlay.png"

health_status="$(
  curl --silent --show-error --connect-timeout 5 --max-time 10 \
    --output "$health_body" --write-out '%{http_code}' \
    "$BASE_URL/healthz"
)"
[[ "$health_status" == "200" ]] || fail "health check returned HTTP $health_status"
require_contains "$health_body" '"status"[[:space:]]*:[[:space:]]*"ok"' "health check"

preflight_status="$(
  curl --silent --show-error --connect-timeout 5 --max-time 10 \
    --header 'Content-Type: application/json' \
    --data-binary '{"point":{"lat":91,"lon":0}}' \
    --output "$probe_body" --write-out '%{http_code}' \
    "$BASE_URL/api/inspect-point"
)"
[[ "$preflight_status" == "422" ]] ||
  fail "operation gate was not idle before the smoke run (HTTP $preflight_status)"

issue_calculation_ticket "$ticket_a_body" "$ticket_a_status"
operation_a="$issued_operation_id"
[[ "$operation_a" != "$UNKNOWN_OPERATION_ID" ]] ||
  fail "ticket A collided with the reserved unknown-operation fixture"

start_calculation "$operation_a" "$request_a" "$calculation_a_body" "$calculation_a_status"
wait_for_calculation_progress "$operation_a" "$progress_a_body" "$progress_a_status" \
  "$calculation_a_status" "ticket A"
progress_a_sequence="$observed_progress_sequence"

cancel_operation "$UNKNOWN_OPERATION_ID" "false" \
  "$cancel_unknown_body" "$cancel_unknown_status" "unknown-id cancellation"
cancel_download_operation "$operation_a" "false" \
  "$cancel_wrong_family_body" "$cancel_wrong_family_status" \
  "ticket A wrong-family cancellation"
curl_job_is_owned_and_running ||
  fail "ticket A stopped before ticket B could exercise the busy gate"

issue_calculation_ticket "$ticket_b_body" "$ticket_b_status"
operation_b="$issued_operation_id"
reserved_operation_id="$operation_b"
[[ "$operation_b" != "$operation_a" ]] || fail "ticket B reused ticket A operation id"
[[ "$operation_b" != "$UNKNOWN_OPERATION_ID" ]] ||
  fail "ticket B collided with the reserved unknown-operation fixture"

write_calculation_request "$operation_b" "$busy_b_request"
curl --silent --show-error --connect-timeout 5 --max-time 10 \
  --header 'Content-Type: application/json' \
  --data-binary "@$busy_b_request" \
  --output "$busy_b_body" --write-out '%{http_code}' \
  "$BASE_URL/api/calculate" >"$busy_b_status"
require_status "$busy_b_status" "409" "ticket B busy calculation"
require_contains "$busy_b_body" 'another validation operation is already running' \
  "ticket B busy calculation"
require_absent "$busy_b_body" '"heatmapPngDataUrl"' "ticket B busy calculation"
require_absent "$busy_b_body" '"mapOverlayPngDataUrl"' "ticket B busy calculation"
require_reserved_status "$operation_b" "$reserved_b_body" "$reserved_b_status" \
  "ticket B status after busy rejection"

curl_job_is_owned_and_running ||
  fail "ticket A stopped before its exact cancellation"
cancel_operation "$operation_a" "true" "$cancel_a_body" "$cancel_a_status" \
  "ticket A cancellation"

wait_for_tracked_calculation "cancelled calculation"
require_status "$calculation_a_status" "422" "cancelled calculation"
require_contains "$calculation_a_body" '[Cc]ancel' "cancelled calculation"
require_absent "$calculation_a_body" '"heatmapPngDataUrl"' "cancelled calculation"
require_absent "$calculation_a_body" '"mapOverlayPngDataUrl"' "cancelled calculation"

require_terminal_status "$operation_a" "cancelled" "$progress_a_sequence" \
  "$terminal_a_body" "$terminal_a_status" "ticket A"
ack_operation "$operation_a" "true" "$ack_a_body" "$ack_a_status" "ticket A acknowledgement"
ack_operation "$operation_a" "false" "$ack_a_again_body" "$ack_a_again_status" \
  "ticket A repeated acknowledgement"
require_status_not_found "$operation_a" "$missing_a_body" "$missing_a_status" \
  "ticket A status after acknowledgement"
calculation_operation_id=""

require_reserved_status "$operation_b" "$reserved_b_body" "$reserved_b_status" \
  "ticket B status before reuse"
start_calculation "$operation_b" "$request_b" "$calculation_b_body" "$calculation_b_status"
wait_for_calculation_progress "$operation_b" "$progress_b_body" "$progress_b_status" \
  "$calculation_b_status" "ticket B"
progress_b_sequence="$observed_progress_sequence"

cancel_operation "$operation_a" "false" "$old_cancel_body" "$old_cancel_status" \
  "acknowledged ticket A cancellation during ticket B"
curl_job_is_owned_and_running ||
  fail "ticket B stopped immediately after the old ticket A cancellation"

wait_for_tracked_calculation "recovery calculation"
require_status "$calculation_b_status" "200" "recovery calculation"
require_contains "$calculation_b_body" '"schemaVersion"[[:space:]]*:[[:space:]]*3([,}])' "schema"
require_contains "$calculation_b_body" '"txGroundElevationSource"[[:space:]]*:[[:space:]]*"dem"' "transmitter ground elevation source"
require_contains "$calculation_b_body" '"txGroundElevationM"[[:space:]]*:[[:space:]]*-?[0-9]+([.][0-9]+)?([eE][+-]?[0-9]+)?([,}])' "finite transmitter ground elevation"
require_contains "$calculation_b_body" '"imageWidth"[[:space:]]*:[[:space:]]*401([,}])' "image width"
require_contains "$calculation_b_body" '"imageHeight"[[:space:]]*:[[:space:]]*401([,}])' "image height"
require_contains "$calculation_b_body" '"mapOverlayWidth"[[:space:]]*:[[:space:]]*401([,}])' \
  "overlay width"
require_contains "$calculation_b_body" '"mapOverlayHeight"[[:space:]]*:[[:space:]]*401([,}])' \
  "overlay height"
validate_png_field "$calculation_b_body" "heatmapPngDataUrl" "$heatmap_match" "$heatmap_png"
validate_png_field "$calculation_b_body" "mapOverlayPngDataUrl" "$overlay_match" "$overlay_png"

require_terminal_status "$operation_b" "succeeded" "$progress_b_sequence" \
  "$terminal_b_body" "$terminal_b_status" "ticket B"
ack_operation "$operation_b" "true" "$ack_b_body" "$ack_b_status" "ticket B acknowledgement"
require_status_not_found "$operation_b" "$missing_b_body" "$missing_b_status" \
  "ticket B status after acknowledgement"
calculation_operation_id=""
reserved_operation_id=""

final_probe_status="$(
  curl --silent --show-error --connect-timeout 5 --max-time 10 \
    --header 'Content-Type: application/json' \
    --data-binary '{"point":{"lat":91,"lon":0}}' \
    --output "$final_probe_body" --write-out '%{http_code}' \
    "$BASE_URL/api/inspect-point"
)"
[[ "$final_probe_status" == "422" ]] ||
  fail "operation gate remained occupied after recovery calculation"

health_status="$(
  curl --silent --show-error --connect-timeout 5 --max-time 10 \
    --output "$health_body" --write-out '%{http_code}' \
    "$BASE_URL/healthz"
)"
[[ "$health_status" == "200" ]] || fail "final health check returned HTTP $health_status"
require_contains "$health_body" '"status"[[:space:]]*:[[:space:]]*"ok"' "final health check"

printf 'validation recovery smoke passed: ticket_a_cancelled=true ticket_b_http=200 progress_a=%s progress_b=%s\n' \
  "$progress_a_sequence" "$progress_b_sequence"
