#!/bin/sh
set -eu

MODE="${1:-up}"
if [ "$MODE" != "up" ] && [ "$MODE" != "smoke" ] && [ "$MODE" != "build" ]; then
    echo "Usage: $0 [build|up|smoke]"
    exit 1
fi

ROOT_DIR="$(git -C "$(dirname -- "$0")" rev-parse --show-toplevel)"
COMPOSE_DIR="$ROOT_DIR/deploy/compose/general"
COMPOSE_FILE="$COMPOSE_DIR/compose.yaml"

PORT="${PORT:-8192}"
SMB_PORT="${SMB_PORT:-445}"
SECRET="${SECRET:-password}"
PERIOD="${PERIOD:-2}"
WINDOW="${WINDOW:-1024}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-60}"
CONNECT_TIMEOUT_SECONDS="${CONNECT_TIMEOUT_SECONDS:-2}"
REQUEST_TIMEOUT_SECONDS="${REQUEST_TIMEOUT_SECONDS:-5}"
COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-$(basename "$COMPOSE_DIR")}"
LOCAL_COMPOSE_FORCE_HTTP="${LOCAL_COMPOSE_FORCE_HTTP:-1}"
DOCKER_PLATFORM="${DOCKER_PLATFORM:-}"
CUSTOM_SETUP_FILE="${CUSTOM_SETUP_FILE:-}"

cleanup_mode="down"

VERSION="$(cat "$ROOT_DIR/VERSION")"
SYNC_WEB_VERSION="$VERSION"

GENERAL_REMOTE_TAG="ghcr.io/sandialabs/sync-web/general:$VERSION"
GATEWAY_REMOTE_TAG="ghcr.io/sandialabs/sync-web/gateway:$VERSION"
EXPLORER_REMOTE_TAG="ghcr.io/sandialabs/sync-web/explorer:$VERSION"
WORKBENCH_REMOTE_TAG="ghcr.io/sandialabs/sync-web/workbench:$VERSION"
ROUTER_REMOTE_TAG="ghcr.io/sandialabs/sync-web/router:$VERSION"
FILE_SYSTEM_REMOTE_TAG="ghcr.io/sandialabs/sync-web/file-system:$VERSION"

GENERAL_LOCAL_TAG="sync-web/local-general:$VERSION"
GATEWAY_LOCAL_TAG="sync-web/local-gateway:$VERSION"
EXPLORER_LOCAL_TAG="sync-web/local-explorer:$VERSION"
WORKBENCH_LOCAL_TAG="sync-web/local-workbench:$VERSION"
ROUTER_LOCAL_TAG="sync-web/local-router:$VERSION"
FILE_SYSTEM_LOCAL_TAG="sync-web/local-file-system:$VERSION"
FILE_SYSTEM_IMAGE="${FILE_SYSTEM_IMAGE:-$FILE_SYSTEM_REMOTE_TAG}"

dc() {
    docker compose -f "$COMPOSE_FILE" "$@"
}

has_existing_named_volumes() {
    names="$(dc config --volumes 2>/dev/null || true)"
    if [ -z "$names" ]; then
        return 1
    fi
    for logical_name in $names; do
        full_name="${COMPOSE_PROJECT_NAME}_${logical_name}"
        if docker volume inspect "$full_name" >/dev/null 2>&1; then
            return 0
        fi
    done
    return 1
}

confirm_volume_wipe_if_needed() {
    if ! has_existing_named_volumes; then
        return 0
    fi
    echo "Existing compose volumes were found for project '$COMPOSE_PROJECT_NAME'."
    while :; do
        printf "Wipe existing volumes and continue? [y/n]: "
        IFS= read -r answer || true
        case "$(printf "%s" "$answer" | tr '[:upper:]' '[:lower:]')" in
            y|yes) return 0 ;;
            n|no)
                echo "Aborting."
                exit 1
                ;;
            *) echo "Please answer y or n." ;;
        esac
    done
}

cleanup() {
    set +e
    if [ "$cleanup_mode" = "down" ]; then
        dc down -v --remove-orphans >/dev/null 2>&1
    fi
}

on_interrupt() {
    cleanup
    exit 130
}

trap cleanup EXIT
trap on_interrupt INT TERM

CUSTOM_SETUP=""
if [ -x "$CUSTOM_SETUP_FILE" ]; then
    CUSTOM_SETUP_SCRIPT="$("$CUSTOM_SETUP_FILE")"
    CUSTOM_SETUP="$(printf "%s" "$CUSTOM_SETUP_SCRIPT")"
fi

if [ "$LOCAL_COMPOSE_FORCE_HTTP" = "1" ]; then
    TLS_STUB_DIR="/tmp/sync-services-local-compose"
    TLS_STUB_CERT="$TLS_STUB_DIR/http-only.crt"
    TLS_STUB_KEY="$TLS_STUB_DIR/http-only.key"
    mkdir -p "$TLS_STUB_DIR"
    printf "HTTP-only local-compose placeholder cert.\n" > "$TLS_STUB_CERT"
    printf "HTTP-only local-compose placeholder key.\n" > "$TLS_STUB_KEY"
    TLS_CERT_HOST_PATH="$TLS_STUB_CERT"
    TLS_KEY_HOST_PATH="$TLS_STUB_KEY"
fi

build_and_retag() {
    context="$1"
    local_tag="$2"
    remote_tag="$3"
    build_platform="${4:-$DOCKER_PLATFORM}"
    dockerfile="${5:-}"

    echo "Building $local_tag ..."

    set -- --build-arg "CUSTOM_SETUP=$CUSTOM_SETUP" -t "$local_tag"
    if [ -n "$build_platform" ]; then
        set -- --platform "$build_platform" "$@"
    fi
    if [ -n "$dockerfile" ]; then
        set -- -f "$dockerfile" "$@"
    fi

    if docker buildx version >/dev/null 2>&1; then
        docker buildx build --load "$@" "$context"
    else
        docker build "$@" "$context"
    fi

    echo "Tagging $local_tag as $remote_tag ..."
    docker tag "$local_tag" "$remote_tag"
}

build_and_retag "$ROOT_DIR" "$GENERAL_LOCAL_TAG" "$GENERAL_REMOTE_TAG" "" "$COMPOSE_DIR/Dockerfile"
build_and_retag "$ROOT_DIR/services/gateway" "$GATEWAY_LOCAL_TAG" "$GATEWAY_REMOTE_TAG"
build_and_retag "$ROOT_DIR/services/explorer" "$EXPLORER_LOCAL_TAG" "$EXPLORER_REMOTE_TAG"
build_and_retag "$ROOT_DIR/services/workbench" "$WORKBENCH_LOCAL_TAG" "$WORKBENCH_REMOTE_TAG"
build_and_retag "$ROOT_DIR/services/router" "$ROUTER_LOCAL_TAG" "$ROUTER_REMOTE_TAG"
build_and_retag "$ROOT_DIR/services/file-system" "$FILE_SYSTEM_LOCAL_TAG" "$FILE_SYSTEM_REMOTE_TAG"
echo "Tagging $FILE_SYSTEM_LOCAL_TAG as $FILE_SYSTEM_IMAGE ..."
docker tag "$FILE_SYSTEM_LOCAL_TAG" "$FILE_SYSTEM_IMAGE"

if [ "$MODE" = "build" ]; then
    echo "PASS: local images built and tagged."
    exit 0
fi

export SECRET PERIOD WINDOW PORT SMB_PORT COMPOSE_PROJECT_NAME TLS_CERT_HOST_PATH TLS_KEY_HOST_PATH FILE_SYSTEM_IMAGE SYNC_WEB_VERSION

confirm_volume_wipe_if_needed
echo "Starting from scratch: removing compose stack + volumes..."
dc down -v --remove-orphans >/dev/null 2>&1

wait_for_http() {
    url="$1"
    elapsed=0
    while [ "$elapsed" -lt "$TIMEOUT_SECONDS" ]; do
        if curl -fsS \
          --connect-timeout "$CONNECT_TIMEOUT_SECONDS" \
          --max-time "$REQUEST_TIMEOUT_SECONDS" \
          "$url" >/dev/null 2>&1; then
            return 0
        fi
        sleep 2
        elapsed=$((elapsed + 2))
    done
    echo "Timed out waiting for $url" >&2
    return 1
}

api_post() {
    body="$1"
    curl -fsS \
      -H "Content-Type: application/json" \
      -d "$body" \
      "http://127.0.0.1:$PORT/interface/json"
}

gateway_status() {
    method="$1"
    path="$2"
    shift 2
    curl -sS -o /dev/null -w "%{http_code}" -X "$method" "$@" "http://127.0.0.1:$PORT$path"
}

gateway_get() {
    path="$1"
    shift
    curl -fsS "$@" "http://127.0.0.1:$PORT$path"
}

if [ "$MODE" = "up" ]; then
    echo "Starting compose stack on port $PORT in up mode (Ctrl+C to stop)..."
    dc --ansi always up --pull never
    exit 0
fi

echo "Starting compose stack on port $PORT in smoke mode..."
dc --ansi always up -d --pull never

echo "Waiting for routes..."
wait_for_http "http://127.0.0.1:$PORT/explorer/"
wait_for_http "http://127.0.0.1:$PORT/workbench/"
wait_for_http "http://127.0.0.1:$PORT/api/v1/docs"

echo "Running API smoke checks..."
size_response="$(api_post '{"function":"size"}' | tr -d '[:space:]')"
case "$size_response" in
    ''|*[!0-9]*)
        echo "FAIL: size response is not a number: $size_response"
        exit 1
        ;;
esac

config_response="$(api_post "{\"function\":\"config\",\"authentication\":\"$SECRET\"}")"
if [ -z "$config_response" ]; then
    echo "FAIL: config response is empty"
    exit 1
fi

gateway_size="$(gateway_get "/api/v1/general/size" | tr -d '[:space:]')"
case "$gateway_size" in
    ''|*[!0-9]*)
        echo "FAIL: gateway size response is not a number: $gateway_size"
        exit 1
        ;;
esac

root_unauthorized_status="$(gateway_status POST "/api/v1/root/step" -H "Content-Type: application/json" -d '[]')"
if [ "$root_unauthorized_status" != "401" ]; then
    echo "FAIL: expected gateway root route to require auth (401), got $root_unauthorized_status"
    exit 1
fi

root_authorized_status="$(gateway_status POST "/api/v1/root/step" -H "Authorization: Bearer $SECRET" -H "Content-Type: application/json" -d '[]')"
if [ "$root_authorized_status" != "200" ]; then
    echo "FAIL: expected authenticated gateway root route to succeed (200), got $root_authorized_status"
    exit 1
fi

if ! command -v smbclient >/dev/null 2>&1; then
    echo "FAIL: smbclient is required for local-compose smoke validation"
    exit 1
fi

echo "Waiting for SMB file-system service on port $SMB_PORT..."
elapsed=0
while [ "$elapsed" -lt "$TIMEOUT_SECONDS" ]; do
    if smbclient //127.0.0.1/sync -N -p "$SMB_PORT" -c 'ls' >/tmp/sync-services-fs-root-ls.log 2>&1; then
        break
    fi
    sleep 2
    elapsed=$((elapsed + 2))
done
if [ "$elapsed" -ge "$TIMEOUT_SECONDS" ]; then
    echo "FAIL: timed out waiting for SMB file-system service on port $SMB_PORT"
    cat /tmp/sync-services-fs-root-ls.log 2>/dev/null || true
    exit 1
fi

fs_root_listing="$(cat /tmp/sync-services-fs-root-ls.log)"
for required in stage ledger root; do
    if ! printf "%s" "$fs_root_listing" | grep -q " $required "; then
        echo "FAIL: expected SMB root listing to contain '$required'"
        printf "%s\n" "$fs_root_listing"
        exit 1
    fi
done

fs_tmp_dir="/tmp/sync-services-fs-smoke"
fs_local_file="$fs_tmp_dir/local.txt"
fs_download_file="$fs_tmp_dir/downloaded.txt"
mkdir -p "$fs_tmp_dir"
printf "compose filesystem smoke\n" > "$fs_local_file"
rm -f "$fs_download_file"

if ! smbclient //127.0.0.1/sync -N -p "$SMB_PORT" -c "cd stage; put $fs_local_file compose-smoke.txt; get compose-smoke.txt $fs_download_file; del compose-smoke.txt" >/tmp/sync-services-fs-stage.log 2>&1; then
    echo "FAIL: SMB stage smoke failed"
    cat /tmp/sync-services-fs-stage.log
    exit 1
fi

if ! cmp -s "$fs_local_file" "$fs_download_file"; then
    echo "FAIL: SMB stage round-trip content mismatch"
    exit 1
fi

echo "PASS: smoke checks succeeded."
