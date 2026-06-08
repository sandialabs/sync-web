#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"

if [ -z "${CONTAINER_RUNTIME+x}" ]; then
    if command -v docker >/dev/null 2>&1; then
        CONTAINER_RUNTIME="docker"
    elif command -v podman >/dev/null 2>&1; then
        CONTAINER_RUNTIME="podman"
    else
        echo "FAIL: neither docker nor podman is available" >&2
        exit 1
    fi
fi
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
COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-sync-test}" \
CUSTOM_SETUP="$CUSTOM_SETUP" SECRET=password HTTP_PORT=8192 HTTPS_PORT=8193 \
  "$ROOT/tests/api/local-compose.sh" smoke

echo "--- all passed ---"
