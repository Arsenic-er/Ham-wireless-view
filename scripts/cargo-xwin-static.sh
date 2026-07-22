#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_xwin="$project_root/.tools/cargo/bin/cargo-xwin"
xwin_cache="${XWIN_CACHE_DIR:-$project_root/.tools/xwin-cache}"
libucrt="$xwin_cache/xwin/sdk/lib/ucrt/x86_64/libucrt.lib"

if [[ ! -x "$cargo_xwin" ]]; then
    echo "cargo-xwin is missing at $cargo_xwin" >&2
    exit 1
fi
if [[ ! -f "$libucrt" ]]; then
    echo "xwin static UCRT is missing at $libucrt" >&2
    echo "run cargo xwin check once to populate the project-local SDK cache" >&2
    exit 1
fi

# cargo-xwin supplies MSVC/SDK search paths, while Rust's +crt-static emits
# conflicting default-library directives for UCRT when a cdylib is also
# produced by Tauri. Passing the archive as an explicit linker input keeps
# the portable static CRT contract without relying on default-library order.
separator=$'\x1f'
export CARGO_ENCODED_RUSTFLAGS="-C${separator}target-feature=+crt-static${separator}-C${separator}link-arg=$libucrt"
export XWIN_CACHE_DIR="$xwin_cache"

exec "$cargo_xwin" "$@"
