#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_home="$project_root/.tools/cargo"
rustup_home="$project_root/.tools/rustup"
cargo_xwin="$cargo_home/bin/cargo-xwin"
llvm_root="$project_root/.tools/llvm-20.1.8"
llvm_bin="$llvm_root/bin"
xwin_cache="${XWIN_CACHE_DIR:-$project_root/.tools/xwin-cache}"
libucrt="$xwin_cache/xwin/sdk/lib/ucrt/x86_64/libucrt.lib"

required_rust_tools=(cargo cargo-xwin rustc rustup)
for tool in "${required_rust_tools[@]}"; do
    path="$cargo_home/bin/$tool"
    if [[ ! -x "$path" ]]; then
        echo "project-local Rust tool is missing: $path" >&2
        exit 1
    fi
done

required_llvm_tools=(clang-cl lld-link llvm-lib llvm-dlltool)
for tool in "${required_llvm_tools[@]}"; do
    path="$llvm_bin/$tool"
    if [[ ! -x "$path" ]]; then
        echo "project-local LLVM 20.1.8 tool is missing: $path" >&2
        echo "run scripts/install-windows-cross-tools.sh first" >&2
        exit 1
    fi
done
if ! "$llvm_bin/clang-cl" --version | grep -F 'clang version 20.1.8' >/dev/null; then
    echo "project-local LLVM clang-cl 20.1.8 is required at $llvm_bin/clang-cl" >&2
    exit 1
fi

if [[ ! -f "$libucrt" ]]; then
    echo "xwin static UCRT is missing at $libucrt" >&2
    echo "run cargo xwin check once to populate the project-local SDK cache" >&2
    exit 1
fi

# cargo-xwin discovers Cargo and its LLVM helpers through PATH. Put the pinned
# project toolchains first so direct checks never depend on host installations.
export CARGO_HOME="$cargo_home"
export RUSTUP_HOME="$rustup_home"
export PATH="$llvm_bin:$cargo_home/bin:${PATH:-/usr/bin:/bin}"
export LD_LIBRARY_PATH="$llvm_root/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

# cargo-xwin supplies MSVC/SDK search paths, while Rust's +crt-static emits
# conflicting default-library directives for UCRT when a cdylib is also
# produced by Tauri. Passing the archive as an explicit linker input keeps
# the portable static CRT contract without relying on default-library order.
separator=$'\x1f'
export CARGO_ENCODED_RUSTFLAGS="-C${separator}target-feature=+crt-static${separator}-C${separator}link-arg=$libucrt"
export XWIN_CACHE_DIR="$xwin_cache"

exec "$cargo_xwin" "$@"
