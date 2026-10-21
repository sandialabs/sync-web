#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
project_root=$(cd -- "$script_dir/.." && pwd)
cd -- "$project_root"

if (( $# != 1 )); then
    echo "Usage: launch_verifier.sh <workflow-start-epoch>" >&2
    exit 2
fi
workflow_start_epoch=$1

verifier_spec="cad_agents/specifications/verifier_specification.json"
report_schema="cad_agents/specifications/verifier_report.schema.json"
report_pattern=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["verifier_outputs"]["report_filename_pattern"])' "$verifier_spec")
report_timestamp=$(date -u +%Y%m%d_%H%M%S)
report_path=${report_pattern//\{timestamp\}/$report_timestamp}

case "$report_path" in
    verifier_files/*) ;;
    *)
        echo "launch_verifier: report path must be below verifier_files/" >&2
        exit 1
        ;;
esac

mkdir -p -- "$(dirname -- "$report_path")"
if [[ -e "$report_path" ]]; then
    echo "launch_verifier: refusing to overwrite existing report: $report_path" >&2
    exit 1
fi

echo "***Fetching verifier inputs***"
manifest_path=$("$script_dir/download_files.sh" "$workflow_start_epoch")
verifier_input_dir=$(dirname -- "$manifest_path")

opencode << EOF
opencode run "Verify the CAD artifacts. Load the verifier configuration from $verifier_spec and its source_of_truth_file. Load the retrieval manifest at $manifest_path. Inspect only the exact STEP and JSONL files named by that manifest under $verifier_input_dir, verify their local lengths and SHA-256 hashes against it, and preserve those manifest hashes in the artifact_inventory of the report. Report any integrity mismatch with both hashes in finding evidence. Write the final report to $report_path using the exact JSON contract in $report_schema." --agent verifier_agent
session_id=\$(opencode session list | awk '/^ses_/ { print \$1; exit }')
opencode export "\$session_id" > "cad_agents/session_logs/\$session_id.json"
exit
EOF

python3 "$script_dir/validate_verifier_report.py" "$report_path" "$report_schema" "$verifier_spec" "$manifest_path"
