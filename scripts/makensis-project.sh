#!/usr/bin/env bash
# Ham Wireless View
# Project creator and lead developer: Arsenic-er
# SPDX-FileCopyrightText: 2026 Arsenic-er
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

script_path="$(readlink -f "${BASH_SOURCE[0]}")"
project_root="$(cd "$(dirname "$script_path")/.." && pwd)"
proot="$project_root/.tools/cross/usr/bin/proot"
makensis="$project_root/.tools/cross/usr/bin/makensis"
nsis_root="$project_root/.tools/cross/usr/share/nsis"
cross_lib="$project_root/.tools/cross/usr/lib/x86_64-linux-gnu"

for required in "$proot" "$makensis" "$nsis_root/Stubs/zlib-x86-unicode"; do
    if [[ ! -e "$required" ]]; then
        echo "project-local NSIS dependency is missing: $required" >&2
        exit 1
    fi
done

export LD_LIBRARY_PATH="$cross_lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$proot" -b "$nsis_root:/usr/share/nsis" "$makensis" "$@"
