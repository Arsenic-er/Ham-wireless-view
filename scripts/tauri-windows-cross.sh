#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_home="$project_root/.tools/cargo"
rustup_home="$project_root/.tools/rustup"
node_bin="$project_root/.tools/node/bin"
llvm_root="$project_root/.tools/llvm-20.1.8"
cross_root="$project_root/.tools/cross"
nsis_bin="$project_root/.tools/nsis/bin"
xwin_cache="$project_root/.tools/xwin-cache"
libucrt="$xwin_cache/xwin/sdk/lib/ucrt/x86_64/libucrt.lib"

export CARGO_HOME="$cargo_home"
export RUSTUP_HOME="$rustup_home"
export XWIN_CACHE_DIR="$xwin_cache"
export XWIN_VERSION="17"
export XWIN_SDK_VERSION="10.0.26100.0"
export XWIN_CRT_VERSION="14.44.35220"
export npm_config_cache="$project_root/.tools/npm-cache"
export PATH="$node_bin:$nsis_bin:$llvm_root/bin:$cross_root/usr/bin:$cargo_home/bin:/usr/bin:/bin"
export LD_LIBRARY_PATH="$llvm_root/lib:$cross_root/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

required_executables=(
    "$node_bin/node"
    "$node_bin/npm"
    "$cargo_home/bin/cargo-xwin"
    "$cargo_home/bin/rustup"
    "$llvm_root/bin/clang-cl"
    "$llvm_root/bin/lld-link"
    "$llvm_root/bin/llvm-lib"
    "$llvm_root/bin/llvm-objdump"
    "$cross_root/usr/bin/proot"
    "$cross_root/usr/bin/makensis"
    "$project_root/scripts/cargo-xwin-static.sh"
    "$project_root/scripts/makensis-project.sh"
)
for path in "${required_executables[@]}"; do
    if [[ ! -x "$path" ]]; then
        echo "project-local Windows build executable is missing: $path" >&2
        echo "run scripts/install-windows-cross-tools.sh first" >&2
        exit 1
    fi
done

required_files=(
    "$libucrt"
    "$xwin_cache/xwin/crt/lib/x86_64/libcmt.lib"
    "$cross_root/usr/share/nsis/Stubs/zlib-x86-unicode"
)
for path in "${required_files[@]}"; do
    if [[ ! -f "$path" ]]; then
        echo "project-local Windows build file is missing: $path" >&2
        echo "run scripts/install-windows-cross-tools.sh first" >&2
        exit 1
    fi
done

if ! "$cargo_home/bin/cargo-xwin" --version | grep -Fx 'cargo-xwin 0.23.0' >/dev/null; then
    echo "cargo-xwin 0.23.0 is required" >&2
    exit 1
fi
if ! "$llvm_root/bin/clang-cl" --version | grep -F 'clang version 20.1.8' >/dev/null; then
    echo "LLVM clang-cl 20.1.8 is required" >&2
    exit 1
fi
if ! "$cargo_home/bin/rustup" target list --installed | grep -Fx 'x86_64-pc-windows-msvc' >/dev/null; then
    echo "Rust target x86_64-pc-windows-msvc is missing" >&2
    echo "run scripts/install-windows-cross-tools.sh first" >&2
    exit 1
fi

mkdir -p "$nsis_bin" "$project_root/.tools/npm-cache"
ln -sfn ../../../scripts/makensis-project.sh "$nsis_bin/makensis"

printf '%s\n' 'Windows cross-build preflight:'
"$node_bin/node" --version
"$node_bin/npm" --version
"$cargo_home/bin/cargo-xwin" --version
"$llvm_root/bin/clang-cl" --version | sed -n '1,3p'
"$llvm_root/bin/lld-link" --version
"$llvm_root/bin/llvm-objdump" --version | sed -n '1,3p'
"$project_root/scripts/makensis-project.sh" -VERSION
printf '\n'
printf 'xwin: VS %s, SDK %s, CRT %s\n' \
    "$XWIN_VERSION" "$XWIN_SDK_VERSION" "$XWIN_CRT_VERSION"

cd "$project_root"
exec "$node_bin/npm" --prefix app run tauri -- \
    build \
    --runner "$project_root/scripts/cargo-xwin-static.sh" \
    --target x86_64-pc-windows-msvc \
    --bundles nsis \
    "$@"
