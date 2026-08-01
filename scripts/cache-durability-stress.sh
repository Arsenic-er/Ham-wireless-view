#!/usr/bin/env bash
# Ham Wireless View
# Project creator and lead developer: Arsenic-er
# SPDX-FileCopyrightText: 2026 Arsenic-er
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runtime_root="$project_root/.runtime/cache-stress"
minimum_available_bytes=4000000000

mkdir -p "$runtime_root"
trap 'rmdir "$runtime_root" 2>/dev/null || true' EXIT

if [[ -n "$(find "$runtime_root" -mindepth 1 -print -quit)" ]]; then
    printf 'cache stress runtime is not empty; refusing to touch existing data: %s\n' "$runtime_root" >&2
    exit 1
fi

available_bytes="$(df -B1 --output=avail "$project_root" | tail -n 1 | tr -d '[:space:]')"
if [[ ! "$available_bytes" =~ ^[0-9]+$ ]]; then
    printf 'could not determine available bytes for %s\n' "$project_root" >&2
    exit 1
fi
if (( available_bytes < minimum_available_bytes )); then
    printf 'cache stress requires at least %s available bytes; found %s\n' \
        "$minimum_available_bytes" "$available_bytes" >&2
    exit 1
fi

started_seconds="$SECONDS"
HAMHEATMAP_RUN_2_5GB_CACHE_STRESS=1 \
HAMHEATMAP_CACHE_STRESS_ROOT="$runtime_root" \
    "$project_root/scripts/cargo-project.sh" test \
        -p hamheatmap-cache \
        --lib \
        --locked \
        store::tests::production_hard_cap_crash_recovery_stress \
        -- \
        --ignored \
        --exact \
        --nocapture \
        --test-threads=1

if [[ -n "$(find "$runtime_root" -mindepth 1 -print -quit)" ]]; then
    printf 'cache stress left unexpected runtime data under %s\n' "$runtime_root" >&2
    exit 1
fi

printf 'cache durability stress passed in %s seconds; runtime data cleaned\n' "$((SECONDS - started_seconds))"
