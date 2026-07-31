#!/usr/bin/env bash
set -euo pipefail

export LC_ALL=C
umask 077

script_path="$(readlink -f "${BASH_SOURCE[0]}")"
project_root="$(cd "$(dirname "$script_path")/.." && pwd)"
runtime_root="$project_root/.runtime/validation-platform"
state_dir="$runtime_root/state"
log_dir="$runtime_root/logs"
data_dir="$runtime_root/data"
secrets_dir="$runtime_root/secrets"
basemap_token_file="$secrets_dir/tianditu.token"
dist_dir="$project_root/app/dist"
server_binary="$project_root/target/release/hamheatmap-validation-server"
pid_file="$state_dir/server.pid"
runner_pid_file="$state_dir/runner.pid"
lock_dir="$state_dir/control.lock"
lock_owner_file="$lock_dir/owner"
runner_claim_dir="$state_dir/runner.claim"
runner_claim_owner_file="$runner_claim_dir/owner"
server_log="$log_dir/server.log"
launcher_log="$log_dir/launcher.log"
build_metadata="$runtime_root/build.txt"
server_help="$runtime_root/server-help.txt"
listen_address="127.0.0.1:1421"
health_url="http://127.0.0.1:1421/healthz"
log_max_bytes=10000000
log_backups=3

usage() {
    cat <<'EOF'
Usage: scripts/validation-platform.sh <command>

Commands:
  build    Build VITE_VALIDATION_SERVER=1 frontend and release server
  start    Start the loopback-only server and require a healthy /healthz
  stop     Stop only a strictly verified project server process
  status   Report managed process and HTTP health state
  health   Query /healthz for a strictly verified managed process
  basemap-token <set|status|clear>  Manage TianDiTu token without echoing it
  self-test  Test stale-lock recovery, path guards, and PID identity checks
EOF
}

fail() { echo "validation platform: $*" >&2; exit 1; }

path_is_safe_project_path() {
    local path="$1" relative="" current="" part="" resolved=""
    local -a path_parts=()
    [[ "$path" == "$project_root" || "$path" == "$project_root/"* ]] || return 1
    relative="${path#"$project_root"}"
    relative="${relative#/}"
    current="$project_root"
    if [[ -n "$relative" ]]; then
        IFS='/' read -r -a path_parts <<< "$relative"
        for part in "${path_parts[@]}"; do
            [[ -n "$part" && "$part" != "." && "$part" != ".." ]] || return 1
            current="$current/$part"
            [[ ! -L "$current" ]] || return 1
        done
    fi
    if [[ -e "$path" ]]; then
        resolved="$(readlink -f -- "$path" 2>/dev/null)" || return 1
        [[ "$resolved" == "$project_root" || "$resolved" == "$project_root/"* ]] || return 1
    fi
}

validate_log_rotation_paths() {
    local index=0 path=""
    for path in "$server_log" "$launcher_log"; do
        path_is_safe_project_path "$path" || \
            fail "unsafe or symlinked managed log path: $path"
    done
    for ((index = 1; index <= log_backups; index++)); do
        path="$server_log.$index"
        path_is_safe_project_path "$path" || \
            fail "unsafe or symlinked managed log backup path: $path"
    done
    path="$launcher_log.1"
    path_is_safe_project_path "$path" || \
        fail "unsafe or symlinked managed log backup path: $path"
}

validate_managed_paths() {
    local path=""
    for path in "$runtime_root" "$state_dir" "$log_dir" "$data_dir" "$secrets_dir" "$dist_dir" \
        "$server_binary" "$pid_file" "$runner_pid_file" "$lock_dir" \
        "$runner_claim_dir" "$server_log" "$launcher_log" "$build_metadata" \
        "$server_help" "$basemap_token_file"; do
        path_is_safe_project_path "$path" || fail "unsafe or symlinked managed path: $path"
    done
    validate_log_rotation_paths
}

ensure_layout() {
    validate_managed_paths
    mkdir -p "$state_dir" "$log_dir" "$data_dir" "$secrets_dir"
    validate_managed_paths
    chmod 700 "$runtime_root" "$state_dir" "$log_dir" "$data_dir" "$secrets_dir"
}

process_start_time() {
    local pid="$1" stat_line="" remainder=""
    [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
    IFS= read -r stat_line < "/proc/$pid/stat" || return 1
    [[ "$stat_line" == *") "* ]] || return 1
    remainder="${stat_line##*) }"
    set -- $remainder
    (( $# >= 20 )) || return 1
    [[ "${20}" =~ ^[0-9]+$ ]] || return 1
    printf '%s\n' "${20}"
}
process_identity_matches() {
    local pid="$1" expected_start="$2" actual_start=""
    actual_start="$(process_start_time "$pid" 2>/dev/null)" || return 1
    [[ "$actual_start" == "$expected_start" ]]
}
read_boot_id() {
    local value=""
    IFS= read -r value < /proc/sys/kernel/random/boot_id || return 1
    [[ "$value" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]] || return 1
    printf '%s\n' "$value"
}
lock_owner_matches() {
    local expected_pid="$1" expected_start="$2" expected_boot="$3"
    local owner_pid="" owner_start="" owner_boot="" extra=""
    [[ -f "$lock_owner_file" && ! -L "$lock_owner_file" ]] || return 1
    [[ "$(stat -c '%u' "$lock_owner_file" 2>/dev/null)" == "$(id -u)" ]] || return 1
    read -r owner_pid owner_start owner_boot extra < "$lock_owner_file" || return 1
    [[ -z "$extra" && "$owner_pid" == "$expected_pid" && \
        "$owner_start" == "$expected_start" && "$owner_boot" == "$expected_boot" ]]
}
lock_owner_is_live() {
    local owner_pid="" owner_start="" owner_boot="" extra="" current_boot=""
    [[ -d "$lock_dir" && ! -L "$lock_dir" ]] || return 1
    [[ "$(stat -c '%u' "$lock_dir" 2>/dev/null)" == "$(id -u)" ]] || return 1
    [[ -f "$lock_owner_file" && ! -L "$lock_owner_file" ]] || return 1
    read -r owner_pid owner_start owner_boot extra < "$lock_owner_file" || return 1
    current_boot="$(read_boot_id)" || return 1
    [[ -z "$extra" && "$owner_pid" =~ ^[1-9][0-9]*$ && \
        "$owner_start" =~ ^[0-9]+$ && "$owner_boot" == "$current_boot" ]] || return 1
    process_identity_matches "$owner_pid" "$owner_start"
}
lock_is_old_enough_to_recover() {
    local modified="" now=""
    [[ -d "$lock_dir" && ! -L "$lock_dir" ]] || return 1
    modified="$(stat -c '%Y' "$lock_dir" 2>/dev/null)" || return 1
    now="$(date +%s)"
    [[ "$modified" =~ ^[0-9]+$ && "$now" =~ ^[0-9]+$ ]] || return 1
    (( now - modified >= 5 ))
}

lock_held=0
lock_owner_start=""
lock_owner_boot=""
release_lock() {
    if [[ "$lock_held" -eq 1 ]]; then
        if lock_owner_matches "$$" "$lock_owner_start" "$lock_owner_boot"; then
            rm -f -- "$lock_owner_file"
            rmdir -- "$lock_dir" 2>/dev/null || true
        fi
        lock_held=0
        lock_owner_start=""
        lock_owner_boot=""
    fi
}
acquire_lock() {
    local stale_dir="" owner_temporary="" attempt=0
    ensure_layout
    lock_owner_start="$(process_start_time "$$")" || fail "cannot identify lock owner process"
    lock_owner_boot="$(read_boot_id)" || fail "cannot identify host boot"
    for ((attempt = 0; attempt < 3; attempt++)); do
        if mkdir -- "$lock_dir" 2>/dev/null; then
            owner_temporary="$(mktemp --tmpdir="$lock_dir" .owner.XXXXXX.tmp)"
            printf '%s %s %s\n' "$$" "$lock_owner_start" "$lock_owner_boot" > "$owner_temporary"
            chmod 600 "$owner_temporary"
            mv -- "$owner_temporary" "$lock_owner_file"
            lock_held=1
            trap release_lock EXIT
            trap 'exit 129' HUP
            trap 'exit 130' INT
            trap 'exit 143' TERM
            return
        fi
        path_is_safe_project_path "$lock_dir" || fail "unsafe control lock path"
        lock_owner_is_live && fail "another build/start/stop operation is active"
        lock_is_old_enough_to_recover || \
            fail "control lock is active or owner initialization is incomplete"
        stale_dir="$state_dir/control.lock.stale.$$.$attempt"
        [[ ! -e "$stale_dir" && ! -L "$stale_dir" ]] || fail "stale-lock recovery target already exists"
        if mv -- "$lock_dir" "$stale_dir" 2>/dev/null; then
            local entry=""
            [[ ! -L "$stale_dir/owner" ]] || fail "stale lock owner is a symlink"
            rm -f -- "$stale_dir/owner"
            for entry in "$stale_dir"/.owner.*.tmp; do
                [[ -e "$entry" || -L "$entry" ]] || continue
                [[ -f "$entry" && ! -L "$entry" ]] || fail "unsafe stale lock temporary: $entry"
                [[ "$(stat -c '%u' "$entry" 2>/dev/null)" == "$(id -u)" ]] || \
                    fail "stale lock temporary has unexpected owner: $entry"
                rm -f -- "$entry"
            done
            rmdir -- "$stale_dir" 2>/dev/null || fail "stale lock contains unexpected files: $stale_dir"
        fi
    done
    fail "could not acquire control lock after stale-lock recovery"
}

runner_claim_held=0
runner_claim_start=""
runner_claim_boot=""
runner_claim_owner_matches() {
    local expected_pid="$1" expected_start="$2" expected_boot="$3"
    local owner_pid="" owner_start="" owner_boot="" extra=""
    [[ -f "$runner_claim_owner_file" && ! -L "$runner_claim_owner_file" ]] || return 1
    [[ "$(stat -c '%u' "$runner_claim_owner_file" 2>/dev/null)" == "$(id -u)" ]] || return 1
    read -r owner_pid owner_start owner_boot extra < "$runner_claim_owner_file" || return 1
    [[ -z "$extra" && "$owner_pid" == "$expected_pid" && \
        "$owner_start" == "$expected_start" && "$owner_boot" == "$expected_boot" ]]
}
runner_claim_owner_is_live() {
    local owner_pid="" owner_start="" owner_boot="" extra="" current_boot=""
    [[ -d "$runner_claim_dir" && ! -L "$runner_claim_dir" ]] || return 1
    [[ "$(stat -c '%u' "$runner_claim_dir" 2>/dev/null)" == "$(id -u)" ]] || return 1
    [[ -f "$runner_claim_owner_file" && ! -L "$runner_claim_owner_file" ]] || return 1
    read -r owner_pid owner_start owner_boot extra < "$runner_claim_owner_file" || return 1
    current_boot="$(read_boot_id)" || return 1
    [[ -z "$extra" && "$owner_pid" =~ ^[1-9][0-9]*$ && \
        "$owner_start" =~ ^[0-9]+$ && "$owner_boot" == "$current_boot" ]] || return 1
    process_identity_matches "$owner_pid" "$owner_start"
}
runner_claim_is_old_enough_to_recover() {
    local modified="" now=""
    [[ -d "$runner_claim_dir" && ! -L "$runner_claim_dir" ]] || return 1
    modified="$(stat -c '%Y' "$runner_claim_dir" 2>/dev/null)" || return 1
    now="$(date +%s)"
    [[ "$modified" =~ ^[0-9]+$ && "$now" =~ ^[0-9]+$ ]] || return 1
    (( now - modified >= 5 ))
}
release_runner_claim() {
    if [[ "$runner_claim_held" -eq 1 ]]; then
        if runner_claim_owner_matches "$$" "$runner_claim_start" "$runner_claim_boot"; then
            rm -f -- "$runner_claim_owner_file"
            rmdir -- "$runner_claim_dir" 2>/dev/null || true
        fi
        runner_claim_held=0
        runner_claim_start=""
        runner_claim_boot=""
    fi
}
acquire_runner_claim() {
    local stale_dir="" owner_temporary="" entry="" attempt=0
    ensure_layout
    runner_claim_start="$(process_start_time "$$")" || fail "cannot identify runner process"
    runner_claim_boot="$(read_boot_id)" || fail "cannot identify host boot"
    for ((attempt = 0; attempt < 3; attempt++)); do
        if mkdir -- "$runner_claim_dir" 2>/dev/null; then
            owner_temporary="$(mktemp --tmpdir="$runner_claim_dir" .owner.XXXXXX.tmp)"
            printf '%s %s %s\n' "$$" "$runner_claim_start" "$runner_claim_boot" > "$owner_temporary"
            chmod 600 "$owner_temporary"
            mv -- "$owner_temporary" "$runner_claim_owner_file"
            runner_claim_held=1
            return
        fi
        path_is_safe_project_path "$runner_claim_dir" || fail "unsafe runner claim path"
        runner_claim_owner_is_live && fail "another validation runner is active"
        runner_claim_is_old_enough_to_recover || \
            fail "runner claim is active or owner initialization is incomplete"
        stale_dir="$state_dir/runner.claim.stale.$$.$attempt"
        [[ ! -e "$stale_dir" && ! -L "$stale_dir" ]] || \
            fail "stale runner-claim recovery target already exists"
        if mv -- "$runner_claim_dir" "$stale_dir" 2>/dev/null; then
            [[ ! -L "$stale_dir/owner" ]] || fail "stale runner claim owner is a symlink"
            rm -f -- "$stale_dir/owner"
            for entry in "$stale_dir"/.owner.*.tmp; do
                [[ -e "$entry" || -L "$entry" ]] || continue
                [[ -f "$entry" && ! -L "$entry" ]] || \
                    fail "unsafe stale runner claim temporary: $entry"
                [[ "$(stat -c '%u' "$entry" 2>/dev/null)" == "$(id -u)" ]] || \
                    fail "stale runner claim temporary has unexpected owner: $entry"
                rm -f -- "$entry"
            done
            rmdir -- "$stale_dir" 2>/dev/null || \
                fail "stale runner claim contains unexpected files: $stale_dir"
        fi
    done
    fail "could not acquire runner claim after stale-claim recovery"
}

read_pid_file() {
    local file="$1" value=""
    [[ -f "$file" && ! -L "$file" ]] || return 1
    [[ "$(stat -c '%u' "$file" 2>/dev/null)" == "$(id -u)" ]] || return 1
    IFS= read -r value < "$file" || true
    [[ "$value" =~ ^[1-9][0-9]*$ ]] || return 1
    printf '%s\n' "$value"
}
write_pid_file() {
    local file="$1" value="$2" temporary=""
    path_is_safe_project_path "$file" || fail "unsafe PID file path: $file"
    temporary="$(mktemp --tmpdir="$(dirname "$file")" ".$(basename "$file").XXXXXX")"
    printf '%s\n' "$value" > "$temporary"
    chmod 600 "$temporary"
    mv -f -- "$temporary" "$file"
}
pid_exists() { kill -0 "$1" 2>/dev/null; }
argv_matches() {
    local pid="$1"
    shift
    local -a actual=()
    [[ -r "/proc/$pid/cmdline" ]] || return 1
    mapfile -d '' -t actual < "/proc/$pid/cmdline" || return 1
    (( ${#actual[@]} == $# )) || return 1
    local index=0 expected=""
    for expected in "$@"; do
        [[ "${actual[$index]}" == "$expected" ]] || return 1
        ((index += 1))
    done
}

pid_is_managed_server() {
    local pid="$1" executable=""
    [[ "$pid" =~ ^[1-9][0-9]*$ && -d "/proc/$pid" ]] || return 1
    [[ "$(stat -c '%u' "/proc/$pid" 2>/dev/null)" == "$(id -u)" ]] || return 1
    executable="$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)"
    [[ "$executable" == "$server_binary" ]] || return 1
    argv_matches "$pid" "$server_binary" \
        "--bind" "$listen_address" \
        "--dist-dir" "$dist_dir" \
        "--data-root" "$data_dir" \
        "--basemap-token-file" "$basemap_token_file"
}
pid_is_managed_runner() {
    local pid="$1" executable=""
    [[ "$pid" =~ ^[1-9][0-9]*$ && -d "/proc/$pid" ]] || return 1
    [[ "$(stat -c '%u' "/proc/$pid" 2>/dev/null)" == "$(id -u)" ]] || return 1
    executable="$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)"
    [[ "$executable" == "$(readlink -f "$(command -v bash)")" ]] || return 1
    argv_matches "$pid" "bash" "$script_path" "__run"
}
pid_is_managed_monitor() {
    local pid="$1" server_pid="$2" server_start="$3" executable=""
    [[ "$pid" =~ ^[1-9][0-9]*$ && -d "/proc/$pid" ]] || return 1
    [[ "$(stat -c '%u' "/proc/$pid" 2>/dev/null)" == "$(id -u)" ]] || return 1
    executable="$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)"
    [[ "$executable" == "$(readlink -f "$(command -v bash)")" ]] || return 1
    argv_matches "$pid" "bash" "$script_path" "__monitor-log" "$server_pid" "$server_start"
}

managed_pid=""
resolve_server() {
    local recorded=""
    managed_pid=""
    [[ -e "$pid_file" ]] || return 1
    recorded="$(read_pid_file "$pid_file" 2>/dev/null || true)"
    [[ -n "$recorded" ]] || return 2
    if pid_is_managed_server "$recorded"; then
        managed_pid="$recorded"
        return 0
    fi
    pid_exists "$recorded" && return 2
    return 3
}

http_health() {
    curl --fail --silent --show-error --connect-timeout 1 --max-time 5 \
        --max-filesize 1048576 "$health_url"
}

rotate_log() {
    local index=0 temporary=""
    validate_log_rotation_paths
    rm -f -- "$server_log.$log_backups"
    for ((index = log_backups - 1; index >= 1; index--)); do
        [[ -f "$server_log.$index" ]] && mv -fT -- \
            "$server_log.$index" "$server_log.$((index + 1))"
    done
    if [[ -f "$server_log" ]]; then
        temporary="$(mktemp --tmpdir="$log_dir" .server.log.1.XXXXXX.tmp)"
        cp -- "$server_log" "$temporary"
        chmod 600 "$temporary"
        path_is_safe_project_path "$server_log.1" || {
            rm -f -- "$temporary"
            fail "unsafe log backup path before replacement: $server_log.1"
        }
        mv -fT -- "$temporary" "$server_log.1"
    fi
    path_is_safe_project_path "$server_log" || fail "unsafe log path before truncation"
    : > "$server_log"
    chmod 600 "$server_log"
}
rotate_launcher_log() {
    validate_log_rotation_paths
    if [[ -f "$launcher_log" && "$(wc -c < "$launcher_log")" -ge 1000000 ]]; then
        mv -fT -- "$launcher_log" "$launcher_log.1"
    fi
}
monitor_log() {
    local server_pid="$1" server_start="$2" size=0
    while pid_is_managed_server "$server_pid" && \
        process_identity_matches "$server_pid" "$server_start"; do
        sleep 5
        [[ -f "$server_log" ]] || continue
        size="$(wc -c < "$server_log")"
        (( size < log_max_bytes )) || rotate_log
    done
}

runner_server_pid=""
runner_server_start=""
runner_monitor_pid=""
runner_monitor_start=""
runner_cleanup() {
    local recorded=""
    if [[ -n "$runner_server_pid" && -n "$runner_server_start" ]] && \
        pid_is_managed_server "$runner_server_pid" && \
        process_identity_matches "$runner_server_pid" "$runner_server_start"; then
        kill -TERM "$runner_server_pid" 2>/dev/null || true
    fi
    if [[ -n "$runner_monitor_pid" && -n "$runner_monitor_start" ]] && \
        pid_is_managed_monitor "$runner_monitor_pid" "$runner_server_pid" "$runner_server_start" && \
        process_identity_matches "$runner_monitor_pid" "$runner_monitor_start"; then
        kill -TERM "$runner_monitor_pid" 2>/dev/null || true
    fi
    recorded="$(read_pid_file "$pid_file" 2>/dev/null || true)"
    [[ -n "$runner_server_pid" && "$recorded" == "$runner_server_pid" ]] && rm -f -- "$pid_file"
    recorded="$(read_pid_file "$runner_pid_file" 2>/dev/null || true)"
    [[ "$recorded" == "$$" ]] && rm -f -- "$runner_pid_file"
    release_runner_claim
}
run_server() {
    local server_status=0 state=0
    acquire_runner_claim
    trap runner_cleanup EXIT
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
    if resolve_server; then
        fail "validation server already runs as PID $managed_pid"
    else
        state=$?
    fi
    [[ "$state" -ne 2 ]] || fail "server PID file is unsafe; refusing runner startup"
    [[ "$state" -ne 3 ]] || rm -f -- "$pid_file"
    write_pid_file "$runner_pid_file" "$$"
    cd "$project_root"
    [[ -x "$server_binary" ]] || fail "server binary is missing: $server_binary"
    [[ -f "$dist_dir/index.html" ]] || fail "frontend is missing: $dist_dir/index.html"
    : >> "$server_log"
    chmod 600 "$server_log"
    "$server_binary" --bind "$listen_address" --dist-dir "$dist_dir" \
        --data-root "$data_dir" --basemap-token-file "$basemap_token_file" \
        >> "$server_log" 2>&1 &
    runner_server_pid=$!
    runner_server_start="$(process_start_time "$runner_server_pid")" || fail "cannot identify server process"
    write_pid_file "$pid_file" "$runner_server_pid"
    "$script_path" __monitor-log "$runner_server_pid" "$runner_server_start" &
    runner_monitor_pid=$!
    runner_monitor_start="$(process_start_time "$runner_monitor_pid")" || \
        fail "cannot identify log monitor process"
    set +e
    wait "$runner_server_pid"
    server_status=$?
    set -e
    if pid_is_managed_monitor "$runner_monitor_pid" "$runner_server_pid" "$runner_server_start" && \
        process_identity_matches "$runner_monitor_pid" "$runner_monitor_start"; then
        kill -TERM "$runner_monitor_pid" 2>/dev/null || true
    fi
    wait "$runner_monitor_pid" 2>/dev/null || true
    return "$server_status"
}

build_platform() {
    local revision="" state=0
    acquire_lock
    if resolve_server; then
        fail "stop PID $managed_pid before rebuilding"
    else
        state=$?
    fi
    [[ "$state" -ne 2 ]] || fail "PID file is unsafe; inspect $pid_file"
    [[ "$state" -ne 3 ]] || rm -f -- "$pid_file"
    acquire_runner_claim
    trap 'release_runner_claim; release_lock' EXIT
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
    [[ -x "$project_root/.tools/node/bin/node" ]] || fail "project Node.js is missing"
    [[ -x "$project_root/.tools/cargo/bin/cargo" ]] || fail "project Cargo is missing"
    VITE_VALIDATION_SERVER=1 "$project_root/scripts/node-project.sh" --prefix app run build
    "$project_root/scripts/cargo-project.sh" build --release --locked \
        -p hamheatmap-validation-server
    [[ -x "$server_binary" ]] || fail "build did not produce $server_binary"
    [[ -f "$dist_dir/index.html" ]] || fail "build did not produce $dist_dir/index.html"
    "$server_binary" --help > "$server_help"
    grep -Fq -- "--bind" "$server_help" || fail "server --help lacks --bind"
    grep -Fq -- "--dist-dir" "$server_help" || fail "server --help lacks --dist-dir"
    grep -Fq -- "--data-root" "$server_help" || fail "server --help lacks --data-root"
    grep -Fq -- "--basemap-token-file" "$server_help" || \
        fail "server --help lacks --basemap-token-file"
    revision="$(git -C "$project_root" rev-parse HEAD)"
    {
        printf 'revision=%s\n' "$revision"
        printf 'built_at=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
        printf 'frontend_mode=validation-server\nlisten=%s\n' "$listen_address"
        printf 'server_sha256='
        sha256sum "$server_binary" | awk '{print $1}'
    } > "$build_metadata.$$.tmp"
    mv -f -- "$build_metadata.$$.tmp" "$build_metadata"
    chmod 600 "$build_metadata" "$server_help"
    release_runner_claim
    echo "Validation platform build complete: $revision"
}

terminate_server() {
    local pid="$1" attempt=0 server_start=""
    pid_is_managed_server "$pid" || fail "refusing to signal unverified PID $pid"
    server_start="$(process_start_time "$pid")" || fail "cannot identify managed PID $pid"
    kill -TERM "$pid"
    for ((attempt = 0; attempt < 100; attempt++)); do
        pid_exists "$pid" || return 0
        process_identity_matches "$pid" "$server_start" || return 0
        sleep 0.2
    done
    pid_is_managed_server "$pid" || fail "PID $pid changed identity; refusing SIGKILL"
    process_identity_matches "$pid" "$server_start" || return 0
    kill -KILL "$pid"
    for ((attempt = 0; attempt < 25; attempt++)); do
        pid_exists "$pid" || return 0
        process_identity_matches "$pid" "$server_start" || return 0
        sleep 0.2
    done
    fail "managed PID $pid did not exit"
}

terminate_runner() {
    local pid="$1" expected_start="$2" expected_boot="$3" attempt=0
    pid_is_managed_runner "$pid" || fail "refusing to signal unverified runner PID $pid"
    process_identity_matches "$pid" "$expected_start" || \
        fail "runner PID $pid changed identity"
    runner_claim_owner_matches "$pid" "$expected_start" "$expected_boot" || \
        fail "runner PID $pid does not own the runner claim"
    kill -TERM "$pid"
    for ((attempt = 0; attempt < 100; attempt++)); do
        pid_exists "$pid" || return 0
        process_identity_matches "$pid" "$expected_start" || return 0
        sleep 0.2
    done
    echo "verified runner PID $pid did not exit after SIGTERM" >&2
    return 1
}

start_platform() {
    local runner_pid="" runner_start="" current_boot="" recorded_runner=""
    local attempt=0 state=0
    acquire_lock
    if resolve_server; then
        echo "Validation platform already runs as PID $managed_pid"
        http_health
        return
    else
        state=$?
    fi
    [[ "$state" -ne 2 ]] || fail "PID file is unsafe; refusing to start"
    [[ "$state" -ne 3 ]] || rm -f -- "$pid_file"
    runner_claim_owner_is_live && fail "a validation runner is already active"
    [[ -x "$server_binary" && -f "$dist_dir/index.html" ]] || \
        fail "run '$script_path build' first"
    rotate_launcher_log
    nohup setsid -- "$script_path" __run >> "$launcher_log" 2>&1 < /dev/null &
    runner_pid=$!
    runner_start="$(process_start_time "$runner_pid")" || fail "cannot identify launched runner"
    current_boot="$(read_boot_id)" || fail "cannot identify host boot"
    for ((attempt = 0; attempt < 60; attempt++)); do
        if resolve_server && http_health >/dev/null 2>&1; then
            echo "healthy pid=$managed_pid url=$health_url"
            return
        fi
        pid_exists "$runner_pid" || break
        sleep 1
    done
    recorded_runner="$(read_pid_file "$runner_pid_file" 2>/dev/null || true)"
    if pid_is_managed_runner "$runner_pid" && \
        process_identity_matches "$runner_pid" "$runner_start" && \
        runner_claim_owner_matches "$runner_pid" "$runner_start" "$current_boot"; then
        terminate_runner "$runner_pid" "$runner_start" "$current_boot" || return 1
    fi
    if [[ "$recorded_runner" == "$runner_pid" ]] && \
        runner_claim_owner_matches "$runner_pid" "$runner_start" "$current_boot" && \
        resolve_server; then
        terminate_server "$managed_pid"
    fi
    echo "startup health check failed" >&2
    tail -n 80 "$launcher_log" "$server_log" 2>/dev/null >&2 || true
    return 1
}

stop_platform() {
    local state=0 runner_pid="" runner_start="" current_boot="" attempt=0
    local server_was_running=0 claim_was_live=0
    acquire_lock
    if resolve_server; then
        server_was_running=1
        echo "Stopping PID $managed_pid"
        terminate_server "$managed_pid"
    else
        state=$?
        case "$state" in
            1) ;;
            2) fail "PID file points to an unrelated process; refusing to signal" ;;
            3) echo "Removing stale PID file"; rm -f -- "$pid_file" ;;
        esac
    fi
    runner_claim_owner_is_live && claim_was_live=1
    runner_pid="$(read_pid_file "$runner_pid_file" 2>/dev/null || true)"
    if [[ "$claim_was_live" -eq 1 && -z "$runner_pid" ]]; then
        fail "validation runner is still initializing; retry stop"
    fi
    if [[ -n "$runner_pid" ]]; then
        if pid_exists "$runner_pid"; then
            pid_is_managed_runner "$runner_pid" || fail "runner PID belongs to another process"
            runner_start="$(process_start_time "$runner_pid")" || fail "cannot identify runner PID"
            current_boot="$(read_boot_id)" || fail "cannot identify host boot"
            if [[ "$claim_was_live" -eq 1 ]]; then
                runner_claim_owner_matches "$runner_pid" "$runner_start" "$current_boot" || \
                    fail "runner PID does not own the active runner claim"
            elif [[ "$server_was_running" -ne 1 ]]; then
                fail "runner PID lacks a verified lifetime claim"
            fi
        fi
        for ((attempt = 0; attempt < 50; attempt++)); do
            pid_exists "$runner_pid" || break
            sleep 0.2
        done
        if pid_exists "$runner_pid"; then
            pid_is_managed_runner "$runner_pid" || fail "runner PID now belongs to another process"
            terminate_runner "$runner_pid" "$runner_start" "$current_boot" || \
                { echo "Runner PID $runner_pid is still finishing" >&2; return 1; }
        else
            rm -f -- "$runner_pid_file"
        fi
    fi
    if [[ "$server_was_running" -eq 0 && "$claim_was_live" -eq 0 && -z "$runner_pid" ]]; then
        echo "Validation platform is already stopped"
    fi
}

status_platform() {
    local state=0
    if resolve_server; then
        echo "running pid=$managed_pid bind=$listen_address"
        if http_health >/dev/null 2>&1; then
            echo "health=healthy url=$health_url"
            return
        fi
        echo "health=unhealthy url=$health_url" >&2
        return 1
    else
        state=$?
    fi
    case "$state" in
        1) echo "stopped"; return 3 ;;
        2) echo "unsafe-pid-state file=$pid_file" >&2; return 4 ;;
        3) echo "stopped stale-pid-file=$pid_file" >&2; return 3 ;;
    esac
}
health_platform() {
    resolve_server || { echo "validation platform is not safely running" >&2; return 1; }
    http_health
    printf '\n'
}

signal_probe_file=""
signal_trap_probe() {
    local destination="$1"
    path_is_safe_project_path "$destination" || fail "unsafe signal-probe path"
    [[ "$destination" == "$runtime_root"/self-test.*/* ]] || \
        fail "signal-probe path must stay in an isolated self-test directory"
    [[ ! -L "$destination" ]] || fail "signal-probe path is a symlink"
    signal_probe_file="$destination"
    trap 'printf "cleanup\n" >> "$signal_probe_file"' EXIT
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
    printf 'ready\n' > "$signal_probe_file"
    while :; do
        sleep 1
    done
    printf 'sentinel\n' >> "$signal_probe_file"
}

self_test_platform() {
    token_is_valid 0123456789abcdef || fail "valid basemap token was rejected"
    if token_is_valid short || token_is_valid "0123456789abcde/"; then
        fail "invalid basemap token was accepted"
    fi
    local test_root="$runtime_root/self-test.$$" saved_state_dir="$state_dir"
    local saved_lock_dir="$lock_dir" saved_lock_owner_file="$lock_owner_file"
    local saved_runner_claim_dir="$runner_claim_dir"
    local saved_runner_claim_owner_file="$runner_claim_owner_file"
    local child_pid="" child_start="" runner_pid="" command='while :; do sleep 1; done'
    local signal_pid="" signal_result="$test_root/signal-probe.txt"
    local saved_server_log="$server_log" saved_launcher_log="$launcher_log"
    local outside_root="" outside_target="" outside_sentinel=""
    local signal_status=0 attempt=0
    validate_managed_paths
    [[ ! -e "$test_root" && ! -L "$test_root" ]] || fail "self-test path already exists"
    mkdir -- "$test_root"
    chmod 700 "$test_root"

    state_dir="$test_root"
    lock_dir="$test_root/control.lock"
    lock_owner_file="$lock_dir/owner"
    mkdir -- "$lock_dir"
    if (acquire_lock) >/dev/null 2>&1; then
        fail "fresh ownerless control lock was incorrectly reclaimed"
    fi
    printf 'incomplete\n' > "$lock_dir/.owner.incomplete.tmp"
    chmod 600 "$lock_dir/.owner.incomplete.tmp"
    touch -d '10 seconds ago' "$lock_dir"
    lock_is_old_enough_to_recover || fail "aged ownerless lock was not recognized"
    acquire_lock
    lock_owner_is_live || fail "recovered lock owner identity is invalid"
    release_lock
    trap - EXIT HUP INT TERM
    [[ ! -e "$lock_dir" ]] || fail "self-test lock was not released"

    runner_claim_dir="$test_root/runner.claim"
    runner_claim_owner_file="$runner_claim_dir/owner"
    mkdir -- "$runner_claim_dir"
    if (acquire_runner_claim) >/dev/null 2>&1; then
        fail "fresh ownerless runner claim was incorrectly reclaimed"
    fi
    printf 'incomplete\n' > "$runner_claim_dir/.owner.incomplete.tmp"
    chmod 600 "$runner_claim_dir/.owner.incomplete.tmp"
    touch -d '10 seconds ago' "$runner_claim_dir"
    runner_claim_is_old_enough_to_recover || fail "aged runner claim was not recognized"
    acquire_runner_claim
    runner_claim_owner_is_live || fail "runner claim owner identity is invalid"
    if (acquire_runner_claim) >/dev/null 2>&1; then
        fail "second runner unexpectedly acquired a live claim"
    fi
    release_runner_claim
    [[ ! -e "$runner_claim_dir" ]] || fail "self-test runner claim was not released"

    ln -s -- /tmp "$test_root/escape"
    if path_is_safe_project_path "$test_root/escape/file"; then
        fail "symlinked managed path escaped containment"
    fi
    rm -f -- "$test_root/escape"

    mkdir -- "$test_root/logs"
    server_log="$test_root/logs/server.log"
    launcher_log="$test_root/logs/launcher.log"
    outside_root="$(mktemp -d /tmp/hamheatmap-validation-log-test.XXXXXX)"
    outside_target="$outside_root/rotated.log"
    outside_sentinel="$outside_root/sentinel"
    printf 'outside-sentinel\n' > "$outside_sentinel"
    printf 'server-log\n' > "$server_log"
    ln -s -- "$outside_target" "$server_log.1"
    if (rotate_log) >/dev/null 2>&1; then
        fail "server log rotation accepted a dangling backup symlink"
    fi
    [[ ! -e "$outside_target" && ! -L "$outside_target" ]] || \
        fail "server log rotation followed a dangling backup symlink"
    grep -Fqx -- outside-sentinel "$outside_sentinel" || \
        fail "server log rotation modified the external sentinel"
    rm -f -- "$server_log.1"

    printf 'launcher-log\n' > "$launcher_log"
    ln -s -- "$outside_target" "$launcher_log.1"
    if (rotate_launcher_log) >/dev/null 2>&1; then
        fail "launcher log rotation accepted a dangling backup symlink"
    fi
    [[ ! -e "$outside_target" && ! -L "$outside_target" ]] || \
        fail "launcher log rotation followed a dangling backup symlink"
    grep -Fqx -- outside-sentinel "$outside_sentinel" || \
        fail "launcher log rotation modified the external sentinel"
    rm -f -- "$launcher_log.1" "$server_log" "$launcher_log" "$outside_sentinel"
    rmdir -- "$outside_root" "$test_root/logs"
    server_log="$saved_server_log"
    launcher_log="$saved_launcher_log"

    bash -c "$command" hamheatmap-validation-self-test &
    child_pid=$!
    child_start="$(process_start_time "$child_pid")" || fail "cannot identify self-test process"
    argv_matches "$child_pid" bash -c "$command" hamheatmap-validation-self-test || \
        fail "exact argv matcher rejected the expected process"
    if argv_matches "$child_pid" bash -c "$command" hamheatmap-validation-self-test extra; then
        fail "exact argv matcher accepted an extra argument"
    fi
    if process_identity_matches "$child_pid" "$child_start"; then
        kill -TERM "$child_pid"
    fi
    wait "$child_pid" 2>/dev/null || true

    "$script_path" __signal-trap-probe "$signal_result" &
    signal_pid=$!
    for ((attempt = 0; attempt < 50; attempt++)); do
        [[ -f "$signal_result" ]] && grep -Fqx -- ready "$signal_result" && break
        pid_exists "$signal_pid" || fail "signal trap probe exited before becoming ready"
        sleep 0.1
    done
    [[ -f "$signal_result" ]] && grep -Fqx -- ready "$signal_result" || \
        fail "signal trap probe did not become ready"
    kill -TERM "$signal_pid"
    set +e
    wait "$signal_pid"
    signal_status=$?
    set -e
    [[ "$signal_status" -eq 143 ]] || fail "signal trap probe exited as $signal_status"
    grep -Fqx -- cleanup "$signal_result" || fail "EXIT cleanup did not run after TERM"
    if grep -Fqx -- sentinel "$signal_result"; then
        fail "signal trap probe continued after TERM"
    fi
    rm -f -- "$signal_result"

    runner_pid="$(read_pid_file "$runner_pid_file" 2>/dev/null || true)"
    if [[ -n "$runner_pid" ]]; then
        pid_is_managed_runner "$runner_pid" || fail "live runner failed strict argv verification"
    fi
    if resolve_server; then
        pid_is_managed_server "$managed_pid" || fail "live server failed strict argv verification"
    else
        case "$?" in
            1|3) ;;
            *) fail "live server PID file is unsafe" ;;
        esac
    fi

    state_dir="$saved_state_dir"
    lock_dir="$saved_lock_dir"
    lock_owner_file="$saved_lock_owner_file"
    runner_claim_dir="$saved_runner_claim_dir"
    runner_claim_owner_file="$saved_runner_claim_owner_file"
    rmdir -- "$test_root"
    echo "validation platform self-test passed"
}

basemap_token_temporary=""

cleanup_basemap_token_temporary() {
    if [[ -n "$basemap_token_temporary" &&
        "$basemap_token_temporary" == "$secrets_dir"/.tianditu.token.* &&
        -f "$basemap_token_temporary" && ! -L "$basemap_token_temporary" ]]; then
        rm -f -- "$basemap_token_temporary"
    fi
    basemap_token_temporary=""
}

token_is_valid() {
    local token="$1"
    [[ "${#token}" -ge 16 && "${#token}" -le 128 && "$token" =~ ^[[:alnum:]]+$ ]]
}

basemap_token_command() {
    local action="$1" token="" permissions="" owner=""
    local -a token_lines=()
    ensure_layout
    case "$action" in
        set)
            [[ -t 0 ]] || fail "basemap-token set requires an interactive terminal"
            read -r -s -p "TianDiTu token: " token
            printf '\n' >&2
            token_is_valid "$token" || {
                token=""
                fail "token must be 16-128 ASCII letters or digits"
            }
            acquire_lock
            path_is_safe_project_path "$basemap_token_file" || fail "unsafe basemap token path"
            [[ ! -L "$basemap_token_file" ]] || fail "basemap token path is a symlink"
            basemap_token_temporary="$(mktemp --tmpdir="$secrets_dir" .tianditu.token.XXXXXX)"
            trap 'cleanup_basemap_token_temporary; release_lock' EXIT
            printf '%s\n' "$token" > "$basemap_token_temporary"
            token=""
            chmod 600 "$basemap_token_temporary"
            mv -f -- "$basemap_token_temporary" "$basemap_token_file"
            basemap_token_temporary=""
            release_lock
            trap - EXIT HUP INT TERM
            echo "TianDiTu basemap token configured; restart the platform to apply it"
            ;;
        status)
            if [[ ! -e "$basemap_token_file" && ! -L "$basemap_token_file" ]]; then
                echo "TianDiTu basemap token is not configured"
                return
            fi
            path_is_safe_project_path "$basemap_token_file" || fail "unsafe basemap token path"
            [[ -f "$basemap_token_file" && ! -L "$basemap_token_file" ]] || \
                fail "basemap token path is not a regular file"
            permissions="$(stat -c '%a' "$basemap_token_file" 2>/dev/null)" || \
                fail "cannot inspect basemap token permissions"
            owner="$(stat -c '%u' "$basemap_token_file" 2>/dev/null)" || \
                fail "cannot inspect basemap token owner"
            mapfile -t token_lines < "$basemap_token_file"
            if [[ "$permissions" == "600" && "$owner" == "$(id -u)" &&
                "${#token_lines[@]}" -eq 1 ]] && token_is_valid "${token_lines[0]}"; then
                echo "TianDiTu basemap token is configured"
            else
                echo "TianDiTu basemap token file is invalid" >&2
                return 1
            fi
            token_lines=()
            ;;
        clear)
            acquire_lock
            path_is_safe_project_path "$basemap_token_file" || fail "unsafe basemap token path"
            [[ ! -L "$basemap_token_file" ]] || fail "basemap token path is a symlink"
            if [[ -e "$basemap_token_file" ]]; then
                [[ -f "$basemap_token_file" ]] || \
                    fail "basemap token path is not a regular file"
                [[ "$(stat -c '%u' "$basemap_token_file" 2>/dev/null)" == "$(id -u)" ]] || \
                    fail "basemap token file has an unexpected owner"
                rm -f -- "$basemap_token_file"
                echo "TianDiTu basemap token cleared; restart the platform to apply it"
            else
                echo "TianDiTu basemap token was already absent"
            fi
            release_lock
            trap - EXIT HUP INT TERM
            ;;
        *) fail "basemap-token requires set, status, or clear" ;;
    esac
}

case "${1:-}" in
    build) build_platform ;;
    start) start_platform ;;
    stop) stop_platform ;;
    status) status_platform ;;
    health) health_platform ;;
    basemap-token) [[ $# -eq 2 ]] || fail "basemap-token requires one action"
        basemap_token_command "$2" ;;
    self-test) self_test_platform ;;
    __run) run_server ;;
    __signal-trap-probe) [[ $# -eq 2 ]] || exit 2
        signal_trap_probe "$2" ;;
    __monitor-log) [[ "${2:-}" =~ ^[1-9][0-9]*$ && "${3:-}" =~ ^[0-9]+$ ]] || exit 2
        monitor_log "$2" "$3" ;;
    -h|--help|help) usage ;;
    *) usage >&2; exit 2 ;;
esac
