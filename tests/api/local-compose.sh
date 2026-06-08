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
        docker)
            CONTAINER_COMPOSE="docker compose"
            ;;
        podman)
            if podman compose version >/dev/null 2>&1; then
                CONTAINER_COMPOSE="podman compose"
            elif command -v podman-compose >/dev/null 2>&1; then
                CONTAINER_COMPOSE="podman-compose"
            else
                CONTAINER_COMPOSE="podman compose"
            fi
            ;;
        *)
            CONTAINER_COMPOSE="$CONTAINER_RUNTIME compose"
            ;;
    esac
fi

HTTP_PORT="${HTTP_PORT:-${PORT:-8192}}"
HTTPS_PORT="${HTTPS_PORT:-8193}"
ORIGIN="${ORIGIN:-http://localhost:$HTTP_PORT}"
SECRET="${SECRET:-password}"
ADMIN_USERNAME="${ADMIN_USERNAME:-admin}"
PERIOD="${PERIOD:-2}"
WINDOW="${WINDOW:-1024}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-60}"
CONNECT_TIMEOUT_SECONDS="${CONNECT_TIMEOUT_SECONDS:-2}"
REQUEST_TIMEOUT_SECONDS="${REQUEST_TIMEOUT_SECONDS:-5}"
COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-sync-local}"
LOCAL_COMPOSE_FORCE_HTTP="${LOCAL_COMPOSE_FORCE_HTTP:-1}"
DOCKER_PLATFORM="${DOCKER_PLATFORM:-}"

LOCAL_COMPOSE_SKIP_FILE_SYSTEM="${LOCAL_COMPOSE_SKIP_FILE_SYSTEM:-0}"

if [ "$CONTAINER_RUNTIME" = "docker" ]; then
    COMPOSE_GLOBAL_ARGS="${COMPOSE_GLOBAL_ARGS:---ansi always}"
    COMPOSE_UP_ARGS="${COMPOSE_UP_ARGS:---pull never}"
else
    COMPOSE_GLOBAL_ARGS="${COMPOSE_GLOBAL_ARGS:-}"
    COMPOSE_UP_ARGS="${COMPOSE_UP_ARGS:-}"
fi

cleanup_mode="down"

VERSION="$(cat "$ROOT_DIR/VERSION")"
SYNC_WEB_VERSION="$VERSION"

JOURNAL_SDK_REMOTE_TAG="ghcr.io/sandialabs/sync-web/journal-sdk:$VERSION"
GATEWAY_REMOTE_TAG="ghcr.io/sandialabs/sync-web/gateway:$VERSION"
EXPLORER_REMOTE_TAG="ghcr.io/sandialabs/sync-web/explorer:$VERSION"
WORKBENCH_REMOTE_TAG="ghcr.io/sandialabs/sync-web/workbench:$VERSION"
ROUTER_REMOTE_TAG="ghcr.io/sandialabs/sync-web/router:$VERSION"
IDENTITY_PROVIDER_REMOTE_TAG="ghcr.io/sandialabs/sync-web/identity-provider:$VERSION"
FILE_SYSTEM_REMOTE_TAG="ghcr.io/sandialabs/sync-web/file-system:$VERSION"

JOURNAL_SDK_LOCAL_TAG="sync-web/local-journal-sdk:$VERSION"
GATEWAY_LOCAL_TAG="sync-web/local-gateway:$VERSION"
EXPLORER_LOCAL_TAG="sync-web/local-explorer:$VERSION"
WORKBENCH_LOCAL_TAG="sync-web/local-workbench:$VERSION"
ROUTER_LOCAL_TAG="sync-web/local-router:$VERSION"
IDENTITY_PROVIDER_LOCAL_TAG="sync-web/local-identity-provider:$VERSION"
FILE_SYSTEM_LOCAL_TAG="sync-web/local-file-system:$VERSION"
FILE_SYSTEM_IMAGE="${FILE_SYSTEM_IMAGE:-$FILE_SYSTEM_REMOTE_TAG}"

dc() {
    $CONTAINER_COMPOSE -f "$COMPOSE_FILE" "$@"
}

has_existing_named_volumes() {
    names="$(dc config --volumes 2>/dev/null || true)"
    if [ -z "$names" ]; then
        return 1
    fi
    for logical_name in $names; do
        full_name="${COMPOSE_PROJECT_NAME}_${logical_name}"
        if $CONTAINER_RUNTIME volume inspect "$full_name" >/dev/null 2>&1; then
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

CUSTOM_SETUP="${CUSTOM_SETUP:-}"

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
    extra_build_arg="${6:-}"

    echo "Building $local_tag ..."

    set -- --build-arg "CUSTOM_SETUP=$CUSTOM_SETUP" -t "$local_tag"
    if [ -n "$extra_build_arg" ]; then
        set -- --build-arg "$extra_build_arg" "$@"
    fi
    if [ -n "$build_platform" ]; then
        set -- --platform "$build_platform" "$@"
    fi
    if [ -n "$dockerfile" ]; then
        set -- -f "$dockerfile" "$@"
    fi

    $CONTAINER_RUNTIME build "$@" "$context"

    echo "Tagging $local_tag as $remote_tag ..."
    $CONTAINER_RUNTIME tag "$local_tag" "$remote_tag"
}

build_and_retag "$ROOT_DIR/journal" "$JOURNAL_SDK_LOCAL_TAG" "$JOURNAL_SDK_REMOTE_TAG"
build_and_retag "$ROOT_DIR/services/gateway" "$GATEWAY_LOCAL_TAG" "$GATEWAY_REMOTE_TAG"
build_and_retag "$ROOT_DIR/services/explorer" "$EXPLORER_LOCAL_TAG" "$EXPLORER_REMOTE_TAG"
build_and_retag "$ROOT_DIR/services/workbench" "$WORKBENCH_LOCAL_TAG" "$WORKBENCH_REMOTE_TAG"
build_and_retag "$ROOT_DIR/services/router" "$ROUTER_LOCAL_TAG" "$ROUTER_REMOTE_TAG"
build_and_retag "$ROOT_DIR/services/identity-provider" "$IDENTITY_PROVIDER_LOCAL_TAG" "$IDENTITY_PROVIDER_REMOTE_TAG"
if [ "$LOCAL_COMPOSE_SKIP_FILE_SYSTEM" != "1" ]; then
    build_and_retag "$ROOT_DIR/services/file-system" "$FILE_SYSTEM_LOCAL_TAG" "$FILE_SYSTEM_REMOTE_TAG"
    echo "Tagging $FILE_SYSTEM_LOCAL_TAG as $FILE_SYSTEM_IMAGE ..."
    $CONTAINER_RUNTIME tag "$FILE_SYSTEM_LOCAL_TAG" "$FILE_SYSTEM_IMAGE"
fi

if [ "$MODE" = "build" ]; then
    echo "PASS: local images built and tagged."
    exit 0
fi

export SECRET ADMIN_USERNAME PERIOD WINDOW HTTP_PORT ORIGIN HTTPS_PORT COMPOSE_PROJECT_NAME TLS_CERT_HOST_PATH TLS_KEY_HOST_PATH FILE_SYSTEM_IMAGE SYNC_WEB_VERSION

compose_up_services=""
if [ "$LOCAL_COMPOSE_SKIP_FILE_SYSTEM" = "1" ]; then
    compose_up_services="journal explorer workbench identity-provider gateway router"
fi

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

wait_for_admin_seed() {
    if [ -z "$ADMIN_USERNAME" ]; then
        return 0
    fi

    elapsed=0
    while [ "$elapsed" -lt "$TIMEOUT_SECONDS" ]; do
        logs="$(dc logs --no-color identity-provider 2>/dev/null || dc logs identity-provider 2>/dev/null || true)"
        if printf "%s" "$logs" | grep -Fq "Admin identity '$ADMIN_USERNAME' created" \
          || printf "%s" "$logs" | grep -Fq "Admin identity '$ADMIN_USERNAME' already exists"; then
            return 0
        fi
        if printf "%s" "$logs" | grep -Fq "Failed to create admin identity '$ADMIN_USERNAME'"; then
            echo "FAIL: identity-provider failed to seed admin identity '$ADMIN_USERNAME'" >&2
            return 1
        fi
        sleep 2
        elapsed=$((elapsed + 2))
    done

    echo "FAIL: timed out waiting for identity-provider to seed admin identity '$ADMIN_USERNAME'" >&2
    return 1
}

api_post() {
    body="$1"
    curl -fsS \
      -H "Content-Type: application/json" \
      -d "$body" \
      "http://127.0.0.1:$HTTP_PORT/interface"
}

gateway_status() {
    method="$1"
    path="$2"
    shift 2
    curl -sS -o /dev/null -w "%{http_code}" -X "$method" "$@" "http://127.0.0.1:$HTTP_PORT$path"
}

gateway_get() {
    path="$1"
    shift
    curl -fsS "$@" "http://127.0.0.1:$HTTP_PORT$path"
}

if [ "$MODE" = "up" ]; then
    echo "Starting compose project '$COMPOSE_PROJECT_NAME' on HTTP $HTTP_PORT / HTTPS $HTTPS_PORT in up mode (Ctrl+C to stop)..."
    dc $COMPOSE_GLOBAL_ARGS up $COMPOSE_UP_ARGS $compose_up_services
    exit 0
fi

echo "Starting compose project '$COMPOSE_PROJECT_NAME' on HTTP $HTTP_PORT / HTTPS $HTTPS_PORT in smoke mode..."
dc $COMPOSE_GLOBAL_ARGS up -d $COMPOSE_UP_ARGS $compose_up_services

echo "Waiting for routes..."
wait_for_http "http://127.0.0.1:$HTTP_PORT/explorer/"
wait_for_http "http://127.0.0.1:$HTTP_PORT/workbench/"
wait_for_http "http://127.0.0.1:$HTTP_PORT/api/v1/docs"
echo "Waiting for seeded admin identity..."
wait_for_admin_seed

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

if [ "$LOCAL_COMPOSE_SKIP_FILE_SYSTEM" = "1" ]; then
    echo "Skipping WebDAV file-system smoke checks."
    echo "PASS: smoke checks succeeded."
    exit 0
fi

echo "Running WebDAV file-system smoke checks..."
webdav_options_status="$(gateway_status OPTIONS "/webdav/stage/admin/compose-smoke.txt")"
if [ "$webdav_options_status" != "204" ]; then
    echo "FAIL: expected WebDAV OPTIONS to return 204, got $webdav_options_status"
    exit 1
fi

webdav_root_status="$(gateway_status PROPFIND "/webdav/")"
if [ "$webdav_root_status" != "207" ]; then
    echo "FAIL: expected WebDAV root PROPFIND to return 207, got $webdav_root_status"
    exit 1
fi

webdav_unauthorized_status="$(gateway_status PROPFIND "/webdav/stage/admin/compose-smoke.txt")"
if [ "$webdav_unauthorized_status" != "401" ]; then
    echo "FAIL: expected unauthorized WebDAV stage PROPFIND to return 401, got $webdav_unauthorized_status"
    exit 1
fi

login_flow_json="$(curl -fsS "http://127.0.0.1:$HTTP_PORT/auth/.ory/self-service/login/api")"
login_flow="$(printf "%s" "$login_flow_json" | python -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
session_json="$(curl -fsS \
  -X POST "http://127.0.0.1:$HTTP_PORT/auth/.ory/self-service/login?flow=$login_flow" \
  -H "Content-Type: application/json" \
  -d "{\"method\":\"password\",\"identifier\":\"$ADMIN_USERNAME\",\"password\":\"$SECRET\"}")"
session_token="$(printf "%s" "$session_json" | python -c 'import json,sys; print(json.load(sys.stdin)["session_token"])')"
api_token_json="$(curl -fsS \
  -X POST "http://127.0.0.1:$HTTP_PORT/api/v1/tokens" \
  -H "X-Session-Token: $session_token" \
  -H "Content-Type: application/json" \
  -d '{"description":"local-compose WebDAV smoke"}')"
api_token="$(printf "%s" "$api_token_json" | python -c 'import json,sys; print(json.load(sys.stdin)["token"])')"

fs_tmp_dir="/tmp/sync-services-webdav-smoke"
fs_local_file="$fs_tmp_dir/local.txt"
fs_download_file="$fs_tmp_dir/downloaded.txt"
mkdir -p "$fs_tmp_dir"
printf "compose WebDAV smoke\n" > "$fs_local_file"
rm -f "$fs_download_file"

webdav_put_status="$(curl -sS -o /dev/null -w "%{http_code}" \
  -u "$ADMIN_USERNAME:$api_token" \
  -T "$fs_local_file" \
  "http://127.0.0.1:$HTTP_PORT/webdav/stage/$ADMIN_USERNAME/compose-smoke.txt")"
if [ "$webdav_put_status" != "201" ]; then
    echo "FAIL: expected WebDAV PUT to return 201, got $webdav_put_status"
    exit 1
fi

webdav_get_status="$(curl -sS -o "$fs_download_file" -w "%{http_code}" \
  -u "$ADMIN_USERNAME:$api_token" \
  "http://127.0.0.1:$HTTP_PORT/webdav/stage/$ADMIN_USERNAME/compose-smoke.txt")"
if [ "$webdav_get_status" != "200" ]; then
    echo "FAIL: expected WebDAV GET to return 200, got $webdav_get_status"
    exit 1
fi
if ! cmp -s "$fs_local_file" "$fs_download_file"; then
    echo "FAIL: WebDAV round-trip content mismatch"
    exit 1
fi

curl -fsS -u "$ADMIN_USERNAME:$api_token" \
  -X PROPFIND "http://127.0.0.1:$HTTP_PORT/webdav/stage/$ADMIN_USERNAME/" \
  -o /tmp/sync-services-webdav-propfind.xml
if ! grep -q "compose-smoke.txt" /tmp/sync-services-webdav-propfind.xml; then
    echo "FAIL: WebDAV PROPFIND did not list uploaded file"
    cat /tmp/sync-services-webdav-propfind.xml
    exit 1
fi

webdav_move_status="$(curl -sS -o /dev/null -w "%{http_code}" \
  -u "$ADMIN_USERNAME:$api_token" \
  -X MOVE \
  -H "Destination: http://127.0.0.1:$HTTP_PORT/webdav/stage/$ADMIN_USERNAME/compose-smoke-moved.txt" \
  "http://127.0.0.1:$HTTP_PORT/webdav/stage/$ADMIN_USERNAME/compose-smoke.txt")"
if [ "$webdav_move_status" != "201" ]; then
    echo "FAIL: expected WebDAV MOVE to return 201, got $webdav_move_status"
    exit 1
fi

webdav_delete_status="$(curl -sS -o /dev/null -w "%{http_code}" \
  -u "$ADMIN_USERNAME:$api_token" \
  -X DELETE \
  "http://127.0.0.1:$HTTP_PORT/webdav/stage/$ADMIN_USERNAME/compose-smoke-moved.txt")"
if [ "$webdav_delete_status" != "204" ]; then
    echo "FAIL: expected WebDAV DELETE to return 204, got $webdav_delete_status"
    exit 1
fi

echo "PASS: smoke checks succeeded."
