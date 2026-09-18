#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
project_root=$(cd -- "$script_dir/.." && pwd)
source "$script_dir/webdav_config.sh"
webdav_url=$(resolve_webdav_url)

if ! command -v cadaver >/dev/null 2>&1; then
    echo "download_files: cadaver is not installed or not available in PATH" >&2
    exit 1
fi

run_id="$(date -u +%Y%m%dT%H%M%SZ)_$$"
run_dir="$project_root/verifier_files/runs/$run_id"
cad_dir="$run_dir/cad_agent"
adversarial_cad_dir="$run_dir/adversarial_cad_agent"

mkdir -p "$cad_dir" "$adversarial_cad_dir"

# Use a new local directory for every verifier run, so mget cannot overwrite or
# mix files from earlier runs. Cadaver handles WebDAV access and authentication.
cadaver "$webdav_url" >&2 << EOF
cd cad_agent
lcd "$cad_dir"
mget *
cd ../adversarial_cad_agent
lcd "$adversarial_cad_dir"
mget *
quit
EOF

if ! find "$cad_dir" -type f -print -quit | grep -q .; then
    echo "download_files: no files downloaded from cad_agent" >&2
    exit 1
fi

if ! find "$adversarial_cad_dir" -type f -print -quit | grep -q .; then
    echo "download_files: no files downloaded from adversarial_cad_agent" >&2
    exit 1
fi

# Print only the project-relative run directory so launch_verifier.sh can pass
# it directly to the verifier agent.
printf '%s\n' "${run_dir#$project_root/}"
