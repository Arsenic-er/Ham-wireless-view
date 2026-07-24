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
dist_dir="$project_root/app/dist"
server_binary="$project_root/target/release/hamheatmap-validation-server"
pid_file="$state_dir/server.pid"
runner_pid_file="$state_dir/runner.pid"
lock_dir="$state_dir/control.lock"
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
EOF
}

fail() { echo "validation platform: $*" >&2; exit 1; }

ensure_layout() {
    mkdir -p "$state_dir" "$log_dir" "$data_dir"
    chmod 700 "$runtime_root" "$state_dir" "$log_dir" "$data_dir"
}

lock_held=0
release_lock() {
    if [[ "$lock_held" -eq 1 ]]; then
        rmdir -- "$lock_dir" 2>/dev/null || true
        lock_held=0
    fi
}
acquire_lock() {
    ensure_layout
    mkdir -- "$lock_dir" 2>/dev/null || fail "another build/start/stop operation is active"
    lock_held=1
    trap release_lock EXIT HUP INT TERM
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
    local file="$1" value="$2" temporary="$1.$$.tmp"
    printf '%s\n' "$value" > "$temporary"
    chmod 600 "$temporary"
    mv -f -- "$temporary" "$file"
}
pid_exists() { kill -0 "$1" 2>/dev/null; }
cmdline_has() { tr '\0' '\n' < "/proc/$1/cmdline" 2>/dev/null | grep -Fqx -- "$2"; }

pid_is_managed_server() {
    local pid="$1" executable=""
    [[ "$pid" =~ ^[1-9][0-9]*$ && -d "/proc/$pid" ]] || return 1
    [[ "$(stat -c '%u' "/proc/$pid" 2>/dev/null)" == "$(id -u)" ]] || return 1
    executable="$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)"
    [[ "$executable" == "$server_binary" ]] || return 1
    cmdline_has "$pid" "--bind" && cmdline_has "$pid" "$listen_address" || return 1
    cmdline_has "$pid" "--dist-dir" && cmdline_has "$pid" "$dist_dir" || return 1
    cmdline_has "$pid" "--data-root" && cmdline_has "$pid" "$data_dir" || return 1
}
pid_is_managed_runner() {
    local pid="$1"
    [[ "$pid" =~ ^[1-9][0-9]*$ && -d "/proc/$pid" ]] || return 1
    [[ "$(stat -c '%u' "/proc/$pid" 2>/dev/null)" == "$(id -u)" ]] || return 1
    cmdline_has "$pid" "$script_path" && cmdline_has "$pid" "__run"
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
    local index=0
    rm -f -- "$server_log.$log_backups"
    for ((index = log_backups - 1; index >= 1; index--)); do
        [[ -f "$server_log.$index" ]] && mv -f -- \
            "$server_log.$index" "$server_log.$((index + 1))"
    done
    [[ -f "$server_log" ]] && cp -f -- "$server_log" "$server_log.1"
    : > "$server_log"
    chmod 600 "$server_log"
}
monitor_log() {
    local server_pid="$1" size=0
    while pid_is_managed_server "$server_pid"; do
        sleep 5
        [[ -f "$server_log" ]] || continue
        size="$(wc -c < "$server_log")"
        (( size < log_max_bytes )) || rotate_log
    done
}

runner_server_pid=""
runner_monitor_pid=""
runner_cleanup() {
    local recorded=""
    if [[ -n "$runner_server_pid" ]] && pid_is_managed_server "$runner_server_pid"; then
        kill -TERM "$runner_server_pid" 2>/dev/null || true
    fi
    [[ -n "$runner_monitor_pid" ]] && kill -TERM "$runner_monitor_pid" 2>/dev/null || true
    recorded="$(read_pid_file "$pid_file" 2>/dev/null || true)"
    [[ -n "$runner_server_pid" && "$recorded" == "$runner_server_pid" ]] && rm -f -- "$pid_file"
    recorded="$(read_pid_file "$runner_pid_file" 2>/dev/null || true)"
    [[ "$recorded" == "$$" ]] && rm -f -- "$runner_pid_file"
}
run_server() {
    local server_status=0
    ensure_layout
    cd "$project_root"
    [[ -x "$server_binary" ]] || fail "server binary is missing: $server_binary"
    [[ -f "$dist_dir/index.html" ]] || fail "frontend is missing: $dist_dir/index.html"
    : >> "$server_log"
    chmod 600 "$server_log"
    trap runner_cleanup EXIT HUP INT TERM
    "$server_binary" --bind "$listen_address" --dist-dir "$dist_dir" \
        --data-root "$data_dir" >> "$server_log" 2>&1 &
    runner_server_pid=$!
    write_pid_file "$pid_file" "$runner_server_pid"
    "$script_path" __monitor-log "$runner_server_pid" &
    runner_monitor_pid=$!
    set +e
    wait "$runner_server_pid"
    server_status=$?
    set -e
    kill -TERM "$runner_monitor_pid" 2>/dev/null || true
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
    echo "Validation platform build complete: $revision"
}

terminate_server() {
    local pid="$1" attempt=0
    pid_is_managed_server "$pid" || fail "refusing to signal unverified PID $pid"
    kill -TERM "$pid"
    for ((attempt = 0; attempt < 100; attempt++)); do
        pid_exists "$pid" || return 0
        sleep 0.2
    done
    pid_is_managed_server "$pid" || fail "PID $pid changed identity; refusing SIGKILL"
    kill -KILL "$pid"
    for ((attempt = 0; attempt < 25; attempt++)); do
        pid_exists "$pid" || return 0
        sleep 0.2
    done
    fail "managed PID $pid did not exit"
}

start_platform() {
    local runner_pid="" attempt=0 state=0
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
    [[ -x "$server_binary" && -f "$dist_dir/index.html" ]] || \
        fail "run '$script_path build' first"
    if [[ -f "$launcher_log" && "$(wc -c < "$launcher_log")" -ge 1000000 ]]; then
        mv -f -- "$launcher_log" "$launcher_log.1"
    fi
    rm -f -- "$runner_pid_file"
    nohup setsid -- "$script_path" __run >> "$launcher_log" 2>&1 < /dev/null &
    runner_pid=$!
    write_pid_file "$runner_pid_file" "$runner_pid"
    for ((attempt = 0; attempt < 60; attempt++)); do
        if resolve_server && http_health >/dev/null 2>&1; then
            echo "healthy pid=$managed_pid url=$health_url"
            return
        fi
        pid_exists "$runner_pid" || break
        sleep 1
    done
    if resolve_server; then terminate_server "$managed_pid"; fi
    echo "startup health check failed" >&2
    tail -n 80 "$launcher_log" "$server_log" 2>/dev/null >&2 || true
    return 1
}

stop_platform() {
    local state=0 runner_pid="" attempt=0
    acquire_lock
    if resolve_server; then
        echo "Stopping PID $managed_pid"
        terminate_server "$managed_pid"
    else
        state=$?
        case "$state" in
            1) echo "Validation platform is already stopped" ;;
            2) fail "PID file points to an unrelated process; refusing to signal" ;;
            3) echo "Removing stale PID file"; rm -f -- "$pid_file" ;;
        esac
    fi
    runner_pid="$(read_pid_file "$runner_pid_file" 2>/dev/null || true)"
    if [[ -n "$runner_pid" ]]; then
        for ((attempt = 0; attempt < 50; attempt++)); do
            pid_exists "$runner_pid" || break
            sleep 0.2
        done
        if pid_exists "$runner_pid"; then
            pid_is_managed_runner "$runner_pid" || fail "runner PID now belongs to another process"
            echo "Runner PID $runner_pid is still finishing; it was not force-killed" >&2
        else
            rm -f -- "$runner_pid_file"
        fi
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

case "${1:-}" in
    build) build_platform ;;
    start) start_platform ;;
    stop) stop_platform ;;
    status) status_platform ;;
    health) health_platform ;;
    __run) run_server ;;
    __monitor-log) [[ "${2:-}" =~ ^[1-9][0-9]*$ ]] || exit 2; monitor_log "$2" ;;
    -h|--help|help) usage ;;
    *) usage >&2; exit 2 ;;
esac
