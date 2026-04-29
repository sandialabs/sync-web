#!/bin/sh
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
INFO_DIR="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)"
SERVICES_DIR="${SYNC_SERVICES_DIR:-}"
if [ -z "$SERVICES_DIR" ]; then
    echo "SYNC_SERVICES_DIR is required." >&2
    exit 1
fi
COMPOSE_DIR="$SERVICES_DIR/compose/general"
LOCAL_OVERRIDE_FILE="$SERVICES_DIR/tests/docker-compose.local.yml"
LOCAL_UI_OVERRIDE_FILE="$SERVICES_DIR/tests/docker-compose.local-ui.yml"

PORT="${PORT:-8192}"
SECRET="${SECRET:-password}"
PERIOD="${PERIOD:-2}"
WINDOW="${WINDOW:-128}"
LOCAL_LISP_PATH="${LOCAL_LISP_PATH:-}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-60}"

COMPOSE_ARGS="-f $COMPOSE_DIR/docker-compose.yml"
if [ -n "$LOCAL_LISP_PATH" ]; then
    COMPOSE_ARGS="$COMPOSE_ARGS -f $LOCAL_OVERRIDE_FILE"
    COMPOSE_ARGS="$COMPOSE_ARGS -f $LOCAL_UI_OVERRIDE_FILE"
fi

dc() {
    # shellcheck disable=SC2086
    docker compose $COMPOSE_ARGS "$@"
}

cleanup() {
    set +e
    dc down -v --remove-orphans >/dev/null 2>&1
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

export PORT SECRET PERIOD WINDOW LOCAL_LISP_PATH

echo "Starting sync-services stack on port $PORT..."
dc up -d --build

echo "Waiting for UI routes..."
wait_for_http "http://127.0.0.1:$PORT/explorer/"
wait_for_http "http://127.0.0.1:$PORT/workbench/"
wait_for_http "http://127.0.0.1:$PORT/docs"

echo "Capturing screenshots..."
SYNC_BASE_URL="http://127.0.0.1:$PORT" npm --prefix "$INFO_DIR" run capture:screenshots

echo "Done."
