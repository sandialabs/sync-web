#!/bin/sh
set -eu

MODE="${1:-up}"
if [ "$MODE" != "build" ] && [ "$MODE" != "generate" ] && [ "$MODE" != "up" ] && [ "$MODE" != "down" ]; then
    echo "Usage: $0 [build|generate|up|down]"
    exit 1
fi

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ROOT_DIR="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
CUSTOM_SETUP_FILE="${CUSTOM_SETUP_FILE:-}"
DOCKER_PLATFORM="${DOCKER_PLATFORM:-}"
COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-social-agent-network}"
export COMPOSE_PROJECT_NAME

require_dir() {
    name="$1"
    value="${2:-}"
    if [ -z "$value" ]; then
        echo "Environment variable $name is required" >&2
        exit 1
    fi
    if [ ! -d "$value" ]; then
        echo "$name must point to an existing directory: $value" >&2
        exit 1
    fi
    (CDPATH= cd -- "$value" && pwd -P)
}

SYNC_SERVICES_DIR="$(require_dir SYNC_SERVICES "${SYNC_SERVICES:-}")"
SYNC_JOURNAL_DIR="$(require_dir SYNC_JOURNAL "${SYNC_JOURNAL:-}")"
SYNC_RECORDS_DIR="$(require_dir SYNC_RECORDS "${SYNC_RECORDS:-}")"
SYNC_RECORDS_LISP_DIR="$SYNC_RECORDS_DIR/lisp"
if [ ! -d "$SYNC_RECORDS_LISP_DIR" ]; then
    echo "Expected Lisp directory at $SYNC_RECORDS_LISP_DIR" >&2
    exit 1
fi
SYNC_SERVICES_GENERAL_COMPOSE="$SYNC_SERVICES_DIR/compose/general/docker-compose.yml"
if [ ! -f "$SYNC_SERVICES_GENERAL_COMPOSE" ]; then
    echo "Expected compose file at $SYNC_SERVICES_GENERAL_COMPOSE" >&2
    exit 1
fi

VERSION="$(cat "$ROOT_DIR/VERSION")"
SOCIAL_AGENT_LOCAL_TAG="sync-web/local-social-agent:$VERSION"
JOURNAL_REMOTE_TAG="ghcr.io/sandialabs/sync-web/journal-sdk:$VERSION"

CUSTOM_SETUP=""
if [ -n "$CUSTOM_SETUP_FILE" ] && [ -x "$CUSTOM_SETUP_FILE" ]; then
    CUSTOM_SETUP_SCRIPT="$("$CUSTOM_SETUP_FILE")"
    CUSTOM_SETUP="$(printf "%s" "$CUSTOM_SETUP_SCRIPT")"
fi

build_social_agent() {
    echo "Building $SOCIAL_AGENT_LOCAL_TAG ..."
    if docker buildx version >/dev/null 2>&1; then
        if [ -n "$DOCKER_PLATFORM" ]; then
            docker buildx build \
                --load \
                --platform "$DOCKER_PLATFORM" \
                -f "$ROOT_DIR/tests/common/social-agent/Dockerfile" \
                --build-arg CUSTOM_SETUP="$CUSTOM_SETUP" \
                -t "$SOCIAL_AGENT_LOCAL_TAG" \
                "$ROOT_DIR/tests/common/social-agent"
        else
            docker buildx build \
                --load \
                -f "$ROOT_DIR/tests/common/social-agent/Dockerfile" \
                --build-arg CUSTOM_SETUP="$CUSTOM_SETUP" \
                -t "$SOCIAL_AGENT_LOCAL_TAG" \
                "$ROOT_DIR/tests/common/social-agent"
        fi
    else
        if [ -n "$DOCKER_PLATFORM" ]; then
            docker build \
                --platform "$DOCKER_PLATFORM" \
                -f "$ROOT_DIR/tests/common/social-agent/Dockerfile" \
                --build-arg CUSTOM_SETUP="$CUSTOM_SETUP" \
                -t "$SOCIAL_AGENT_LOCAL_TAG" \
                "$ROOT_DIR/tests/common/social-agent"
        else
            docker build \
                -f "$ROOT_DIR/tests/common/social-agent/Dockerfile" \
                --build-arg CUSTOM_SETUP="$CUSTOM_SETUP" \
                -t "$SOCIAL_AGENT_LOCAL_TAG" \
                "$ROOT_DIR/tests/common/social-agent"
        fi
    fi
}

build_journal_sdk() {
    echo "Building $JOURNAL_REMOTE_TAG from $SYNC_JOURNAL_DIR ..."
    if docker buildx version >/dev/null 2>&1; then
        if [ -n "$DOCKER_PLATFORM" ]; then
            docker buildx build \
                --load \
                --platform "$DOCKER_PLATFORM" \
                --build-arg CUSTOM_SETUP="$CUSTOM_SETUP" \
                -t "$JOURNAL_REMOTE_TAG" \
                "$SYNC_JOURNAL_DIR"
        else
            docker buildx build \
                --load \
                --build-arg CUSTOM_SETUP="$CUSTOM_SETUP" \
                -t "$JOURNAL_REMOTE_TAG" \
                "$SYNC_JOURNAL_DIR"
        fi
    else
        if [ -n "$DOCKER_PLATFORM" ]; then
            docker build \
                --platform "$DOCKER_PLATFORM" \
                --build-arg CUSTOM_SETUP="$CUSTOM_SETUP" \
                -t "$JOURNAL_REMOTE_TAG" \
                "$SYNC_JOURNAL_DIR"
        else
            docker build \
                --build-arg CUSTOM_SETUP="$CUSTOM_SETUP" \
                -t "$JOURNAL_REMOTE_TAG" \
                "$SYNC_JOURNAL_DIR"
        fi
    fi
}

build_local_stack() {
    build_journal_sdk
    CUSTOM_SETUP_FILE="$CUSTOM_SETUP_FILE" \
    DOCKER_PLATFORM="$DOCKER_PLATFORM" \
    "$ROOT_DIR/tests/api/local-compose.sh" build
    build_social_agent
}

generate_network() {
    cd "$SCRIPT_DIR"
    IMAGE_OVERRIDE_SOCIAL_AGENT="$SOCIAL_AGENT_LOCAL_TAG" \
    SYNC_SERVICES_GENERAL_COMPOSE="$SYNC_SERVICES_GENERAL_COMPOSE" \
    COMPOSE_PROJECT_NAME="$COMPOSE_PROJECT_NAME" \
    python3 generate.py
}

if [ "$MODE" = "down" ]; then
    cd "$SCRIPT_DIR"
    docker compose down -v --remove-orphans
    exit 0
fi

build_local_stack

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
exec docker compose up
