#!/bin/sh
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ROOT_DIR="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
NETWORK_DIR="$ROOT_DIR/tests/network/compose"
PROJECT="${COMPOSE_PROJECT_NAME:-sync-release-qa-sticky}"
RUNTIME="${CONTAINER_RUNTIME:-podman}"
COMPOSE="${CONTAINER_COMPOSE:-$RUNTIME compose}"
HTTP_PORT_BASE="${HTTP_PORT_BASE:-18192}"
GATEWAY_PORT_BASE="${GATEWAY_PORT_BASE:-$((HTTP_PORT_BASE + 100))}"
OUTPUT="${OUTPUT:-$ROOT_DIR/target/release-qa/sticky-readiness}"
IMAGE_MANIFEST="${IMAGE_MANIFEST:-$OUTPUT/image-manifest.json}"
MODE="${1:---no-build}"
export PYTHONDONTWRITEBYTECODE=1

case "$PROJECT" in
  *[!A-Za-z0-9_.-]*|*..*) echo "Unsafe Compose project name: $PROJECT" >&2; exit 2 ;;
esac

case "$MODE" in
  --build) UP_ARGUMENTS="-d" ;;
  --no-build)
    UP_ARGUMENTS="-d --no-build"
    [ -f "$IMAGE_MANIFEST" ] || {
      echo "--no-build requires IMAGE_MANIFEST from an exact --build run: $IMAGE_MANIFEST" >&2
      exit 2
    }
    python3 "$SCRIPT_DIR/image_manifest.py" verify \
      --manifest "$IMAGE_MANIFEST" --source-root "$ROOT_DIR" --runtime "$RUNTIME"
    ;;
  *) echo "Usage: $0 [--build|--no-build]" >&2; exit 2 ;;
esac

if "$RUNTIME" ps -a --filter "label=com.docker.compose.project=$PROJECT" -q | grep -q .; then
  echo "Refusing to reuse existing Compose project: $PROJECT" >&2
  exit 2
fi

mkdir -p "$OUTPUT"
rm -rf "$NETWORK_DIR/runs/$PROJECT"
cleanup() {
  set +e
  COMPOSE_PROJECT_NAME="$PROJECT" CONTAINER_RUNTIME="$RUNTIME" CONTAINER_COMPOSE="$COMPOSE" \
    "$NETWORK_DIR/local-compose.sh" down >"$OUTPUT/down.out" 2>"$OUTPUT/down.err"
  rm -rf "$NETWORK_DIR/runs/$PROJECT" "$SCRIPT_DIR/__pycache__" "$NETWORK_DIR/__pycache__"
}
on_signal() {
  trap - EXIT
  cleanup
  exit 130
}
trap cleanup EXIT
trap on_signal INT TERM

COMPOSE_PROJECT_NAME="$PROJECT" \
CONTAINER_RUNTIME="$RUNTIME" \
CONTAINER_COMPOSE="$COMPOSE" \
HTTP_PORT_BASE="$HTTP_PORT_BASE" \
GATEWAY_PORT_BASE="$GATEWAY_PORT_BASE" \
HOST_BIND_ADDRESS=127.0.0.1 \
AGGREGATE_RESULTS_PORT="$((HTTP_PORT_BASE + 98))" \
NODE_COUNT=4 CONNECTIVITY=2 PERIOD=4 WINDOW=1024 SIZE=7 \
USERS=1 SEGMENTS=2 WORDS=8 CLIENTS=2 ACTIVITY=30 ACTIVITY_DISABLED=0 \
"$NETWORK_DIR/local-compose.sh" up $UP_ARGUMENTS

RUN_DIR="$NETWORK_DIR/runs/$PROJECT"
cp "$NETWORK_DIR/compose.yml" "$OUTPUT/compose.yml"
cp "$RUN_DIR/peers.json" "$OUTPUT/peers.json"
git -C "$ROOT_DIR" rev-parse HEAD HEAD^{tree} > "$OUTPUT/source.txt"
: > "$OUTPUT/container-images.txt"
for id in $($RUNTIME ps -q --filter "label=com.docker.compose.project=$PROJECT"); do
  name="$($RUNTIME inspect --format '{{.Name}}' "$id")"
  image="$($RUNTIME inspect --format '{{.Image}}' "$id")"
  printf '%s %s %s\n' "$id" "$image" "$name" >> "$OUTPUT/container-images.txt"
done
if [ "$MODE" = "--build" ]; then
  python3 "$SCRIPT_DIR/image_manifest.py" write \
    --manifest "$IMAGE_MANIFEST" --source-root "$ROOT_DIR" --runtime "$RUNTIME" --project "$PROJECT"
elif [ "$IMAGE_MANIFEST" != "$OUTPUT/image-manifest.json" ]; then
  cp "$IMAGE_MANIFEST" "$OUTPUT/image-manifest.json"
fi

qualify() {
  phase="$1"
  python3 "$SCRIPT_DIR/sticky_readiness.py" \
    --peers "$RUN_DIR/peers.json" \
    --results "$RUN_DIR/results" \
    --output "$OUTPUT/$phase" \
    --router-port-base "$HTTP_PORT_BASE" \
    --gateway-port-base "$GATEWAY_PORT_BASE"
}

qualify initial

journal_ids="$($RUNTIME ps -q --filter "label=com.docker.compose.project=$PROJECT" --filter 'label=com.docker.compose.service=journal-0') \
$($RUNTIME ps -q --filter "label=com.docker.compose.project=$PROJECT" --filter 'label=com.docker.compose.service=journal-1') \
$($RUNTIME ps -q --filter "label=com.docker.compose.project=$PROJECT" --filter 'label=com.docker.compose.service=journal-2') \
$($RUNTIME ps -q --filter "label=com.docker.compose.project=$PROJECT" --filter 'label=com.docker.compose.service=journal-3')"
# shellcheck disable=SC2086
$RUNTIME restart $journal_ids > "$OUTPUT/restart.out"

healthy=0
for _ in $(seq 1 90); do
  healthy=1
  for id in $journal_ids; do
    status="$($RUNTIME inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$id")"
    [ "$status" = healthy ] || healthy=0
  done
  [ "$healthy" -eq 1 ] && break
  sleep 1
done
[ "$healthy" -eq 1 ] || { echo "Journals did not become healthy after restart" >&2; exit 1; }

qualify post-restart
printf 'PASS\n' > "$OUTPUT/status.txt"
echo "PASS: sticky readiness survived process-isolated federation and Journal restart"
