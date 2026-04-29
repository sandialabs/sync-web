#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"

CUSTOM_SETUP="${CUSTOM_SETUP:-}"

build_args=()
if [ -n "$CUSTOM_SETUP" ]; then
    build_args+=(--build-arg "CUSTOM_SETUP=$CUSTOM_SETUP")
fi

echo "--- journal ---"
docker build --target test "${build_args[@]}" -f "$ROOT/journal/Dockerfile" "$ROOT/journal"

echo "--- file-system ---"
docker build --target test "${build_args[@]}" -f "$ROOT/services/file-system/Dockerfile" "$ROOT/services/file-system"

echo "--- gateway ---"
docker build --target build "${build_args[@]}" -f "$ROOT/services/gateway/Dockerfile" "$ROOT/services/gateway"

echo "--- explorer ---"
docker build --target test "${build_args[@]}" -f "$ROOT/services/explorer/Dockerfile" "$ROOT/services/explorer"

echo "--- workbench ---"
docker build --target test "${build_args[@]}" -f "$ROOT/services/workbench/Dockerfile" "$ROOT/services/workbench"

echo "--- integration ---"
CUSTOM_SETUP="$CUSTOM_SETUP" SECRET=password PORT=8192 SMB_PORT=1445 \
  "$ROOT/tests/api/local-compose.sh" smoke

echo "--- all passed ---"
