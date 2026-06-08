#!/bin/bash
set -eu

MODE="${1:-up}"
if [ "$MODE" != "build" ] && [ "$MODE" != "generate" ] && [ "$MODE" != "up" ] && [ "$MODE" != "down" ]; then
    echo "Usage: $0 [build|generate|up|down] [-d|--detach] [--no-build]"
    exit 1
fi
shift || true

DETACH=0
SKIP_BUILD=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        -d|--detach)
            DETACH=1
            ;;
        --no-build)
            SKIP_BUILD=1
            ;;
        *)
            echo "Usage: $0 [build|generate|up|down] [-d|--detach] [--no-build]"
            exit 1
            ;;
    esac
    shift
done

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ROOT_DIR="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DOCKER_PLATFORM="${DOCKER_PLATFORM:-}"
COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-social-agent-network}"
CONTAINER_RUNTIME="${CONTAINER_RUNTIME:-docker}"
CONTAINER_COMPOSE="${CONTAINER_COMPOSE:-$CONTAINER_RUNTIME compose}"
export COMPOSE_PROJECT_NAME

GENERAL_COMPOSE_FILE="$ROOT_DIR/deploy/compose/general/compose.yaml"
if [ ! -f "$GENERAL_COMPOSE_FILE" ]; then
    echo "Expected compose file at $GENERAL_COMPOSE_FILE" >&2
    exit 1
fi

VERSION="$(cat "$ROOT_DIR/VERSION")"
SOCIAL_AGENT_LOCAL_TAG="sync-web/local-social-agent:$VERSION"

CUSTOM_SETUP="${CUSTOM_SETUP:-}"

build_social_agent() {
    echo "Building $SOCIAL_AGENT_LOCAL_TAG ..."
    set -- \
        -f "$ROOT_DIR/tests/network/common/social-agent/Dockerfile" \
        --build-arg CUSTOM_SETUP="$CUSTOM_SETUP" \
        -t "$SOCIAL_AGENT_LOCAL_TAG"
    if [ -n "$DOCKER_PLATFORM" ]; then
        set -- --platform "$DOCKER_PLATFORM" "$@"
    fi
    $CONTAINER_RUNTIME build "$@" "$ROOT_DIR/tests/network/common/social-agent"
}

build_local_stack() {
    CUSTOM_SETUP="$CUSTOM_SETUP" \
    DOCKER_PLATFORM="$DOCKER_PLATFORM" \
    "$ROOT_DIR/tests/api/local-compose.sh" build
    build_social_agent
}

generate_network() {
    cd "$SCRIPT_DIR"
    IMAGE_OVERRIDE_SOCIAL_AGENT="$SOCIAL_AGENT_LOCAL_TAG" \
    SYNC_SERVICES_GENERAL_COMPOSE="$GENERAL_COMPOSE_FILE" \
    COMPOSE_PROJECT_NAME="$COMPOSE_PROJECT_NAME" \
    python3 generate.py
}

if [ "$MODE" = "down" ]; then
    cd "$SCRIPT_DIR"
    $CONTAINER_COMPOSE down -v --remove-orphans
    exit 0
fi

if [ "$SKIP_BUILD" = "0" ]; then
    build_local_stack
fi

if [ "$MODE" = "build" ]; then
    echo "PASS: local journal, services, and social-agent images are built."
    exit 0
fi

generate_network

if [ "$MODE" = "generate" ]; then
    echo "PASS: generated local social-agent network compose files."
    exit 0
fi

cd "$SCRIPT_DIR"
$CONTAINER_COMPOSE down -v --remove-orphans
if [ "$DETACH" = "1" ]; then
    $CONTAINER_COMPOSE up -d
else
    $CONTAINER_COMPOSE up
fi
