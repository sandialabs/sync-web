#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
project_root=$(cd -- "$script_dir/.." && pwd)
cd -- "$project_root"

# Create runtime directories
mkdir -p cad_agents/session_logs failed incoming logs pending uploaded verifier_files

# WebDAV Last-Modified timestamps have one-second resolution. Start the agents
# on the next whole second so a file already present cannot meet this boundary.
workflow_start_epoch=$(python3 -c 'import time; print(int(time.time()) + 1)')
python3 -c 'import sys, time; time.sleep(max(0, int(sys.argv[1]) - time.time()))' "$workflow_start_epoch"
echo "Workflow artifact boundary (UTC epoch): $workflow_start_epoch"

echo "***Launching CAD Agent***"
"$script_dir/launch_cad.sh" > /dev/null 2>&1 &
cad_pid=$!

echo "***Launching Adversarial CAD Agent***"
"$script_dir/launch_adversarial.sh" > /dev/null 2>&1 &
adversarial_cad_pid=$!

# Check each wait explicitly so errexit does not stop the workflow before both
# agents have finished and both exit statuses have been collected.
if wait "$cad_pid"; then
    cad_status=0
else
    cad_status=$?
fi

if wait "$adversarial_cad_pid"; then
    adversarial_cad_status=0
else
    adversarial_cad_status=$?
fi

if (( cad_status != 0 || adversarial_cad_status != 0 )); then
    echo "***CAD or Adversarial CAD agent failed. Not running Verifier agent.***"
    echo "CAD Agent exit status: $cad_status"
    echo "Adversarial CAD Agent exit status: $adversarial_cad_status"
    exit 1
fi

echo "***Launching Verifier Agent***"
"$script_dir/launch_verifier.sh" "$workflow_start_epoch"

echo "Run complete"
