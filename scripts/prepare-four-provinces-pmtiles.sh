#!/usr/bin/env bash
set -euo pipefail

export LC_ALL=C
umask 077

fail() {
    printf 'prepare four-provinces PMTiles: %s\n' "$*" >&2
    exit 1
}

for command_name in awk chmod curl df dirname du git grep id mkdir mktemp mv prlimit rm \
    readlink sha256sum stat tar; do
    command -v "$command_name" >/dev/null 2>&1 || \
        fail "required command is unavailable: $command_name"
done

script_source="${BASH_SOURCE[0]}"
if [[ "$script_source" != /* ]]; then
    script_source="$PWD/$script_source"
fi
[[ ! -L "$script_source" ]] || fail "the preparation script must not be a symlink"
script_path="$(readlink -f -- "$script_source")" || fail "cannot resolve script path"
project_root="$(readlink -f -- "$(dirname "$script_path")/..")" || \
    fail "cannot resolve project root"
git_root="$(git -C "$project_root" rev-parse --show-toplevel 2>/dev/null)" || \
    fail "project root is not a Git worktree"
git_root="$(readlink -f -- "$git_root")" || fail "cannot resolve Git worktree root"
[[ "$git_root" == "$project_root" ]] || \
    fail "script must run from the project Git worktree"
[[ "$script_path" == "$project_root/scripts/prepare-four-provinces-pmtiles.sh" ]] || \
    fail "unexpected script location"

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
        [[ "$resolved" == "$project_root" || "$resolved" == "$project_root/"* ]] || \
            return 1
    fi
}

ensure_private_directory() {
    local path="$1" owner=""
    path_is_safe_project_path "$path" || fail "unsafe or symlinked directory path: $path"
    mkdir -p -- "$path"
    path_is_safe_project_path "$path" || fail "unsafe or symlinked directory path: $path"
    [[ -d "$path" && ! -L "$path" ]] || fail "expected a regular directory: $path"
    owner="$(stat -c '%u' -- "$path")" || fail "cannot inspect directory owner: $path"
    [[ "$owner" == "$(id -u)" ]] || fail "directory has an unexpected owner: $path"
    chmod 700 -- "$path"
}

require_regular_owned_file() {
    local path="$1" owner="" links=""
    path_is_safe_project_path "$path" || return 1
    [[ -f "$path" && ! -L "$path" ]] || return 1
    owner="$(stat -c '%u' -- "$path" 2>/dev/null)" || return 1
    links="$(stat -c '%h' -- "$path" 2>/dev/null)" || return 1
    [[ "$owner" == "$(id -u)" && "$links" == "1" ]]
}

runtime_parent="$project_root/.runtime"
validation_runtime="$runtime_parent/validation-platform"
data_root="$validation_runtime/data"
basemap_dir="$data_root/basemap"
target="$basemap_dir/four-provinces.pmtiles"

cli_version="1.31.2"
cli_archive_size=17444324
cli_archive_sha256="3ed7dbf4ec2e6dfe5e25b6f70d1ffc932729f93c86db353bf514dd71010a312f"
cli_url="https://github.com/protomaps/go-pmtiles/releases/download/v1.31.2/go-pmtiles_1.31.2_Linux_x86_64.tar.gz"
source_url="https://build.protomaps.com/20260731.pmtiles"
bbox="107.5,18.0,125.5,33.5"
maxzoom=9
download_threads=4
overfetch=0.05
expected_size=33044072
expected_sha256="5bda49bf909a5b9fae931353edf5aea82ba35be9f8187128643b972eed4c87d0"
quota_bytes=2500000000
max_output_bytes=500000000
disk_margin_bytes=20000000

for path in "$runtime_parent" "$validation_runtime" "$data_root" "$basemap_dir" "$target"; do
    path_is_safe_project_path "$path" || fail "unsafe or symlinked managed path: $path"
done
ensure_private_directory "$runtime_parent"
ensure_private_directory "$validation_runtime"
ensure_private_directory "$data_root"
ensure_private_directory "$basemap_dir"

target_matches_expected() {
    local size="" digest=""
    require_regular_owned_file "$target" || return 1
    size="$(stat -c '%s' -- "$target" 2>/dev/null)" || return 1
    [[ "$size" == "$expected_size" ]] || return 1
    digest="$(sha256sum -- "$target" | awk '{print $1}')" || return 1
    [[ "$digest" == "$expected_sha256" ]]
}

if [[ -e "$target" || -L "$target" ]]; then
    require_regular_owned_file "$target" || \
        fail "existing target must be a regular, owned, non-symlink, single-link file"
    if target_matches_expected; then
        chmod 600 -- "$target"
        printf 'ready: %s (%s bytes, sha256 %s)\n' \
            "$target" "$expected_size" "$expected_sha256"
        exit 0
    fi
fi

quota_projection() {
    local current_bytes="" old_size=0 projected=""
    current_bytes="$(du -sb -- "$data_root" 2>/dev/null | awk 'NR == 1 {print $1}')" || \
        fail "cannot measure validation data root"
    [[ "$current_bytes" =~ ^[0-9]+$ ]] || fail "invalid validation data byte count"
    if [[ -e "$target" || -L "$target" ]]; then
        require_regular_owned_file "$target" || \
            fail "existing target changed to an unsafe file"
        old_size="$(stat -c '%s' -- "$target")" || \
            fail "cannot inspect existing target size"
    fi
    (( current_bytes >= old_size )) || fail "invalid quota accounting"
    projected=$((current_bytes - old_size + expected_size))
    if (( projected > quota_bytes )); then
        printf 'prepare four-provinces PMTiles: quota exceeded: current=%s old=%s requested=%s projected=%s cap=%s\n' \
            "$current_bytes" "$old_size" "$expected_size" "$projected" "$quota_bytes" >&2
        exit 2
    fi
}

quota_projection

available_bytes="$(df -B1 --output=avail "$runtime_parent" | awk 'NR == 2 {print $1}')" || \
    fail "cannot inspect available disk space"
[[ "$available_bytes" =~ ^[0-9]+$ ]] || fail "invalid available disk byte count"
required_work_bytes=$((cli_archive_size + max_output_bytes + disk_margin_bytes))
(( available_bytes >= required_work_bytes )) || \
    fail "insufficient free disk: available=$available_bytes required=$required_work_bytes"

work_root=""
cleanup() {
    local status="${1:-$?}"
    trap - EXIT HUP INT TERM
    if [[ -n "$work_root" ]]; then
        if [[ "$work_root" == "$runtime_parent/.prepare-four-provinces-pmtiles."* &&
            "$work_root" != "$runtime_parent/.prepare-four-provinces-pmtiles." &&
            -d "$work_root" && ! -L "$work_root" &&
            "$(stat -c '%u' -- "$work_root" 2>/dev/null || true)" == "$(id -u)" ]]; then
            chmod -R u+rwX -- "$work_root" 2>/dev/null || true
            rm -rf --one-file-system -- "$work_root"
        else
            printf 'prepare four-provinces PMTiles: refusing unsafe cleanup path: %s\n' \
                "$work_root" >&2
        fi
    fi
    exit "$status"
}
trap 'cleanup $?' EXIT
trap 'cleanup 129' HUP
trap 'cleanup 130' INT
trap 'cleanup 143' TERM

work_root="$(mktemp -d -p "$runtime_parent" .prepare-four-provinces-pmtiles.XXXXXXXX)" || \
    fail "cannot create project-local temporary directory"
path_is_safe_project_path "$work_root" || fail "unsafe temporary directory"
[[ -d "$work_root" && ! -L "$work_root" ]] || fail "invalid temporary directory"
[[ "$(stat -c '%u' -- "$work_root")" == "$(id -u)" ]] || \
    fail "temporary directory has an unexpected owner"
chmod 700 -- "$work_root"

archive_partial="$work_root/go-pmtiles.tar.gz.partial"
archive="$work_root/go-pmtiles.tar.gz"
tar_list="$work_root/tar.list"
tar_verbose="$work_root/tar.verbose"
tool_dir="$work_root/tool"
output_part="$work_root/four-provinces.pmtiles.part"
ensure_private_directory "$tool_dir"

printf 'downloading pinned PMTiles CLI v%s (%s bytes) inside project runtime...\n' \
    "$cli_version" "$cli_archive_size"
curl --fail --location --silent --show-error \
    --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --connect-timeout 10 --max-time 300 --retry 2 \
    --max-filesize "$cli_archive_size" \
    --output "$archive_partial" "$cli_url"
require_regular_owned_file "$archive_partial" || fail "invalid downloaded CLI archive"
[[ "$(stat -c '%s' -- "$archive_partial")" == "$cli_archive_size" ]] || \
    fail "CLI archive size mismatch"
echo "$cli_archive_sha256  $archive_partial" | sha256sum --check --status || \
    fail "CLI archive SHA-256 mismatch"
mv --no-target-directory -- "$archive_partial" "$archive"

tar -tzf "$archive" > "$tar_list" || fail "cannot list CLI archive"
mapfile -t archive_entries < "$tar_list"
expected_entries=("LICENSE" "README.md" "pmtiles")
[[ "${#archive_entries[@]}" == "${#expected_entries[@]}" ]] || \
    fail "unexpected CLI archive entry count"
for index in "${!expected_entries[@]}"; do
    entry="${archive_entries[$index]}"
    [[ "$entry" == "${expected_entries[$index]}" ]] || \
        fail "unexpected CLI archive entry: $entry"
    [[ "$entry" != /* && "$entry" != *'\\'* && "$entry" != *'//'* ]] || \
        fail "unsafe CLI archive entry path: $entry"
    IFS='/' read -r -a entry_parts <<< "$entry"
    for entry_part in "${entry_parts[@]}"; do
        [[ -n "$entry_part" && "$entry_part" != "." && "$entry_part" != ".." ]] || \
            fail "unsafe CLI archive entry path: $entry"
    done
done
tar -tvzf "$archive" > "$tar_verbose" || fail "cannot inspect CLI archive types"
[[ "$(awk 'substr($0, 1, 1) != "-" {bad=1} END {print bad+0}' "$tar_verbose")" == "0" ]] || \
    fail "CLI archive contains a non-regular entry"

tar --extract --gzip --file="$archive" --directory="$tool_dir" \
    --no-same-owner --no-same-permissions -- pmtiles
pmtiles="$tool_dir/pmtiles"
require_regular_owned_file "$pmtiles" || fail "extracted CLI is not a safe regular file"
chmod 700 -- "$pmtiles"
version_output="$("$pmtiles" version 2>&1)" || fail "cannot run pinned PMTiles CLI"
grep -Eq '(^|[^0-9])1[.]31[.]2([^0-9]|$)' <<< "$version_output" || \
    fail "unexpected PMTiles CLI version: $version_output"

printf 'extracting only HTTP ranges for bbox %s at z0-z%s (output capped at %s bytes)...\n' \
    "$bbox" "$maxzoom" "$max_output_bytes"
if ! prlimit --fsize="$max_output_bytes:$max_output_bytes" -- \
    "$pmtiles" extract "$source_url" "$output_part" \
        --bbox="$bbox" \
        --maxzoom="$maxzoom" \
        --download-threads="$download_threads" \
        --overfetch="$overfetch"; then
    fail "bounded PMTiles extraction failed"
fi

require_regular_owned_file "$output_part" || fail "extract output is not a safe regular file"
actual_size="$(stat -c '%s' -- "$output_part")" || fail "cannot inspect extract size"
(( actual_size <= max_output_bytes )) || fail "extract exceeded hard output limit"
[[ "$actual_size" == "$expected_size" ]] || \
    fail "extract size mismatch: expected $expected_size, got $actual_size"
actual_sha256="$(sha256sum -- "$output_part" | awk '{print $1}')" || \
    fail "cannot hash extract output"
[[ "$actual_sha256" == "$expected_sha256" ]] || \
    fail "extract SHA-256 mismatch: expected $expected_sha256, got $actual_sha256"
"$pmtiles" verify "$output_part" || fail "PMTiles verification failed"

quota_projection
for path in "$data_root" "$basemap_dir" "$target"; do
    path_is_safe_project_path "$path" || fail "managed path became unsafe: $path"
done
if [[ -e "$target" || -L "$target" ]]; then
    require_regular_owned_file "$target" || fail "existing target became unsafe"
fi
[[ "$(stat -c '%d' -- "$output_part")" == "$(stat -c '%d' -- "$basemap_dir")" ]] || \
    fail "temporary output and target directory are not on the same filesystem"
chmod 600 -- "$output_part"
mv --force --no-target-directory -- "$output_part" "$target"
target_matches_expected || fail "installed PMTiles archive failed final validation"
chmod 600 -- "$target"
chmod 700 -- "$runtime_parent" "$validation_runtime" "$data_root" "$basemap_dir"

printf 'ready: %s (%s bytes, sha256 %s)\n' \
    "$target" "$expected_size" "$expected_sha256"
