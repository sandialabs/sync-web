#!/bin/sh
set -eu

MODE="${1:-up}"
if [ "$MODE" != "build" ] && [ "$MODE" != "generate" ] && [ "$MODE" != "up" ] && [ "$MODE" != "down" ]; then
    echo "Usage: $0 [build|generate|up|down]"
    exit 1
fi

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
SYNC_ANALYSIS_DIR="$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)"
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

SOCIAL_AGENT_VERSION="$(cat "$SYNC_ANALYSIS_DIR/firewheel/model-components/social-agent/version.txt")"
SOCIAL_AGENT_LOCAL_TAG="sync-analysis/local-social-agent:$SOCIAL_AGENT_VERSION"
JOURNAL_SDK_TAG="$(
    awk '
        $1 == "FROM" && $2 ~ /^ghcr.io\/sandialabs\/sync-journal\/journal-sdk:/ {
            split($2, parts, ":");
            print parts[2];
            exit;
        }
    ' "$SYNC_SERVICES_DIR/compose/general/Dockerfile"
)"

if [ -z "$JOURNAL_SDK_TAG" ]; then
    echo "Could not determine journal SDK tag from $SYNC_SERVICES_DIR/compose/general/Dockerfile" >&2
    exit 1
fi

JOURNAL_REMOTE_TAG="ghcr.io/sandialabs/sync-journal/journal-sdk:$JOURNAL_SDK_TAG"

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
                -f "$SYNC_ANALYSIS_DIR/docker/social-agent/Dockerfile" \
                --build-arg CUSTOM_SETUP="$CUSTOM_SETUP" \
                -t "$SOCIAL_AGENT_LOCAL_TAG" \
                "$SYNC_ANALYSIS_DIR"
        else
            docker buildx build \
                --load \
                -f "$SYNC_ANALYSIS_DIR/docker/social-agent/Dockerfile" \
                --build-arg CUSTOM_SETUP="$CUSTOM_SETUP" \
                -t "$SOCIAL_AGENT_LOCAL_TAG" \
                "$SYNC_ANALYSIS_DIR"
        fi
    else
        if [ -n "$DOCKER_PLATFORM" ]; then
            docker build \
                --platform "$DOCKER_PLATFORM" \
                -f "$SYNC_ANALYSIS_DIR/docker/social-agent/Dockerfile" \
                --build-arg CUSTOM_SETUP="$CUSTOM_SETUP" \
                -t "$SOCIAL_AGENT_LOCAL_TAG" \
                "$SYNC_ANALYSIS_DIR"
        else
            docker build \
                -f "$SYNC_ANALYSIS_DIR/docker/social-agent/Dockerfile" \
                --build-arg CUSTOM_SETUP="$CUSTOM_SETUP" \
                -t "$SOCIAL_AGENT_LOCAL_TAG" \
                "$SYNC_ANALYSIS_DIR"
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
    LOCAL_LISP_DIRECTORY="$SYNC_RECORDS_LISP_DIR" \
    CUSTOM_SETUP_FILE="$CUSTOM_SETUP_FILE" \
    DOCKER_PLATFORM="$DOCKER_PLATFORM" \
    "$SYNC_SERVICES_DIR/tests/local-compose.sh" build
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
