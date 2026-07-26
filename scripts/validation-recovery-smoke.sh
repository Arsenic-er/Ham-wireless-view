#!/usr/bin/env bash
set -euo pipefail

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
readonly PLATFORM_SCRIPT="$SCRIPT_DIR/validation-platform.sh"
readonly BASE_URL="http://127.0.0.1:1421"
readonly RUNTIME_ROOT="$PROJECT_ROOT/.runtime/validation-platform"
readonly CURL_PATH="$(readlink -f -- "$(command -v curl)")"

work_dir=""
calculation_curl_pid=""
calculation_curl_start=""
calculation_operation_owned=false

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
  trap - EXIT

  if [[ -n "${calculation_curl_pid:-}" ]]; then
    if curl_job_is_owned_and_running; then
      if [[ "${calculation_operation_owned:-false}" == "true" ]]; then
        curl --silent --show-error --connect-timeout 2 --max-time 5 \
          --request POST --header 'Content-Type: application/json' \
          "$BASE_URL/api/cancel-calculation" >/dev/null 2>&1 || true
      fi
      kill -TERM "$calculation_curl_pid" 2>/dev/null || true
    fi
    set +e
    wait "$calculation_curl_pid" >/dev/null 2>&1
    set -e
    calculation_curl_pid=""
    calculation_curl_start=""
    calculation_operation_owned=false
  fi

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
  local actual
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

# status validates the managed executable, fixed loopback bind address, dist path,
# data path, PID identity, and process start time without starting or stopping it.
"$PLATFORM_SCRIPT" status >/dev/null ||
  fail "managed validation platform is not running with its expected identity"

work_dir="$(mktemp -d -- "$RUNTIME_ROOT/validation-recovery-smoke.XXXXXXXX")"
[[ -d "$work_dir" && ! -L "$work_dir" ]] || fail "could not create safe runtime workspace"

readonly request_file="$work_dir/calculate-request.json"
readonly cancelled_body="$work_dir/cancelled-calculation.json"
readonly cancelled_status="$work_dir/cancelled-calculation.status"
readonly probe_body="$work_dir/gate-probe.json"
readonly cancel_body="$work_dir/cancel-response.json"
readonly cancel_status="$work_dir/cancel-response.status"
readonly success_body="$work_dir/successful-calculation.json"
readonly success_status="$work_dir/successful-calculation.status"
readonly final_probe_body="$work_dir/final-gate-probe.json"
readonly health_body="$work_dir/health.json"
readonly heatmap_match="$work_dir/heatmap-field.txt"
readonly heatmap_png="$work_dir/heatmap.png"
readonly overlay_match="$work_dir/overlay-field.txt"
readonly overlay_png="$work_dir/overlay.png"

printf '%s\n' '{"request":{"center":{"lat":30.5,"lon":103.5},"band":"vhf-144","frequencyMhz":145.0,"powerValue":25.0,"powerUnit":"watt","txGainValue":6.0,"txGainUnit":"dbi","txHeightM":20.0,"rxGainValue":-3.0,"rxGainUnit":"dbi","rxHeightM":1.5,"polarization":"vertical"}}' >"$request_file"

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

curl --silent --show-error --connect-timeout 5 --max-time 180 \
  --header 'Content-Type: application/json' \
  --data-binary "@$request_file" \
  --output "$cancelled_body" --write-out '%{http_code}' \
  "$BASE_URL/api/calculate" >"$cancelled_status" &
calculation_curl_pid=$!
calculation_curl_start="$(process_start_time "$calculation_curl_pid")" ||
  fail "could not identify the calculation curl child"

operation_entered=false
for _ in $(seq 1 100); do
  probe_status="$(
    curl --silent --show-error --connect-timeout 5 --max-time 10 \
      --header 'Content-Type: application/json' \
      --data-binary '{"point":{"lat":91,"lon":0}}' \
      --output "$probe_body" --write-out '%{http_code}' \
      "$BASE_URL/api/inspect-point"
  )"
  if [[ "$probe_status" == "409" ]] &&
    grep -Fq -- 'another validation operation is already running' "$probe_body"; then
    curl_job_is_owned_and_running ||
      fail "gate became busy after the calculation curl stopped being our child job"
    [[ ! -s "$cancelled_status" ]] ||
      fail "calculation completed before gate ownership was confirmed"
    calculation_operation_owned=true
    operation_entered=true
    break
  fi
  [[ ! -s "$cancelled_status" ]] ||
    fail "calculation completed before its active operation could be confirmed"
  [[ "$probe_status" == "422" ]] ||
    fail "gate probe returned unexpected HTTP $probe_status"
  sleep 0.05
done
[[ "$operation_entered" == "true" ]] ||
  fail "calculation did not enter the validation operation gate"

curl --silent --show-error --connect-timeout 5 --max-time 10 \
  --request POST --header 'Content-Type: application/json' \
  --output "$cancel_body" --write-out '%{http_code}' \
  "$BASE_URL/api/cancel-calculation" >"$cancel_status"
require_status "$cancel_status" "200" "cancellation request"
require_contains "$cancel_body" '"cancelled"[[:space:]]*:[[:space:]]*true' "cancellation request"

set +e
wait "$calculation_curl_pid"
calculation_curl_status=$?
set -e
calculation_curl_pid=""
calculation_curl_start=""
calculation_operation_owned=false
[[ "$calculation_curl_status" -eq 0 ]] ||
  fail "cancelled calculation curl exited with status $calculation_curl_status"
require_status "$cancelled_status" "422" "cancelled calculation"
require_contains "$cancelled_body" '[Cc]ancel' "cancelled calculation"
require_absent "$cancelled_body" '"heatmapPngDataUrl"' "cancelled calculation"
require_absent "$cancelled_body" '"mapOverlayPngDataUrl"' "cancelled calculation"

curl --silent --show-error --connect-timeout 5 --max-time 180 \
  --header 'Content-Type: application/json' \
  --data-binary "@$request_file" \
  --output "$success_body" --write-out '%{http_code}' \
  "$BASE_URL/api/calculate" >"$success_status"
require_status "$success_status" "200" "recovery calculation"
require_contains "$success_body" '"schemaVersion"[[:space:]]*:[[:space:]]*2([,}])' "schema"
require_contains "$success_body" '"imageWidth"[[:space:]]*:[[:space:]]*401([,}])' "image width"
require_contains "$success_body" '"imageHeight"[[:space:]]*:[[:space:]]*401([,}])' "image height"
require_contains "$success_body" '"mapOverlayWidth"[[:space:]]*:[[:space:]]*401([,}])' "overlay width"
require_contains "$success_body" '"mapOverlayHeight"[[:space:]]*:[[:space:]]*401([,}])' "overlay height"
validate_png_field "$success_body" "heatmapPngDataUrl" "$heatmap_match" "$heatmap_png"
validate_png_field "$success_body" "mapOverlayPngDataUrl" "$overlay_match" "$overlay_png"

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
printf 'validation recovery smoke passed: cancel=true cancelled_http=422 recovery_http=200\n'
