#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

exec scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- \
    cache prepare --cache-root data --lat 30.5 --lon 103.5 --yes
