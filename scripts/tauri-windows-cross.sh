#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_home="$project_root/.tools/cargo"
rustup_home="$project_root/.tools/rustup"
node_bin="$project_root/.tools/node/bin"
llvm_root="$project_root/.tools/llvm-20.1.8"
cross_root="$project_root/.tools/cross"
nsis_bin="$project_root/.tools/nsis/bin"

required=(
    "$node_bin/npm"
    "$cargo_home/bin/cargo-xwin"
    "$llvm_root/bin/clang-cl"
    "$cross_root/usr/bin/proot"
    "$cross_root/usr/bin/makensis"
    "$project_root/scripts/cargo-xwin-static.sh"
    "$project_root/scripts/makensis-project.sh"
)
for path in "${required[@]}"; do
    if [[ ! -e "$path" ]]; then
        echo "project-local Windows build dependency is missing: $path" >&2
        exit 1
    fi
done

mkdir -p "$nsis_bin" "$project_root/.tools/npm-cache"
ln -sfn ../../../scripts/makensis-project.sh "$nsis_bin/makensis"

export CARGO_HOME="$cargo_home"
export RUSTUP_HOME="$rustup_home"
export XWIN_CACHE_DIR="$project_root/.tools/xwin-cache"
export npm_config_cache="$project_root/.tools/npm-cache"
export PATH="$node_bin:$nsis_bin:$llvm_root/bin:$cross_root/usr/bin:$cargo_home/bin:/usr/bin:/bin"
export LD_LIBRARY_PATH="$llvm_root/lib:$cross_root/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

cd "$project_root"
exec "$node_bin/npm" --prefix app run tauri -- \
    build \
    --runner "$project_root/scripts/cargo-xwin-static.sh" \
    --target x86_64-pc-windows-msvc \
    --bundles nsis \
    "$@"
