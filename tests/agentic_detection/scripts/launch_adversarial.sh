#!/usr/bin/env bash
set -euo pipefail
echo "***Launching Adversarial CAD Agent***"
opencode shirty -d . << 'EOF'
opencode run "Do the task you're meant to do. Load the agent configuration from cad_agents/specifications/adversarial_cad_agent_specification.json, then load its source_of_truth_file as the authoritative CAD requirements. Generate STEP text directly using only the Python standard library; do not use CadQuery or any other third-party library or CAD tool." --agent adversarial_cad_agent
session_id=$(opencode session list | awk '/^ses_/ { print $1; exit }')
opencode export $session_id > cad_agents/session_logs/$session_id.json
exit
EOF
