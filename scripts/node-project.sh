#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
node_home="$project_root/.tools/node"

if [[ ! -x "$node_home/bin/node" ]]; then
    echo "project Node.js is missing at $node_home" >&2
    echo "install the version pinned in .node-version before running this command" >&2
    exit 1
fi

export PATH="$node_home/bin:/usr/bin:/bin"
export npm_config_cache="$project_root/.tools/npm-cache"

cd "$project_root"
exec npm "$@"

