#!/usr/bin/env bash
# Ham Wireless View
# Project creator and lead developer: Arsenic-er
# SPDX-FileCopyrightText: 2026 Arsenic-er
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version="$(tr -d '[:space:]' < "$project_root/.node-version")"
tools_root="$project_root/.tools"
install_root="$tools_root/node-v$version-linux-x64"
node_link="$tools_root/node"
archive="node-v$version-linux-x64.tar.xz"
download_root="$tools_root/node-download"

if [[ -x "$node_link/bin/node" ]]; then
    installed_version="$($node_link/bin/node --version)"
    if [[ "$installed_version" == "v$version" ]]; then
        echo "Node.js $installed_version is already installed in the project"
        exit 0
    fi
    echo "project Node.js link points to $installed_version, expected v$version" >&2
    exit 1
fi

if [[ -e "$node_link" || -e "$install_root" ]]; then
    echo "refusing to replace an existing project Node.js path" >&2
    exit 1
fi

mkdir -p "$download_root"
curl -fL --retry 3 \
    "https://nodejs.org/dist/v$version/SHASUMS256.txt" \
    -o "$download_root/SHASUMS256.txt"
curl -fL --retry 3 \
    "https://nodejs.org/dist/v$version/$archive" \
    -o "$download_root/$archive"

expected="$({ grep " $archive\$" "$download_root/SHASUMS256.txt" || true; } | awk '{print $1}')"
actual="$(sha256sum "$download_root/$archive" | awk '{print $1}')"
if [[ -z "$expected" || "$expected" != "$actual" ]]; then
    echo "Node.js archive SHA-256 verification failed" >&2
    exit 1
fi

tar -xJf "$download_root/$archive" -C "$tools_root"
ln -s "$(basename "$install_root")" "$node_link"
echo "installed Node.js $($node_link/bin/node --version) with verified SHA-256 $actual"
