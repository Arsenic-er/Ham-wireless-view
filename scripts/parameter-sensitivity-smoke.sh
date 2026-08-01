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
readonly DATA_ROOT="$PROJECT_ROOT/.runtime/validation-platform/data"
readonly RUNTIME_ROOT="$PROJECT_ROOT/.runtime/parameter-sensitivity-smoke"

work_dir=""
cache_lock_fd=""

fail() {
  printf 'parameter sensitivity smoke failed: %s\n' "$*" >&2
  exit 1
}

path_is_safe_project_path() {
  local path="$1" relative="" current="" part="" resolved=""
  local -a parts=()
  [[ "$path" == "$PROJECT_ROOT" || "$path" == "$PROJECT_ROOT/"* ]] || return 1
  relative="${path#"$PROJECT_ROOT"}"
  relative="${relative#/}"
  current="$PROJECT_ROOT"
  if [[ -n "$relative" ]]; then
    IFS='/' read -r -a parts <<<"$relative"
    for part in "${parts[@]}"; do
      [[ -n "$part" && "$part" != "." && "$part" != ".." ]] || return 1
      current="$current/$part"
      [[ ! -L "$current" ]] || return 1
    done
  fi
  if [[ -e "$path" ]]; then
    resolved="$(readlink -f -- "$path" 2>/dev/null)" || return 1
    [[ "$resolved" == "$PROJECT_ROOT" || "$resolved" == "$PROJECT_ROOT/"* ]] || return 1
  fi
}

safe_work_dir() {
  local name=""
  [[ -n "${work_dir:-}" && -d "$work_dir" && ! -L "$work_dir" ]] || return 1
  path_is_safe_project_path "$RUNTIME_ROOT" || return 1
  path_is_safe_project_path "$work_dir" || return 1
  [[ "$(dirname -- "$work_dir")" == "$RUNTIME_ROOT" ]] || return 1
  [[ "$(readlink -f -- "$(dirname -- "$work_dir")")" == "$RUNTIME_ROOT" ]] || return 1
  name="$(basename -- "$work_dir")"
  [[ "$name" == run.* && "$name" != */* ]]
}

cleanup() {
  local status=$?
  trap - EXIT INT TERM HUP
  if [[ "$cache_lock_fd" =~ ^[0-9]+$ ]]; then
    flock -u "$cache_lock_fd" 2>/dev/null || status=1
    exec {cache_lock_fd}>&-
    cache_lock_fd=""
  fi
  if [[ -n "${work_dir:-}" ]]; then
    if ! safe_work_dir; then
      printf 'refusing to clean unsafe smoke workspace: %s\n' "$work_dir" >&2
      status=1
    else
      find "$work_dir" -depth -mindepth 1 -delete 2>/dev/null || status=1
      [[ ! -d "$work_dir" ]] || rmdir -- "$work_dir" 2>/dev/null || status=1
    fi
  fi
  if [[ -d "$RUNTIME_ROOT" && ! -L "$RUNTIME_ROOT" ]] &&
    path_is_safe_project_path "$RUNTIME_ROOT"; then
    rmdir -- "$RUNTIME_ROOT" 2>/dev/null || true
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

[[ $# -eq 0 ]] || fail "this script accepts no arguments"
[[ -d "$DATA_ROOT" && ! -L "$DATA_ROOT" ]] ||
  fail "real validation cache is missing or symlinked: $DATA_ROOT"
[[ "$(readlink -f -- "$DATA_ROOT")" == "$DATA_ROOT" ]] ||
  fail "real validation cache did not resolve to its canonical project path"
for directory in "$DATA_ROOT/dem" "$DATA_ROOT/water"; do
  [[ -d "$directory" && ! -L "$directory" ]] ||
    fail "required cache payload directory is missing or symlinked: $directory"
done
if find "$DATA_ROOT" -type l -print -quit | grep -q .; then
  fail "real validation cache contains a symlink"
fi

path_is_safe_project_path "$RUNTIME_ROOT" ||
  fail "unsafe smoke runtime root before creation: $RUNTIME_ROOT"
mkdir -p -- "$RUNTIME_ROOT"
path_is_safe_project_path "$RUNTIME_ROOT" ||
  fail "unsafe smoke runtime root after creation: $RUNTIME_ROOT"
[[ -d "$RUNTIME_ROOT" && ! -L "$RUNTIME_ROOT" ]] ||
  fail "unsafe smoke runtime root: $RUNTIME_ROOT"
chmod 700 "$RUNTIME_ROOT"
work_dir="$(mktemp -d --tmpdir="$RUNTIME_ROOT" run.XXXXXX)"
chmod 700 "$work_dir"
safe_work_dir || fail "unsafe smoke workspace after creation: $work_dir"

snapshot_root="$work_dir/cache-snapshot"
cache_lock="$DATA_ROOT/.cache.lock"
[[ -f "$cache_lock" && ! -L "$cache_lock" ]] ||
  fail "real cache lock file is missing or unsafe"
exec {cache_lock_fd}<>"$cache_lock"
flock -n "$cache_lock_fd" ||
  fail "real cache is active; retry after the current cache operation finishes"

source_manifest() {
  find "$DATA_ROOT" -type f -print0 |
    sort -z |
    xargs -0 -r sha256sum
}

source_manifest >"$work_dir/before.manifest"
before_bytes="$(du -sb -- "$DATA_ROOT" | awk '{print $1}')"
payload_count="$(find "$DATA_ROOT/dem" "$DATA_ROOT/water" -type f | wc -l | tr -d ' ')"
[[ "$before_bytes" =~ ^[0-9]+$ && "$payload_count" =~ ^[0-9]+$ ]] ||
  fail "could not snapshot cache size and payload count"
[[ "$payload_count" -ge 50 ]] ||
  fail "expected at least the 50 Chengdu DEM/WBM payloads, found $payload_count"

mkdir -- "$snapshot_root"
chmod 700 "$snapshot_root"
cp -a --reflink=auto -- "$DATA_ROOT/." "$snapshot_root/"
path_is_safe_project_path "$snapshot_root" ||
  fail "cache snapshot escaped the isolated smoke workspace"
[[ -f "$snapshot_root/cache.sqlite3" && -f "$snapshot_root/.cache.lock" ]] ||
  fail "cache snapshot is incomplete"

printf 'running real Chengdu parameter matrix against isolated snapshot %s\n' "$snapshot_root"
HAMHEATMAP_REAL_CACHE_ROOT="$snapshot_root" "$PROJECT_ROOT/scripts/cargo-project.sh" test --release --locked -p hamheatmap-app-service --lib real_parameter_sensitivity::real_chengdu_parameter_sensitivity_matrix -- --ignored --exact --nocapture |
  tee "$work_dir/test-output.txt"

[[ "$(grep -c '^PARAMETER_SENSITIVITY_JSON=' "$work_dir/test-output.txt")" -eq 1 ]] ||
  fail "test output did not contain exactly one structured sensitivity report"

source_manifest >"$work_dir/after.manifest"
after_bytes="$(du -sb -- "$DATA_ROOT" | awk '{print $1}')"
[[ "$after_bytes" == "$before_bytes" ]] ||
  fail "real cache size changed from $before_bytes to $after_bytes bytes"
cmp -s "$work_dir/before.manifest" "$work_dir/after.manifest" ||
  fail "real cache content, including SQLite metadata, changed during the matrix"
for checked_root in "$DATA_ROOT" "$snapshot_root"; do
  if find "$checked_root" -type f -name '*.partial' -print -quit | grep -q .; then
    fail "matrix observed or left a partial cache payload under $checked_root"
  fi
done

manifest_sha256="$(sha256sum "$work_dir/after.manifest" | awk '{print $1}')"
flock -u "$cache_lock_fd"
exec {cache_lock_fd}>&-
cache_lock_fd=""
printf 'parameter sensitivity smoke passed: cache_bytes=%s payloads=%s source_manifest_sha256=%s\n' "$after_bytes" "$payload_count" "$manifest_sha256"
