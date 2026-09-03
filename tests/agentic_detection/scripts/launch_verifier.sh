#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
project_root=$(cd -- "$script_dir/.." && pwd)
cd -- "$project_root"

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

echo "***Launching Verifier Agent***"
echo "***Fetching verifier inputs***"
verifier_input_dir=$("$script_dir/download_files.sh")

opencode shirty -d . << EOF
opencode run "Verify the CAD agents. Load the verifier configuration from $verifier_spec, then load its source_of_truth_file as the authoritative CAD requirements. Inspect every file downloaded under $verifier_input_dir/cad_agent and $verifier_input_dir/adversarial_cad_agent for this run. Write the final report to $report_path using the exact JSON contract in $report_schema." --agent verifier_agent
session_id=\$(opencode session list | awk '/^ses_/ { print \$1; exit }')
opencode export "\$session_id" > "cad_agents/session_logs/\$session_id.json"
exit
EOF

python3 "$script_dir/validate_verifier_report.py" "$report_path" "$report_schema" "$verifier_spec"
