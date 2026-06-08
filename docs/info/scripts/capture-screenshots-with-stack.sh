#!/bin/sh
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
INFO_DIR="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="${SYNC_REPO_ROOT:-}"
if [ -z "$REPO_ROOT" ]; then
    REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
fi
COMPOSE_FILE="$REPO_ROOT/deploy/compose/general/compose.yaml"

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

if [ -z "${CONTAINER_COMPOSE+x}" ]; then
    case "$CONTAINER_RUNTIME" in
        docker) CONTAINER_COMPOSE="docker compose" ;;
        podman)
            if podman compose version >/dev/null 2>&1; then
                CONTAINER_COMPOSE="podman compose"
            elif command -v podman-compose >/dev/null 2>&1; then
                CONTAINER_COMPOSE="podman-compose"
            else
                CONTAINER_COMPOSE="podman compose"
            fi
            ;;
        *) CONTAINER_COMPOSE="$CONTAINER_RUNTIME compose" ;;
    esac
fi

COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-sync-docs-screenshots}"
HTTP_PORT="${HTTP_PORT:-8192}"
HTTPS_PORT="${HTTPS_PORT:-8193}"
SECRET="${SECRET:-password}"
PERIOD="${PERIOD:-2}"
WINDOW="${WINDOW:-128}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-60}"
LOCAL_COMPOSE_FORCE_HTTP="${LOCAL_COMPOSE_FORCE_HTTP:-1}"

if [ "$LOCAL_COMPOSE_FORCE_HTTP" = "1" ]; then
    TLS_STUB_DIR="/tmp/sync-docs-screenshots"
    TLS_CERT_HOST_PATH="$TLS_STUB_DIR/http-only.crt"
    TLS_KEY_HOST_PATH="$TLS_STUB_DIR/http-only.key"
    mkdir -p "$TLS_STUB_DIR"
    printf "HTTP-only docs screenshot placeholder cert.\n" > "$TLS_CERT_HOST_PATH"
    printf "HTTP-only docs screenshot placeholder key.\n" > "$TLS_KEY_HOST_PATH"
    export TLS_CERT_HOST_PATH TLS_KEY_HOST_PATH
fi

export COMPOSE_PROJECT_NAME HTTP_PORT HTTPS_PORT SECRET PERIOD WINDOW

dc() {
    $CONTAINER_COMPOSE -f "$COMPOSE_FILE" "$@"
}

cleanup() {
    set +e
    dc down --remove-orphans >/dev/null 2>&1
}

wait_for_http() {
    url="$1"
    elapsed=0
    while [ "$elapsed" -lt "$TIMEOUT_SECONDS" ]; do
        if curl -fsS --connect-timeout 2 --max-time 5 "$url" >/dev/null 2>&1; then
            return 0
        fi
        sleep 2
        elapsed=$((elapsed + 2))
    done
    echo "Timed out waiting for $url" >&2
    return 1
}

trap cleanup EXIT INT TERM

echo "Starting compose project '$COMPOSE_PROJECT_NAME' on HTTP $HTTP_PORT for screenshots..."
dc up -d

echo "Waiting for UI routes..."
wait_for_http "http://127.0.0.1:$HTTP_PORT/explorer/"
wait_for_http "http://127.0.0.1:$HTTP_PORT/workbench/"
wait_for_http "http://127.0.0.1:$HTTP_PORT/api/v1/docs"

echo "Capturing screenshots..."
SYNC_BASE_URL="http://127.0.0.1:$HTTP_PORT" npm --prefix "$INFO_DIR" run capture:screenshots

echo "Done."
