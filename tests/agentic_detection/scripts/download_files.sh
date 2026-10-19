#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
project_root=$(cd -- "$script_dir/.." && pwd)
source "$script_dir/webdav_config.sh"

if (( $# != 1 )); then
    echo "Usage: download_files.sh <workflow-start-epoch>" >&2
    exit 2
fi

webdav_url=$(resolve_webdav_url)
cd -- "$project_root"
python3 "$script_dir/fetch_verifier_inputs.py" \
    --spec cad_agents/specifications/verifier_specification.json \
    --base-url "$webdav_url" \
    --not-before-epoch "$1"
