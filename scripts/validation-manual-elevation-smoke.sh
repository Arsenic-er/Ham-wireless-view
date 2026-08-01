#!/usr/bin/env bash
# Ham Wireless View
# Project creator and lead developer: Arsenic-er
# SPDX-FileCopyrightText: 2026 Arsenic-er
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

export LC_ALL=C
umask 077

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
readonly PLATFORM_SCRIPT="$SCRIPT_DIR/validation-platform.sh"
readonly BASE_URL="http://127.0.0.1:1421"
readonly RUNTIME_ROOT="$PROJECT_ROOT/.runtime/validation-platform"

work_dir=""
command_file=""
active_operation_id=""
auto_operation_id=""
manual_operation_id=""
issued_operation_id=""

fail() {
  printf 'validation manual elevation smoke failed: %s\n' "$*" >&2
  exit 1
}

is_uuid_v4() {
  [[ "$1" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$ ]]
}

safe_work_dir() {
  local name=""
  [[ -n "${work_dir:-}" && -d "$work_dir" && ! -L "$work_dir" ]] || return 1
  [[ "$(dirname -- "$work_dir")" == "$RUNTIME_ROOT" ]] || return 1
  name="$(basename -- "$work_dir")"
  [[ "$name" == validation-manual-elevation-smoke.* && "$name" != */* ]]
}

read_status() {
  local value=""
  [[ -f "$1" ]] && IFS= read -r value <"$1" || true
  printf '%s' "$value"
}

post_operation_id() {
  local endpoint=$1 operation_id=$2 body_file=$3 status_file=$4
  is_uuid_v4 "$operation_id" || fail "$endpoint received an unsafe operation id"
  printf '{"operationId":"%s"}\n' "$operation_id" >"$command_file"
  curl --silent --show-error --connect-timeout 5 --max-time 10 \
    --request POST --header 'Content-Type: application/json' \
    --data-binary "@$command_file" --output "$body_file" --write-out '%{http_code}' \
    "$BASE_URL$endpoint" >"$status_file"
}

cleanup_operation() {
  local operation_id=$1 body="" status=""
  is_uuid_v4 "$operation_id" || return 0
  safe_work_dir || return 0
  body="$work_dir/cleanup-$operation_id.json"
  status="$work_dir/cleanup-$operation_id.status"
  post_operation_id "/api/cancel-calculation" "$operation_id" "$body" "$status" \
    >/dev/null 2>&1 || true
  for _ in $(seq 1 50); do
    post_operation_id "/api/operation-ack" "$operation_id" "$body" "$status" \
      >/dev/null 2>&1 || true
    if [[ "$(read_status "$status")" == "200" ]] &&
      grep -Eq -- '"acknowledged"[[:space:]]*:[[:space:]]*true([,}])' "$body"; then
      return 0
    fi
    sleep 0.1
  done
}

cleanup() {
  local exit_status=$? operation_id=""
  trap - EXIT INT TERM HUP
  for operation_id in "${active_operation_id:-}" "${auto_operation_id:-}" \
    "${manual_operation_id:-}"; do
    cleanup_operation "$operation_id"
  done
  if [[ -n "${work_dir:-}" ]]; then
    case "$work_dir" in
      "$RUNTIME_ROOT"/validation-manual-elevation-smoke.*)
        if [[ -L "$work_dir" || -e "$work_dir" && ! -d "$work_dir" ]]; then
          printf 'refusing to clean unsafe smoke workspace: %s\n' "$work_dir" >&2
          exit_status=1
        elif [[ -d "$work_dir" ]]; then
          find "$work_dir" -depth -mindepth 1 -delete 2>/dev/null ||
            { printf 'could not clean smoke workspace contents: %s\n' "$work_dir" >&2; exit_status=1; }
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
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

require_status() {
  local actual=""
  actual="$(read_status "$1")"
  [[ "$actual" == "$2" ]] ||
    fail "$3 returned HTTP ${actual:-<empty>}, expected $2"
}

require_contains() {
  grep -Eq -- "$2" "$1" || fail "$3 did not match the expected response"
}

require_absent() {
  if grep -Fq -- "$2" "$1"; then
    fail "$3 unexpectedly contained $2"
  fi
}

extract_string() {
  local match=""
  match="$(grep -Eo -- "\"$2\"[[:space:]]*:[[:space:]]*\"[^\"]*\"" "$1" |
    head -n 1 || true)"
  [[ -n "$match" ]] || return 1
  printf '%s\n' "$match" | sed -E 's/^[^:]+:[[:space:]]*"([^"]*)"$/\1/'
}

extract_integer() {
  local match=""
  match="$(grep -Eo -- "\"$2\"[[:space:]]*:[[:space:]]*[0-9]+" "$1" |
    head -n 1 || true)"
  [[ -n "$match" ]] || return 1
  printf '%s\n' "$match" | sed -E 's/^.*:[[:space:]]*([0-9]+)$/\1/'
}

extract_number() {
  local match=""
  match="$(grep -Eo -- "\"$2\"[[:space:]]*:[[:space:]]*-?[0-9]+([.][0-9]+)?([eE][+-]?[0-9]+)?" \
    "$1" | head -n 1 || true)"
  [[ -n "$match" ]] || return 1
  printf '%s\n' "$match" | sed -E 's/^.*:[[:space:]]*//'
}

issue_ticket() {
  local label=$1 body="$work_dir/$1-ticket.json" status="$work_dir/$1-ticket.status"
  local operation_id=""
  curl --silent --show-error --connect-timeout 5 --max-time 10 \
    --request POST --header 'Content-Type: application/json' \
    --data-binary '{"kind":"calculation"}' --output "$body" --write-out '%{http_code}' \
    "$BASE_URL/api/operation-ticket" >"$status"
  require_status "$status" "200" "$label ticket"
  require_contains "$body" '"schemaVersion"[[:space:]]*:[[:space:]]*1([,}])' "$label ticket schema"
  require_contains "$body" '"kind"[[:space:]]*:[[:space:]]*"calculation"' "$label ticket kind"
  require_contains "$body" '"state"[[:space:]]*:[[:space:]]*"reserved"' "$label ticket state"
  operation_id="$(extract_string "$body" "operationId")" ||
    fail "$label ticket omitted operationId"
  is_uuid_v4 "$operation_id" || fail "$label ticket did not issue a lowercase UUIDv4"
  issued_operation_id="$operation_id"
}

write_request() {
  local operation_id=$1 override=$2 output=$3
  is_uuid_v4 "$operation_id" || fail "cannot write a request with an unsafe operation id"
  [[ "$override" == "null" || "$override" == "1500.0" ]] ||
    fail "unexpected elevation override fixture"
  printf '%s\n' \
    "{\"operationId\":\"$operation_id\",\"request\":{\"center\":{\"lat\":30.5,\"lon\":103.5},\"band\":\"vhf-144\",\"frequencyMhz\":145.0,\"powerValue\":25.0,\"powerUnit\":\"watt\",\"txGainValue\":6.0,\"txGainUnit\":\"dbi\",\"txHeightM\":20.0,\"txGroundElevationOverrideM\":$override,\"rxGainValue\":-3.0,\"rxGainUnit\":\"dbi\",\"rxHeightM\":1.5,\"polarization\":\"vertical\"}}" \
    >"$output"
}

require_terminal_and_ack() {
  local label=$1 operation_id=$2 body="$work_dir/$1-terminal.json"
  local status="$work_dir/$1-terminal.status" response_id="" sequence=""
  post_operation_id "/api/operation-status" "$operation_id" "$body" "$status"
  require_status "$status" "200" "$label terminal status"
  response_id="$(extract_string "$body" "operationId")" ||
    fail "$label terminal status omitted operationId"
  [[ "$response_id" == "$operation_id" ]] ||
    fail "$label terminal status returned a different operation id"
  require_contains "$body" '"kind"[[:space:]]*:[[:space:]]*"calculation"' "$label terminal kind"
  require_contains "$body" '"state"[[:space:]]*:[[:space:]]*"succeeded"' "$label terminal state"
  sequence="$(extract_integer "$body" "sequence")" ||
    fail "$label terminal status omitted sequence"
  (( sequence >= 2 )) || fail "$label terminal sequence omitted calculation progress"
  require_absent "$body" 'data:image/png' "$label terminal status"

  post_operation_id "/api/operation-ack" "$operation_id" "$body" "$status"
  require_status "$status" "200" "$label acknowledgement"
  require_contains "$body" '"acknowledged"[[:space:]]*:[[:space:]]*true([,}])' "$label acknowledgement"
  post_operation_id "/api/operation-status" "$operation_id" "$body" "$status"
  require_status "$status" "404" "$label status after acknowledgement"
  require_contains "$body" '"message"[[:space:]]*:[[:space:]]*"operation not found"' \
    "$label status after acknowledgement"
}

png_payload_hash() {
  local response=$1 key=$2 label=$3
  local field="$work_dir/$3-$2.field" png="$work_dir/$3-$2.png"
  local count="" header=""
  grep -Eo -- "\"$key\"[[:space:]]*:[[:space:]]*\"data:image/png;base64,[A-Za-z0-9+/=]+\"" \
    "$response" >"$field" ||
    fail "$label $key was missing, empty, or invalid base64"
  count="$(wc -l <"$field" | tr -d ' ')"
  [[ "$count" == "1" ]] || fail "$label $key occurred $count times"
  cut -d, -f2- "$field" | tr -d '"' | base64 --decode >"$png" ||
    fail "$label $key could not be decoded"
  header="$(od -An -tx1 -N24 "$png" | tr -d ' \n')"
  [[ "$header" == "89504e470d0a1a0a0000000d494844520000019100000191" ]] ||
    fail "$label $key was not a non-empty 401x401 PNG"
  sha256sum "$png" | awk '{print $1}'
}

run_calculation() {
  local label=$1 override=$2 expected_source=$3
  local request="$work_dir/$1-request.json" result="$work_dir/$1-result.json"
  local status="$work_dir/$1-result.status" source="" elevation=""

  issue_ticket "$label"
  active_operation_id="$issued_operation_id"
  if [[ "$label" == "auto" ]]; then
    auto_operation_id="$active_operation_id"
  else
    manual_operation_id="$active_operation_id"
  fi
  write_request "$active_operation_id" "$override" "$request"
  curl --silent --show-error --connect-timeout 5 --max-time 180 \
    --request POST --header 'Content-Type: application/json' \
    --data-binary "@$request" --output "$result" --write-out '%{http_code}' \
    "$BASE_URL/api/calculate" >"$status"
  require_status "$status" "200" "$label calculation"
  require_contains "$result" '"schemaVersion"[[:space:]]*:[[:space:]]*4([,}])' "$label result schema"
  for dimension in imageWidth imageHeight mapOverlayWidth mapOverlayHeight; do
    require_contains "$result" "\"$dimension\"[[:space:]]*:[[:space:]]*401([,}])" "$label $dimension"
  done
  "$SCRIPT_DIR/validate-calculation-result.py" "$result" >/dev/null ||
    fail "$label result violated the schema-4 filter contract"

  source="$(extract_string "$result" "txGroundElevationSource")" ||
    fail "$label result omitted txGroundElevationSource"
  [[ "$source" == "$expected_source" ]] ||
    fail "$label elevation source was $source, expected $expected_source"
  elevation="$(extract_number "$result" "txGroundElevationM")" ||
    fail "$label result omitted txGroundElevationM"
  awk -v value="$elevation" 'BEGIN {
    numeric = value + 0
    exit (numeric != numeric || numeric <= -1e308 || numeric >= 1e308)
  }' || fail "$label effective elevation was not finite"

  if [[ "$label" == "auto" ]]; then
    awk -v value="$elevation" 'BEGIN {
      difference = value - 526.3443
      if (difference < 0) difference = -difference
      exit !(difference <= 0.1)
    }' || fail "auto DEM elevation $elevation was not within 0.1 m of 526.3443 m"
    auto_elevation="$elevation"
    auto_heatmap_hash="$(png_payload_hash "$result" "heatmapPngDataUrl" "$label")"
    auto_overlay_hash="$(png_payload_hash "$result" "mapOverlayPngDataUrl" "$label")"
  else
    awk -v value="$elevation" 'BEGIN {
      difference = value - 1500
      if (difference < 0) difference = -difference
      exit !(difference <= 1e-9)
    }' || fail "manual effective elevation was $elevation, expected 1500"
    manual_elevation="$elevation"
    manual_heatmap_hash="$(png_payload_hash "$result" "heatmapPngDataUrl" "$label")"
    manual_overlay_hash="$(png_payload_hash "$result" "mapOverlayPngDataUrl" "$label")"
  fi

  require_terminal_and_ack "$label" "$active_operation_id"
  if [[ "$label" == "auto" ]]; then
    auto_operation_id=""
  else
    manual_operation_id=""
  fi
  active_operation_id=""
}

[[ -x "$PLATFORM_SCRIPT" ]] || fail "validation platform manager is unavailable"
[[ -d "$RUNTIME_ROOT" && ! -L "$RUNTIME_ROOT" ]] ||
  fail "managed validation runtime directory is missing or is a symlink"
"$PLATFORM_SCRIPT" status >/dev/null ||
  fail "managed validation platform is not running with the expected identity"
"$PLATFORM_SCRIPT" health >/dev/null ||
  fail "managed validation platform failed its identity-bound health check"

work_dir="$(mktemp -d -- "$RUNTIME_ROOT/validation-manual-elevation-smoke.XXXXXXXX")"
[[ -d "$work_dir" && ! -L "$work_dir" ]] || fail "could not create safe runtime workspace"
command_file="$work_dir/operation-command.json"

health_body="$work_dir/health.json"
health_status="$(curl --silent --show-error --connect-timeout 5 --max-time 10 \
  --output "$health_body" --write-out '%{http_code}' "$BASE_URL/healthz")"
[[ "$health_status" == "200" ]] || fail "health check returned HTTP $health_status"
require_contains "$health_body" '"status"[[:space:]]*:[[:space:]]*"ok"' "health check"
require_contains "$health_body" '"schemaVersion"[[:space:]]*:[[:space:]]*1([,}])' "health schema"

auto_elevation=""
manual_elevation=""
auto_heatmap_hash=""
manual_heatmap_hash=""
auto_overlay_hash=""
manual_overlay_hash=""

run_calculation "auto" "null" "dem"
run_calculation "manual" "1500.0" "manual"

[[ -n "$auto_heatmap_hash" && "$auto_heatmap_hash" != "$manual_heatmap_hash" ]] ||
  fail "manual elevation did not change the heatmap payload hash"
[[ -n "$auto_overlay_hash" && "$auto_overlay_hash" != "$manual_overlay_hash" ]] ||
  fail "manual elevation did not change the overlay payload hash"
"$PLATFORM_SCRIPT" health >/dev/null ||
  fail "managed validation platform failed its final identity-bound health check"

printf 'validation manual elevation smoke passed: auto_ground_m=%s manual_ground_m=%s heatmap_changed=true overlay_changed=true\n' \
  "$auto_elevation" "$manual_elevation"
