#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C
umask 077

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
readonly PLATFORM_SCRIPT="$SCRIPT_DIR/validation-platform.sh"
readonly BASE_URL="http://127.0.0.1:1421"
readonly RUNTIME_ROOT="$PROJECT_ROOT/.runtime/validation-platform"
readonly CURL_PATH="$(readlink -f -- "$(command -v curl)")"

work_dir=""
command_file=""
calculation_curl_pid=""
calculation_curl_start=""
active_operation_id=""
issued_operation_id=""

fail() { printf 'validation progressive preview smoke failed: %s\n' "$*" >&2; exit 1; }
now_ms() { date +%s%3N; }
is_uuid_v4() { [[ "$1" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$ ]]; }

safe_work_dir() {
  local name=""
  [[ -n "${work_dir:-}" && -d "$work_dir" && ! -L "$work_dir" ]] || return 1
  [[ "$(dirname -- "$work_dir")" == "$RUNTIME_ROOT" ]] || return 1
  name="$(basename -- "$work_dir")"
  [[ "$name" == validation-progressive-preview-smoke.* && "$name" != */* ]]
}

process_start_time() {
  local pid=$1 line="" remainder=""
  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
  IFS= read -r line <"/proc/$pid/stat" || return 1
  [[ "$line" == *") "* ]] || return 1
  remainder="${line##*) }"; set -- $remainder
  (( $# >= 20 )) && [[ "${20}" =~ ^[0-9]+$ ]] || return 1
  printf '%s\n' "${20}"
}

process_parent_pid() {
  local pid=$1 line="" remainder=""
  IFS= read -r line <"/proc/$pid/stat" || return 1
  [[ "$line" == *") "* ]] || return 1
  remainder="${line##*) }"; set -- $remainder
  [[ "${2:-}" =~ ^[1-9][0-9]*$ ]] || return 1
  printf '%s\n' "${2}"
}

curl_job_is_owned_and_running() {
  local job_pid=""
  [[ -n "${calculation_curl_pid:-}" && -n "${calculation_curl_start:-}" ]] || return 1
  [[ "$(process_start_time "$calculation_curl_pid" 2>/dev/null)" == "$calculation_curl_start" ]] || return 1
  [[ "$(process_parent_pid "$calculation_curl_pid" 2>/dev/null)" == "$BASHPID" ]] || return 1
  [[ "$(readlink -f -- "/proc/$calculation_curl_pid/exe" 2>/dev/null)" == "$CURL_PATH" ]] || return 1
  while IFS= read -r job_pid; do
    [[ "$job_pid" == "$calculation_curl_pid" ]] && return 0
  done < <(jobs -pr)
  return 1
}

read_status() { local value=""; [[ -f "$1" ]] && IFS= read -r value <"$1" || true; printf '%s' "$value"; }
require_status() {
  local actual=""; actual="$(read_status "$1")"
  [[ "$actual" == "$2" ]] || fail "$3 returned HTTP ${actual:-<empty>}, expected $2"
}
require_contains() { grep -Eq -- "$2" "$1" || fail "$3 did not match the expected response"; }
require_absent() { ! grep -Fq -- "$2" "$1" || fail "$3 unexpectedly contained $2"; }
extract_string() {
  local match=""; match="$(grep -Eo -- "\"$2\"[[:space:]]*:[[:space:]]*\"[^\"]*\"" "$1" | head -n 1 || true)"
  [[ -n "$match" ]] || return 1
  printf '%s\n' "$match" | sed -E 's/^[^:]+:[[:space:]]*"([^"]*)"$/\1/'
}
extract_integer() {
  local match=""; match="$(grep -Eo -- "\"$2\"[[:space:]]*:[[:space:]]*[0-9]+" "$1" | head -n 1 || true)"
  [[ -n "$match" ]] || return 1
  printf '%s\n' "$match" | sed -E 's/^.*:[[:space:]]*([0-9]+)$/\1/'
}

post_operation_id() {
  local endpoint=$1 operation_id=$2 body=$3 status=$4
  is_uuid_v4 "$operation_id" || fail "$endpoint received an unsafe operation id"
  printf '{"operationId":"%s"}\n' "$operation_id" >"$command_file"
  curl --silent --show-error --connect-timeout 5 --max-time 10 --request POST \
    --header 'Content-Type: application/json' --data-binary "@$command_file" \
    --output "$body" --write-out '%{http_code}' "$BASE_URL$endpoint" >"$status"
}

cleanup_operation() {
  local operation_id=$1 body="" status=""
  is_uuid_v4 "$operation_id" && safe_work_dir || return 0
  body="$work_dir/cleanup.json"; status="$work_dir/cleanup.status"
  post_operation_id "/api/cancel-calculation" "$operation_id" "$body" "$status" >/dev/null 2>&1 || true
  curl_job_is_owned_and_running && kill -TERM "$calculation_curl_pid" 2>/dev/null || true
  if [[ -n "${calculation_curl_pid:-}" ]]; then
    set +e; wait "$calculation_curl_pid" >/dev/null 2>&1; set -e
    calculation_curl_pid=""; calculation_curl_start=""
  fi
  for _ in $(seq 1 50); do
    post_operation_id "/api/operation-ack" "$operation_id" "$body" "$status" >/dev/null 2>&1 || true
    if [[ "$(read_status "$status")" == 200 ]] && grep -Eq '"acknowledged"[[:space:]]*:[[:space:]]*true([,}])' "$body"; then return 0; fi
    sleep 0.1
  done
}

cleanup() {
  local code=$?
  trap - EXIT INT TERM HUP
  cleanup_operation "${active_operation_id:-}"
  if [[ -n "${work_dir:-}" ]]; then
    case "$work_dir" in
      "$RUNTIME_ROOT"/validation-progressive-preview-smoke.*)
        if [[ -L "$work_dir" || -e "$work_dir" && ! -d "$work_dir" ]]; then
          printf 'refusing to clean unsafe smoke workspace: %s\n' "$work_dir" >&2; code=1
        elif [[ -d "$work_dir" ]]; then
          find "$work_dir" -depth -mindepth 1 -delete 2>/dev/null || code=1
          [[ ! -d "$work_dir" ]] || rmdir -- "$work_dir" 2>/dev/null || code=1
        fi ;;
      *) printf 'refusing to clean unexpected smoke directory: %s\n' "$work_dir" >&2; code=1 ;;
    esac
  fi
  exit "$code"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

validate_json() {
  python3 - "$1" "$2" <<'PY'
import base64, binascii, hashlib, json, math, struct, sys
mode, path = sys.argv[1:]
text = open(path, encoding="utf-8").read()
def stop(msg): raise SystemExit(msg)
def unique(key):
    if text.count(f'"{key}"') != 1: stop(f"{mode} field {key} was not unique")
def png_hash(value, key):
    unique(key); url = value.get(key); prefix = "data:image/png;base64,"
    if not isinstance(url, str) or not url.startswith(prefix): stop(f"{mode} {key} was not a PNG data URL")
    try: png = base64.b64decode(url[len(prefix):], validate=True)
    except (binascii.Error, ValueError) as exc: stop(f"{mode} {key} base64 was invalid: {exc}")
    if len(png) < 24 or png[:8] != b"\x89PNG\r\n\x1a\n" or png[12:16] != b"IHDR": stop(f"{mode} {key} was not a PNG")
    if struct.unpack(">II", png[16:24]) != (401, 401): stop(f"{mode} {key} was not 401x401")
    return hashlib.sha256(png).hexdigest()
def corners(value):
    item = value.get("mapOverlayCorners")
    if not isinstance(item, list) or len(item) != 4: stop(f"{mode} did not have four corners")
    for pair in item:
        if not isinstance(pair, list) or len(pair) != 2: stop(f"{mode} corner was not a pair")
        for number in pair:
            if isinstance(number, bool) or not isinstance(number, (int, float)) or not math.isfinite(float(number)): stop(f"{mode} corner was invalid")
try: value = json.loads(text)
except json.JSONDecodeError as exc: stop(f"{mode} was not valid JSON: {exc}")
if not isinstance(value, dict): stop(f"{mode} was not an object")
if mode == "preview":
    for key in ("schemaVersion","sequence","completedPixelCount","totalPixelCount","mapOverlayProjection","mapOverlayWidth","mapOverlayHeight","mapOverlayCorners"): unique(key)
    if value.get("schemaVersion") != 1: stop("preview schema was not 1")
    if '"heatmapPngDataUrl"' in text: stop("preview exposed authoritative heatmap")
    if value.get("mapOverlayProjection") != "EPSG:3857" or value.get("mapOverlayWidth") != 401 or value.get("mapOverlayHeight") != 401: stop("preview overlay metadata was invalid")
    sequence, completed, total = (value.get(k) for k in ("sequence","completedPixelCount","totalPixelCount"))
    if any(isinstance(v, bool) or not isinstance(v, int) for v in (sequence,completed,total)): stop("preview counts were not integers")
    if sequence <= 0 or not 0 < completed < total: stop("preview was not a positive partial snapshot")
    corners(value); print(sequence, completed, total, png_hash(value,"mapOverlayPngDataUrl"), sep="\t")
elif mode == "final":
    unique("schemaVersion")
    if value.get("schemaVersion") != 4: stop("final authoritative schema was not 4")
    if any(k in value for k in ("sequence","completedPixelCount","totalPixelCount")): stop("final result contained preview fields")
    if value.get("imageWidth") != 401 or value.get("imageHeight") != 401: stop("final heatmap metadata was invalid")
    if value.get("mapOverlayProjection") != "EPSG:3857" or value.get("mapOverlayWidth") != 401 or value.get("mapOverlayHeight") != 401: stop("final overlay metadata was invalid")
    corners(value); print(png_hash(value,"heatmapPngDataUrl"), png_hash(value,"mapOverlayPngDataUrl"), sep="\t")
elif mode == "cache":
    usage = value.get("usage")
    if not isinstance(usage, dict): stop("cache overview omitted usage")
    total, partial = usage.get("totalBytes"), usage.get("partialBytes")
    if any(isinstance(v,bool) or not isinstance(v,int) or v < 0 for v in (total,partial)): stop("cache byte counts were invalid")
    print(total, partial, sep="\t")
elif mode == "ready":
    keys=("regionId","tileCount","readyDemCount","readyWaterCount","missingAssetCount")
    if not isinstance(value.get("regionId"),str) or not value["regionId"]: stop("inspection region was invalid")
    if any(isinstance(value.get(k),bool) or not isinstance(value.get(k),int) or value[k] < 0 for k in keys[1:]): stop("inspection readiness count was invalid")
    if value.get("dataReady") is not True or value.get("missingAssetCount") != 0: stop("real Chengdu cache was not ready")
    print(*(value[k] for k in keys), "true", sep="\t")
else: stop("unknown validation mode")
PY
}

png_free_status() {
  require_absent "$1" '"heatmapPngDataUrl"' "$2"
  require_absent "$1" '"mapOverlayPngDataUrl"' "$2"
  require_absent "$1" '"mapOverlayFilterEncoding"' "$2"
  require_absent "$1" '"mapOverlayFilterBase64"' "$2"
  require_absent "$1" 'data:image/png' "$2"
}

operation_status() {
  local id=$1 body=$2 status=$3 label=$4 response_id="" kind="" state="" sequence=""
  post_operation_id "/api/operation-status" "$id" "$body" "$status"
  require_status "$status" 200 "$label"
  require_contains "$body" '"schemaVersion"[[:space:]]*:[[:space:]]*1([,}])' "$label schema"
  response_id="$(extract_string "$body" operationId)" || fail "$label omitted operationId"
  [[ "$response_id" == "$id" ]] || fail "$label returned another operation id"
  kind="$(extract_string "$body" kind)" || fail "$label omitted kind"
  [[ "$kind" == calculation ]] || fail "$label kind was $kind"
  state="$(extract_string "$body" state)" || fail "$label omitted state"
  sequence="$(extract_integer "$body" sequence)" || fail "$label omitted sequence"
  png_free_status "$body" "$label"
  printf '%s\t%s\n' "$state" "$sequence"
}

fetch_cache() {
  curl --silent --show-error --connect-timeout 5 --max-time 20 --output "$1" --write-out '%{http_code}' "$BASE_URL/api/cache-overview" >"$2"
  require_status "$2" 200 "cache overview"
}
inspect_chengdu() {
  printf '%s\n' '{"point":{"lat":30.5,"lon":103.5}}' >"$3"
  curl --silent --show-error --connect-timeout 5 --max-time 30 --request POST --header 'Content-Type: application/json' --data-binary "@$3" --output "$1" --write-out '%{http_code}' "$BASE_URL/api/inspect-point" >"$2"
  require_status "$2" 200 "Chengdu point inspection"
}
issue_ticket() {
  curl --silent --show-error --connect-timeout 5 --max-time 10 --request POST --header 'Content-Type: application/json' --data-binary '{"kind":"calculation"}' --output "$1" --write-out '%{http_code}' "$BASE_URL/api/operation-ticket" >"$2"
  require_status "$2" 200 "calculation ticket"
  require_contains "$1" '"schemaVersion"[[:space:]]*:[[:space:]]*1([,}])' "ticket schema"
  require_contains "$1" '"kind"[[:space:]]*:[[:space:]]*"calculation"' "ticket kind"
  require_contains "$1" '"state"[[:space:]]*:[[:space:]]*"reserved"' "ticket state"
  issued_operation_id="$(extract_string "$1" operationId)" || fail "ticket omitted operationId"
  is_uuid_v4 "$issued_operation_id" || fail "ticket did not issue a lowercase UUIDv4"
}
start_calculation() {
  [[ -z "${calculation_curl_pid:-}" ]] || fail "a calculation curl is already tracked"
  printf '%s\n' "{\"operationId\":\"$1\",\"request\":{\"center\":{\"lat\":30.5,\"lon\":103.5},\"band\":\"vhf-144\",\"frequencyMhz\":145.0,\"powerValue\":25.0,\"powerUnit\":\"watt\",\"txGainValue\":6.0,\"txGainUnit\":\"dbi\",\"txHeightM\":20.0,\"txGroundElevationOverrideM\":null,\"rxGainValue\":-3.0,\"rxGainUnit\":\"dbi\",\"rxHeightM\":1.5,\"polarization\":\"vertical\"}}" >"$2"
  curl --silent --show-error --connect-timeout 5 --max-time 300 --request POST --header 'Content-Type: application/json' --data-binary "@$2" --output "$3" --write-out '%{http_code}' "$BASE_URL/api/calculate" >"$4" &
  calculation_curl_pid=$!
  calculation_curl_start="$(process_start_time "$calculation_curl_pid")" || fail "could not identify calculation curl child"
}
post_preview() {
  is_uuid_v4 "$1" && [[ "$2" =~ ^[0-9]+$ ]] || fail "unsafe preview poll capability"
  printf '{"operationId":"%s","afterSequence":%s}\n' "$1" "$2" >"$command_file"
  curl --silent --show-error --connect-timeout 5 --max-time 10 --request POST --header 'Content-Type: application/json' --data-binary "@$command_file" --output "$3" --write-out '%{http_code}' "$BASE_URL/api/operation-preview" >"$4"
}

[[ -x "$PLATFORM_SCRIPT" ]] || fail "validation platform manager is unavailable"
[[ -d "$RUNTIME_ROOT" && ! -L "$RUNTIME_ROOT" ]] || fail "managed runtime is missing or unsafe"
"$PLATFORM_SCRIPT" status >/dev/null || fail "managed validation platform identity check failed"
"$PLATFORM_SCRIPT" health >/dev/null || fail "managed validation platform health check failed"
work_dir="$(mktemp -d -- "$RUNTIME_ROOT/validation-progressive-preview-smoke.XXXXXXXX")"
[[ -d "$work_dir" && ! -L "$work_dir" ]] || fail "could not create safe runtime workspace"
command_file="$work_dir/command.json"

cache_before_body="$work_dir/cache-before.json"; cache_before_status="$work_dir/cache-before.status"
inspect_before_body="$work_dir/inspect-before.json"; inspect_before_status="$work_dir/inspect-before.status"; inspect_before_request="$work_dir/inspect-before-request.json"
ticket_body="$work_dir/ticket.json"; ticket_status="$work_dir/ticket.status"
reserved_body="$work_dir/reserved.json"; reserved_status="$work_dir/reserved.status"
calculation_request="$work_dir/calculation-request.json"; calculation_body="$work_dir/calculation.json"; calculation_status="$work_dir/calculation.status"
preview_body="$work_dir/preview.json"; preview_status="$work_dir/preview.status"; preview_metadata="$work_dir/preview.tsv"; preview_hashes="$work_dir/preview-hashes.txt"
poll_body="$work_dir/poll.json"; poll_status="$work_dir/poll.status"
terminal_preview_body="$work_dir/terminal-preview.json"; terminal_preview_status="$work_dir/terminal-preview.status"
terminal_body="$work_dir/terminal.json"; terminal_status="$work_dir/terminal.status"
ack_body="$work_dir/ack.json"; ack_status="$work_dir/ack.status"; missing_body="$work_dir/missing.json"; missing_status="$work_dir/missing.status"
cache_after_body="$work_dir/cache-after.json"; cache_after_status="$work_dir/cache-after.status"
inspect_after_body="$work_dir/inspect-after.json"; inspect_after_status="$work_dir/inspect-after.status"; inspect_after_request="$work_dir/inspect-after-request.json"

fetch_cache "$cache_before_body" "$cache_before_status"; cache_before="$(validate_json cache "$cache_before_body")"
inspect_chengdu "$inspect_before_body" "$inspect_before_status" "$inspect_before_request"; ready_before="$(validate_json ready "$inspect_before_body")"
issue_ticket "$ticket_body" "$ticket_status"; active_operation_id="$issued_operation_id"
reserved="$(operation_status "$active_operation_id" "$reserved_body" "$reserved_status" "reserved status")"
IFS=$'\t' read -r state status_sequence <<<"$reserved"
[[ "$state" == reserved && "$status_sequence" == 0 ]] || fail "ticket was not reserved at sequence zero"
calculation_started_ms="$(now_ms)"
start_calculation "$active_operation_id" "$calculation_request" "$calculation_body" "$calculation_status"

last_sequence=0; last_completed=0; preview_total=0; preview_count=0; last_status_sequence=0
first_preview_ms=0; previous_preview_observed_ms=0; min_preview_interval_ms=0
max_preview_interval_ms=0; max_preview_json_bytes=0
calculation_observed_stopped=0
: >"$preview_hashes"
for _ in $(seq 1 3600); do
  post_preview "$active_operation_id" "$last_sequence" "$preview_body" "$preview_status"
  preview_http="$(read_status "$preview_status")"
  if [[ "$preview_http" == 200 ]]; then
    validate_json preview "$preview_body" >"$preview_metadata" || fail "preview payload validation failed"
    IFS=$'\t' read -r sequence completed total png_hash <"$preview_metadata"
    preview_observed_ms="$(now_ms)"
    (( preview_observed_ms >= calculation_started_ms )) || fail "preview timing moved backwards"
    preview_json_bytes="$(wc -c <"$preview_body" | tr -d ' ')"
    [[ "$preview_json_bytes" =~ ^[1-9][0-9]*$ ]] || fail "preview JSON byte count was invalid"
    (( preview_json_bytes <= max_preview_json_bytes )) ||
      max_preview_json_bytes=$preview_json_bytes
    if (( preview_count == 0 )); then
      first_preview_ms=$((preview_observed_ms - calculation_started_ms))
    else
      interval_ms=$((preview_observed_ms - previous_preview_observed_ms))
      (( interval_ms > 0 )) || fail "adjacent preview timing did not advance"
      if (( min_preview_interval_ms == 0 || interval_ms < min_preview_interval_ms )); then
        min_preview_interval_ms=$interval_ms
      fi
      if (( interval_ms > max_preview_interval_ms )); then
        max_preview_interval_ms=$interval_ms
      fi
    fi
    previous_preview_observed_ms=$preview_observed_ms
    (( sequence > last_sequence )) || fail "preview sequence did not strictly increase"
    (( completed > last_completed )) || fail "preview completed count did not strictly increase"
    if (( preview_total == 0 )); then preview_total=$total; else (( total == preview_total )) || fail "preview total changed"; fi
    printf '%s\n' "$png_hash" >>"$preview_hashes"
    last_sequence=$sequence; last_completed=$completed; preview_count=$((preview_count + 1))
  elif [[ "$preview_http" == 204 ]]; then
    [[ ! -s "$preview_body" ]] || fail "204 preview response contained a body"
  else
    fail "preview poll returned HTTP ${preview_http:-<empty>}"
  fi
  snapshot="$(operation_status "$active_operation_id" "$poll_body" "$poll_status" "progress status")"
  IFS=$'\t' read -r state status_sequence <<<"$snapshot"
  (( status_sequence >= last_status_sequence )) || fail "status sequence moved backwards"
  last_status_sequence=$status_sequence
  [[ "$state" == reserved || "$state" == running || "$state" == succeeded ]] || fail "calculation entered unexpected state $state"
  [[ ! -s "$calculation_status" ]] || break
  if ! curl_job_is_owned_and_running; then
    calculation_observed_stopped=1
    break
  fi
  sleep 0.05
done
if [[ ! -s "$calculation_status" && "$calculation_observed_stopped" == 0 ]]; then
  fail "calculation exceeded smoke timeout"
fi
set +e; wait "$calculation_curl_pid"; curl_exit=$?; set -e
calculation_curl_pid=""; calculation_curl_start=""
[[ "$curl_exit" == 0 ]] || fail "calculation curl exited with status $curl_exit"
[[ -s "$calculation_status" ]] || fail "calculation curl did not publish HTTP status"
require_status "$calculation_status" 200 "authoritative calculation"
calculation_finished_ms="$(now_ms)"
(( calculation_finished_ms >= calculation_started_ms )) || fail "calculation timing moved backwards"
total_ms=$((calculation_finished_ms - calculation_started_ms))
(( preview_count >= 2 )) || fail "calculation completed too fast to observe two previews, or preview cadence regressed"
unique_hashes="$(sort -u "$preview_hashes" | wc -l | tr -d ' ')"
(( unique_hashes >= 2 )) || fail "two preview PNG hashes were not different"
(( first_preview_ms > 0 && min_preview_interval_ms > 0 && max_preview_interval_ms >= min_preview_interval_ms && max_preview_json_bytes > 0 )) ||
  fail "progressive preview runtime metrics were incomplete"
validate_json final "$calculation_body" >"$work_dir/final.tsv" || fail "schema-4 authoritative result validation failed"
"$SCRIPT_DIR/validate-calculation-result.py" "$calculation_body" >"$work_dir/filter-contract.txt" ||
  fail "authoritative result violated the schema-4 filter contract"

post_preview "$active_operation_id" "$last_sequence" "$terminal_preview_body" "$terminal_preview_status"
require_status "$terminal_preview_status" 204 "terminal preview"; [[ ! -s "$terminal_preview_body" ]] || fail "terminal preview retained a body"
terminal="$(operation_status "$active_operation_id" "$terminal_body" "$terminal_status" "terminal status")"
IFS=$'\t' read -r state terminal_sequence <<<"$terminal"
[[ "$state" == succeeded ]] || fail "terminal state was $state"; (( terminal_sequence >= last_status_sequence )) || fail "terminal sequence moved backwards"
post_operation_id "/api/operation-ack" "$active_operation_id" "$ack_body" "$ack_status"
require_status "$ack_status" 200 "operation acknowledgement"; require_contains "$ack_body" '"acknowledged"[[:space:]]*:[[:space:]]*true([,}])' "operation acknowledgement"
post_operation_id "/api/operation-status" "$active_operation_id" "$missing_body" "$missing_status"
require_status "$missing_status" 404 "status after acknowledgement"; require_contains "$missing_body" '"message"[[:space:]]*:[[:space:]]*"operation not found"' "status after acknowledgement"; png_free_status "$missing_body" "status after acknowledgement"
active_operation_id=""

fetch_cache "$cache_after_body" "$cache_after_status"; cache_after="$(validate_json cache "$cache_after_body")"
inspect_chengdu "$inspect_after_body" "$inspect_after_status" "$inspect_after_request"; ready_after="$(validate_json ready "$inspect_after_body")"
[[ "$cache_after" == "$cache_before" ]] || fail "calculation changed cache totalBytes or partialBytes"
[[ "$ready_after" == "$ready_before" ]] || fail "calculation changed Chengdu readiness"
"$PLATFORM_SCRIPT" health >/dev/null || fail "final managed platform health check failed"
IFS=$'\t' read -r cache_total cache_partial <<<"$cache_after"
printf 'validation progressive preview smoke passed: previews=%s unique_pngs=%s last_completed=%s total=%s total_ms=%s first_preview_ms=%s preview_interval_ms_min=%s preview_interval_ms_max=%s max_preview_json_bytes=%s cache_bytes=%s partial_bytes=%s\n' \
  "$preview_count" "$unique_hashes" "$last_completed" "$preview_total" "$total_ms" "$first_preview_ms" \
  "$min_preview_interval_ms" "$max_preview_interval_ms" "$max_preview_json_bytes" "$cache_total" "$cache_partial"
