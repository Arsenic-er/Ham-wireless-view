#!/usr/bin/env bash
# Ham Wireless View
# Project creator and lead developer: Arsenic-er
# SPDX-FileCopyrightText: 2026 Arsenic-er
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_HOME="$project_root/.tools/cargo"
export RUSTUP_HOME="$project_root/.tools/rustup"
export PATH="$CARGO_HOME/bin:$PATH"

cd "$project_root"
exec cargo "$@"

