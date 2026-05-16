#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"

CONTAINER_RUNTIME="${CONTAINER_RUNTIME:-docker}"
CUSTOM_SETUP="${CUSTOM_SETUP:-}"

build_args=()
if [ -n "$CUSTOM_SETUP" ]; then
    build_args+=(--build-arg "CUSTOM_SETUP=$CUSTOM_SETUP")
fi

echo "--- journal ---"
$CONTAINER_RUNTIME build --target test "${build_args[@]}" -f "$ROOT/journal/Dockerfile" "$ROOT/journal"

echo "--- file-system ---"
$CONTAINER_RUNTIME build --target test "${build_args[@]}" -f "$ROOT/services/file-system/Dockerfile" "$ROOT/services/file-system"

echo "--- gateway ---"
$CONTAINER_RUNTIME build --target test "${build_args[@]}" -f "$ROOT/services/gateway/Dockerfile" "$ROOT/services/gateway"

echo "--- identity-provider ---"
sh "$ROOT/services/identity-provider/test-config.sh"

echo "--- explorer ---"
$CONTAINER_RUNTIME build --target test "${build_args[@]}" -f "$ROOT/services/explorer/Dockerfile" "$ROOT/services/explorer"

echo "--- workbench ---"
$CONTAINER_RUNTIME build --target test "${build_args[@]}" -f "$ROOT/services/workbench/Dockerfile" "$ROOT/services/workbench"

echo "--- integration ---"
CUSTOM_SETUP="$CUSTOM_SETUP" SECRET=password PORT=8192 SMB_PORT=1445 \
  "$ROOT/tests/api/local-compose.sh" smoke

echo "--- all passed ---"
