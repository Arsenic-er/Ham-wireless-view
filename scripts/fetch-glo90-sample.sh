#!/usr/bin/env bash
# Ham Wireless View
# Project creator and lead developer: Arsenic-er
# SPDX-FileCopyrightText: 2026 Arsenic-er
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
data_root="$project_root/data"
target_dir="$data_root/dem/2021_1-aws-cog"
target="$target_dir/N30E103.tif"
partial="$target.partial"
url="https://copernicus-dem-90m.s3.amazonaws.com/Copernicus_DSM_COG_30_N30_00_E103_00_DEM/Copernicus_DSM_COG_30_N30_00_E103_00_DEM.tif"
expected_size=5169591
expected_sha256="2009766e446d6a33537e013dc0fa1944ce51857b0574182583e2d35eca2a8ab8"
quota_bytes=2500000000

mkdir -p "$target_dir"

if [[ -f "$target" ]]; then
    echo "$expected_sha256  $target" | sha256sum --check --status
    printf 'ready: %s\n' "$target"
    exit 0
fi

current_bytes="$(du -sb "$data_root" 2>/dev/null | awk '{print $1}')"
current_bytes="${current_bytes:-0}"
if (( current_bytes + expected_size > quota_bytes )); then
    printf 'quota exceeded: current=%s requested=%s cap=%s\n' \
        "$current_bytes" "$expected_size" "$quota_bytes" >&2
    exit 2
fi

curl --fail --location --proto =https --tlsv1.2 --output "$partial" "$url"
[[ "$(stat -c %s "$partial")" == "$expected_size" ]]
echo "$expected_sha256  $partial" | sha256sum --check --status
mv "$partial" "$target"
printf 'ready: %s\n' "$target"
