#!/usr/bin/env bash
# Ham Wireless View
# Project creator and lead developer: Arsenic-er
# SPDX-FileCopyrightText: 2026 Arsenic-er
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tools_root="$project_root/.tools"
download_root="$tools_root/windows-download"
cargo_home="$tools_root/cargo"
rustup_home="$tools_root/rustup"
cargo_xwin="$cargo_home/bin/cargo-xwin"
xwin_cache="$tools_root/xwin-cache"
llvm_root="$tools_root/llvm-20.1.8"
cross_root="$tools_root/cross"
archive_root="$tools_root/archive"

cargo_xwin_version="0.23.0"
cargo_xwin_archive="cargo-xwin-v0.23.0.x86_64-unknown-linux-musl.tar.gz"
cargo_xwin_url="https://github.com/rust-cross/cargo-xwin/releases/download/v0.23.0/$cargo_xwin_archive"
cargo_xwin_sha256="74a216f64f10ea81c909f02d6b1a84cd0fda8de4c87ee52fe63ba76ab2392b75"
cargo_xwin_size="3831263"

llvm_archive="LLVM-20.1.8-Linux-X64.tar.xz"
llvm_url="https://github.com/llvm/llvm-project/releases/download/llvmorg-20.1.8/$llvm_archive"
llvm_sha256="1ead36b3dfcb774b57be530df42bec70ab2d239fbce9889447c7a29a4ddc1ae6"
llvm_size="2021269412"

nsis_archive="nsis_3.08-2_amd64.deb"
nsis_url="https://archive.ubuntu.com/ubuntu/pool/universe/n/nsis/$nsis_archive"
nsis_sha256="c36e6be757d7d4686c14cec3d7443746889fd48eced2a0a75f2bfecac3401176"
nsis_size="309208"

nsis_common_archive="nsis-common_3.08-2_all.deb"
nsis_common_url="https://archive.ubuntu.com/ubuntu/pool/universe/n/nsis/$nsis_common_archive"
nsis_common_sha256="a6c50d3fb74656da8ea0c64e4a8ef9a688f9475a19665d56c61c8a87c8521429"
nsis_common_size="971408"

proot_archive="proot_5.1.0-1.3_amd64.deb"
proot_url="https://archive.ubuntu.com/ubuntu/pool/universe/p/proot/$proot_archive"
proot_sha256="01a5d27c4ac16e184bdb356c9e69fa7d494325ac653c4cd64fae4c3fc63cdbbb"
proot_size="75316"

libtalloc_archive="libtalloc2_2.3.3-2build1_amd64.deb"
libtalloc_url="https://archive.ubuntu.com/ubuntu/pool/main/t/talloc/$libtalloc_archive"
libtalloc_sha256="0910059bb0329add8d13b502f5a10d18d5b3c5202fbbbe25ef4f6d58e7edfe6c"
libtalloc_size="25610"

p7zip_archive="p7zip_16.02+dfsg-8_amd64.deb"
p7zip_url="https://archive.ubuntu.com/ubuntu/pool/universe/p/p7zip/$p7zip_archive"
p7zip_sha256="37c809e01934f3d7cc1607ec242d7adca2a59147a147004c680f070ec845d5bd"
p7zip_size="363228"

p7zip_full_archive="p7zip-full_16.02+dfsg-8_amd64.deb"
p7zip_full_url="https://archive.ubuntu.com/ubuntu/pool/universe/p/p7zip/$p7zip_full_archive"
p7zip_full_sha256="8d84ac7b0fd3ca45a80c52a88ad2a44de8b680d3c0db22e11cf265d862c287bf"
p7zip_full_size="1185636"

rust_version="1.97.0"
rust_target="x86_64-pc-windows-msvc"
xwin_version="17"
xwin_sdk_version="10.0.26100.0"
xwin_crt_version="14.44.35220"

for command in curl sha256sum stat tar dpkg-deb grep awk df mktemp; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "required host command is missing: $command" >&2
        exit 1
    fi
done

mkdir -p "$tools_root" "$download_root"
install_tmp="$(mktemp -d "$tools_root/.windows-cross-install.XXXXXX")"
cleanup() {
    case "$install_tmp" in
        "$tools_root"/.windows-cross-install.*)
            rm -rf -- "$install_tmp"
            ;;
        *)
            echo "refusing to clean unexpected temporary path: $install_tmp" >&2
            ;;
    esac
}
trap cleanup EXIT INT TERM

verify_download() {
    local path="$1"
    local expected_sha="$2"
    local expected_size="$3"
    local actual_sha actual_size

    [[ -f "$path" ]] || return 1
    actual_size="$(stat -c '%s' "$path")"
    [[ "$actual_size" == "$expected_size" ]] || return 1
    actual_sha="$(sha256sum "$path")"
    actual_sha="${actual_sha%% *}"
    [[ "$actual_sha" == "$expected_sha" ]]
}

preserve_path() {
    local path="$1"
    local suffix candidate counter
    suffix="$(date -u +%Y%m%dT%H%M%SZ)"
    candidate="$path.invalid.$suffix"
    counter=0
    while [[ -e "$candidate" || -L "$candidate" ]]; do
        counter=$((counter + 1))
        candidate="$path.invalid.$suffix.$counter"
    done
    mv -- "$path" "$candidate"
    echo "preserved unexpected existing path as $candidate" >&2
}

download_verified() {
    local filename="$1"
    local url="$2"
    local expected_sha="$3"
    local expected_size="$4"
    local destination="$download_root/$filename"
    local partial="$install_tmp/$filename.partial"

    if verify_download "$destination" "$expected_sha" "$expected_size"; then
        echo "verified cached download: $destination"
        return
    fi

    if [[ -e "$destination" || -L "$destination" ]]; then
        preserve_path "$destination"
    fi

    echo "downloading $url"
    curl --fail --location --retry 3 --retry-delay 2 --output "$partial" "$url"
    if ! verify_download "$partial" "$expected_sha" "$expected_size"; then
        echo "download integrity check failed for $url" >&2
        exit 1
    fi
    mv -- "$partial" "$destination"
    echo "installed verified download: $destination"
}

require_free_kib() {
    local required_kib="$1"
    local available_kib
    available_kib="$(df -Pk "$tools_root" | awk 'NR == 2 { print $4 }')"
    if [[ -z "$available_kib" || "$available_kib" -lt "$required_kib" ]]; then
        echo "insufficient free space in $tools_root: need at least ${required_kib} KiB" >&2
        exit 1
    fi
}

cargo_xwin_valid() {
    [[ -x "$cargo_xwin" ]] &&
        "$cargo_xwin" --version 2>/dev/null | grep -Fx "cargo-xwin $cargo_xwin_version" >/dev/null
}

llvm_valid_at() {
    local root="$1"
    [[ -x "$root/bin/clang-cl" ]] &&
        [[ -x "$root/bin/lld-link" ]] &&
        [[ -x "$root/bin/llvm-lib" ]] &&
        [[ -x "$root/bin/llvm-objdump" ]] &&
        "$root/bin/clang-cl" --version 2>/dev/null | grep -F 'clang version 20.1.8' >/dev/null
}

cross_valid_at() {
    local root="$1"
    local lib="$root/usr/lib/x86_64-linux-gnu"
    [[ -x "$root/usr/bin/proot" ]] &&
        [[ -x "$root/usr/bin/makensis" ]] &&
        [[ -f "$root/usr/share/nsis/Stubs/zlib-x86-unicode" ]] &&
        [[ -f "$lib/libtalloc.so.2" ]] &&
        env LD_LIBRARY_PATH="$lib" "$root/usr/bin/proot" --version 2>/dev/null | grep -F '5.1.0' >/dev/null &&
        env LD_LIBRARY_PATH="$lib" "$root/usr/bin/proot" \
            -b "$root/usr/share/nsis:/usr/share/nsis" \
            "$root/usr/bin/makensis" -VERSION 2>/dev/null | grep -Fx 'v3.08-2' >/dev/null
}

archive_valid_at() {
    local root="$1"
    [[ -x "$root/usr/lib/p7zip/7z" ]] &&
        "$root/usr/lib/p7zip/7z" i 2>/dev/null | grep -F 'p7zip Version 16.02' >/dev/null
}

xwin_cache_valid() {
    local crt_header="$xwin_cache/xwin/crt/include/crtversion.h"
    [[ -f "$xwin_cache/xwin/sdk/lib/ucrt/x86_64/libucrt.lib" ]] &&
        [[ -f "$xwin_cache/xwin/crt/lib/x86_64/libcmt.lib" ]] &&
        [[ -e "$xwin_cache/xwin/sdk/include/10.0.26100" ]] &&
        [[ -e "$xwin_cache/xwin/sdk/lib/10.0.26100" ]] &&
        [[ -f "$crt_header" ]] &&
        grep -Eq '^#define _VC_CRT_MAJOR_VERSION[[:space:]]+14' "$crt_header" &&
        grep -Eq '^#define _VC_CRT_MINOR_VERSION[[:space:]]+44' "$crt_header" &&
        grep -Eq '^#define _VC_CRT_BUILD_VERSION[[:space:]]+35220' "$crt_header"
}

if cargo_xwin_valid; then
    echo "cargo-xwin $cargo_xwin_version is already installed"
else
    download_verified "$cargo_xwin_archive" "$cargo_xwin_url" \
        "$cargo_xwin_sha256" "$cargo_xwin_size"
    mkdir -p "$install_tmp/cargo-xwin"
    tar -xzf "$download_root/$cargo_xwin_archive" -C "$install_tmp/cargo-xwin"
    chmod 0755 "$install_tmp/cargo-xwin/cargo-xwin"
    if ! "$install_tmp/cargo-xwin/cargo-xwin" --version | grep -Fx "cargo-xwin $cargo_xwin_version" >/dev/null; then
        echo "extracted cargo-xwin failed version validation" >&2
        exit 1
    fi
    mkdir -p "$cargo_home/bin"
    if [[ -e "$cargo_xwin" || -L "$cargo_xwin" ]]; then
        preserve_path "$cargo_xwin"
    fi
    mv -- "$install_tmp/cargo-xwin/cargo-xwin" "$cargo_xwin"
fi

if [[ ! -x "$cargo_home/bin/rustup" ]]; then
    echo "project-local rustup is missing; restore Rust $rust_version first" >&2
    exit 1
fi
export CARGO_HOME="$cargo_home"
export RUSTUP_HOME="$rustup_home"
export PATH="$cargo_home/bin:/usr/bin:/bin"

if rustup target list --installed | grep -Fx "$rust_target" >/dev/null; then
    echo "Rust target $rust_target is already installed"
else
    rustup target add --toolchain "$rust_version" "$rust_target"
fi
if ! rustup target list --installed | grep -Fx "$rust_target" >/dev/null; then
    echo "Rust target installation failed: $rust_target" >&2
    exit 1
fi

export XWIN_CACHE_DIR="$xwin_cache"
export XWIN_VERSION="$xwin_version"
export XWIN_SDK_VERSION="$xwin_sdk_version"
export XWIN_CRT_VERSION="$xwin_crt_version"
if xwin_cache_valid; then
    echo "xwin SDK/CRT cache is already valid"
else
    "$cargo_xwin" cache xwin --update
    if ! xwin_cache_valid; then
        echo "xwin SDK/CRT cache failed validation" >&2
        exit 1
    fi
fi

if llvm_valid_at "$llvm_root"; then
    echo "LLVM 20.1.8 is already installed"
else
    require_free_kib 16000000
    download_verified "$llvm_archive" "$llvm_url" "$llvm_sha256" "$llvm_size"
    mkdir -p "$install_tmp/llvm"
    tar -xJf "$download_root/$llvm_archive" \
        --strip-components=1 -C "$install_tmp/llvm"
    if ! llvm_valid_at "$install_tmp/llvm"; then
        echo "extracted LLVM failed validation" >&2
        exit 1
    fi
    if [[ -e "$llvm_root" || -L "$llvm_root" ]]; then
        preserve_path "$llvm_root"
    fi
    mv -- "$install_tmp/llvm" "$llvm_root"
fi

if cross_valid_at "$cross_root"; then
    echo "NSIS 3.08-2 and proot 5.1.0 are already installed"
else
    download_verified "$nsis_archive" "$nsis_url" "$nsis_sha256" "$nsis_size"
    download_verified "$nsis_common_archive" "$nsis_common_url" \
        "$nsis_common_sha256" "$nsis_common_size"
    download_verified "$proot_archive" "$proot_url" "$proot_sha256" "$proot_size"
    download_verified "$libtalloc_archive" "$libtalloc_url" \
        "$libtalloc_sha256" "$libtalloc_size"
    mkdir -p "$install_tmp/cross"
    for archive in \
        "$nsis_common_archive" \
        "$nsis_archive" \
        "$libtalloc_archive" \
        "$proot_archive"; do
        dpkg-deb -x "$download_root/$archive" "$install_tmp/cross"
    done
    if ! cross_valid_at "$install_tmp/cross"; then
        echo "extracted NSIS/proot toolchain failed validation" >&2
        exit 1
    fi
    if [[ -e "$cross_root" || -L "$cross_root" ]]; then
        preserve_path "$cross_root"
    fi
    mv -- "$install_tmp/cross" "$cross_root"
fi

if archive_valid_at "$archive_root"; then
    echo "p7zip 16.02 is already installed"
else
    download_verified "$p7zip_archive" "$p7zip_url" "$p7zip_sha256" "$p7zip_size"
    download_verified "$p7zip_full_archive" "$p7zip_full_url" \
        "$p7zip_full_sha256" "$p7zip_full_size"
    mkdir -p "$install_tmp/archive"
    dpkg-deb -x "$download_root/$p7zip_archive" "$install_tmp/archive"
    dpkg-deb -x "$download_root/$p7zip_full_archive" "$install_tmp/archive"
    if ! archive_valid_at "$install_tmp/archive"; then
        echo "extracted p7zip toolchain failed validation" >&2
        exit 1
    fi
    if [[ -e "$archive_root" || -L "$archive_root" ]]; then
        preserve_path "$archive_root"
    fi
    mv -- "$install_tmp/archive" "$archive_root"
fi

printf '%s\n' 'Windows cross-build tools are ready:'
"$cargo_xwin" --version
"$llvm_root/bin/clang-cl" --version | sed -n '1,3p'
"$llvm_root/bin/lld-link" --version
"$llvm_root/bin/llvm-objdump" --version | sed -n '1,3p'
env LD_LIBRARY_PATH="$cross_root/usr/lib/x86_64-linux-gnu" \
    "$cross_root/usr/bin/proot" --version | tail -n 1
env LD_LIBRARY_PATH="$cross_root/usr/lib/x86_64-linux-gnu" \
    "$cross_root/usr/bin/proot" \
    -b "$cross_root/usr/share/nsis:/usr/share/nsis" \
    "$cross_root/usr/bin/makensis" -VERSION
"$archive_root/usr/lib/p7zip/7z" i | sed -n '1,4p'
printf 'Rust target: %s\n' "$rust_target"
printf 'xwin: VS %s, SDK %s, CRT %s\n' \
    "$xwin_version" "$xwin_sdk_version" "$xwin_crt_version"
du -sh "$cargo_xwin" "$xwin_cache" "$llvm_root" "$cross_root" "$archive_root"
