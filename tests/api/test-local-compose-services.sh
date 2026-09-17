#!/bin/sh
set -eu

ROOT_DIR="$(git -C "$(dirname -- "$0")" rev-parse --show-toplevel)"
HELPER="$ROOT_DIR/tests/api/local-compose.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

cat > "$TMP_DIR/compose" <<'EOF'
#!/bin/sh
printf '%s\n' "$*" >> "$LOCAL_COMPOSE_TEST_LOG"

case " $* " in
    *" config --volumes "*)
        case "$LOCAL_COMPOSE_TEST_DISCOVERY_MODE" in
            primary) printf '%s\n' database identity-provider-data ;;
            unsupported|plain-failure)
                echo "podman-compose: error: unrecognized arguments: --volumes" >&2
                exit 2
                ;;
            primary-failure)
                echo "mock unrelated config failure" >&2
                exit 17
                ;;
        esac
        ;;
    *" config ")
        if [ "$LOCAL_COMPOSE_TEST_DISCOVERY_MODE" = "plain-failure" ]; then
            echo "mock plain config failure" >&2
            exit 19
        fi
        cat "$LOCAL_COMPOSE_TEST_STATE/normalized-config"
        ;;
    *" down -v --remove-orphans "*)
        count=0
        if [ -f "$LOCAL_COMPOSE_TEST_STATE/down-count" ]; then
            count="$(cat "$LOCAL_COMPOSE_TEST_STATE/down-count")"
        fi
        count=$((count + 1))
        printf '%s\n' "$count" > "$LOCAL_COMPOSE_TEST_STATE/down-count"

        if [ "$count" -eq 1 ] || [ "$LOCAL_COMPOSE_TEST_DOWN_MODE" = "complete" ]; then
            : > "$LOCAL_COMPOSE_TEST_STATE/containers"
            : > "$LOCAL_COMPOSE_TEST_STATE/networks"
            awk -v prefix="${COMPOSE_PROJECT_NAME}_" 'index($0, prefix) != 1' \
                "$LOCAL_COMPOSE_TEST_STATE/volumes" > "$LOCAL_COMPOSE_TEST_STATE/volumes.next"
            mv "$LOCAL_COMPOSE_TEST_STATE/volumes.next" "$LOCAL_COMPOSE_TEST_STATE/volumes"
        elif [ "$LOCAL_COMPOSE_TEST_DOWN_MODE" = "nonzero" ]; then
            : > "$LOCAL_COMPOSE_TEST_STATE/containers"
            : > "$LOCAL_COMPOSE_TEST_STATE/networks"
            awk -v prefix="${COMPOSE_PROJECT_NAME}_" 'index($0, prefix) != 1' \
                "$LOCAL_COMPOSE_TEST_STATE/volumes" > "$LOCAL_COMPOSE_TEST_STATE/volumes.next"
            mv "$LOCAL_COMPOSE_TEST_STATE/volumes.next" "$LOCAL_COMPOSE_TEST_STATE/volumes"
            echo "mock Compose down diagnostic" >&2
            exit 23
        elif [ "$LOCAL_COMPOSE_TEST_DOWN_MODE" = "partial" ]; then
            : > "$LOCAL_COMPOSE_TEST_STATE/containers"
            : > "$LOCAL_COMPOSE_TEST_STATE/networks"
        fi
        ;;
    *" up "*)
        printf '%s\n' container-id > "$LOCAL_COMPOSE_TEST_STATE/containers"
        printf '%s\n' network-id > "$LOCAL_COMPOSE_TEST_STATE/networks"
        printf '%s\n' \
            "${COMPOSE_PROJECT_NAME}_database" \
            "${COMPOSE_PROJECT_NAME}_identity-provider-data" \
            unrelated_volume > "$LOCAL_COMPOSE_TEST_STATE/volumes"
        if [ "$LOCAL_COMPOSE_TEST_PRIMARY_STATUS" -ne 0 ]; then
            echo "mock primary failure" >&2
            exit "$LOCAL_COMPOSE_TEST_PRIMARY_STATUS"
        fi
        ;;
esac
EOF
chmod +x "$TMP_DIR/compose"

cat > "$TMP_DIR/runtime" <<'EOF'
#!/bin/sh
case "${1-} ${2-}" in
    "ps -aq")
        cat "$LOCAL_COMPOSE_TEST_STATE/containers"
        ;;
    "network ls")
        cat "$LOCAL_COMPOSE_TEST_STATE/networks"
        ;;
    "volume inspect")
        grep -Fxq "${3-}" "$LOCAL_COMPOSE_TEST_STATE/volumes"
        ;;
    "volume rm")
        volume="${3-}"
        printf '%s\n' "$volume" >> "$LOCAL_COMPOSE_TEST_STATE/volume-rm.log"
        if [ "$volume" = "${LOCAL_COMPOSE_TEST_REMOVE_FAILURE-}" ]; then
            echo "mock volume removal diagnostic: $volume" >&2
            exit 31
        fi
        awk -v volume="$volume" '$0 != volume' \
            "$LOCAL_COMPOSE_TEST_STATE/volumes" > "$LOCAL_COMPOSE_TEST_STATE/volumes.next"
        mv "$LOCAL_COMPOSE_TEST_STATE/volumes.next" "$LOCAL_COMPOSE_TEST_STATE/volumes"
        printf '%s\n' "$volume"
        ;;
    *)
        echo "unexpected runtime command: $*" >&2
        exit 90
        ;;
esac
EOF
chmod +x "$TMP_DIR/runtime"

cat > "$TMP_DIR/python3" <<'EOF'
#!/bin/sh
exit 0
EOF
chmod +x "$TMP_DIR/python3"
touch "$TMP_DIR/images.json"

run_helper() {
    name="$1"
    skip_file_system="$2"
    down_mode="$3"
    primary_status="$4"
    remove_failure="$5"
    discovery_mode="${6:-primary}"
    normalized_fixture="${7:-valid}"
    state="$TMP_DIR/state-$name"
    mkdir "$state"
    : > "$state/containers"
    : > "$state/networks"
    : > "$state/volumes"
    : > "$state/volume-rm.log"
    case "$normalized_fixture" in
        valid)
            cat > "$state/normalized-config" <<'EOF'
services:
  journal:
    image: example
volumes:
  database: null
  identity-provider-data: null
EOF
            ;;
        empty)
            cat > "$state/normalized-config" <<'EOF'
services:
  journal:
    image: example
volumes: {}
EOF
            ;;
        malformed)
            cat > "$state/normalized-config" <<'EOF'
services: {}
volumes:
   database: null
EOF
            ;;
        duplicate)
            cat > "$state/normalized-config" <<'EOF'
services: {}
volumes:
  database: null
  database: null
EOF
            ;;
        ambiguous)
            cat > "$state/normalized-config" <<'EOF'
services: {}
volumes: &shared
  database: null
EOF
            ;;
    esac
    log="$TMP_DIR/$name.compose.log"

    set +e
    PATH="$TMP_DIR:$PATH" \
    CONTAINER_RUNTIME="$TMP_DIR/runtime" \
    CONTAINER_COMPOSE="$TMP_DIR/compose" \
    LOCAL_COMPOSE_TEST_LOG="$log" \
    LOCAL_COMPOSE_TEST_STATE="$state" \
    LOCAL_COMPOSE_TEST_DOWN_MODE="$down_mode" \
    LOCAL_COMPOSE_TEST_PRIMARY_STATUS="$primary_status" \
    LOCAL_COMPOSE_TEST_REMOVE_FAILURE="$remove_failure" \
    LOCAL_COMPOSE_TEST_DISCOVERY_MODE="$discovery_mode" \
    LOCAL_COMPOSE_SKIP_BUILD=1 \
    LOCAL_COMPOSE_SKIP_FILE_SYSTEM="$skip_file_system" \
    IMAGE_MANIFEST="$TMP_DIR/images.json" \
    COMPOSE_PROJECT_NAME="sync-local-compose-$name" \
    "$HELPER" up > "$TMP_DIR/$name.out" 2> "$TMP_DIR/$name.err"
    status=$?
    set -e
    printf '%s\n' "$status" > "$TMP_DIR/$name.status"
}

assert_status() {
    name="$1"
    expected="$2"
    actual="$(cat "$TMP_DIR/$name.status")"
    if [ "$actual" -ne "$expected" ]; then
        echo "FAIL: $name expected status $expected, got $actual" >&2
        cat "$TMP_DIR/$name.err" >&2
        exit 1
    fi
}

assert_unrelated_untouched() {
    name="$1"
    state="$TMP_DIR/state-$name"
    grep -Fxq unrelated_volume "$state/volumes"
    if grep -Fxq unrelated_volume "$state/volume-rm.log"; then
        echo "FAIL: $name attempted to remove an unrelated volume" >&2
        exit 1
    fi
}

run_helper ordinary 0 complete 0 ""
assert_status ordinary 0
if grep -Fq "Cleanup fallback:" "$TMP_DIR/ordinary.err"; then
    echo "FAIL: ordinary complete down unexpectedly used fallback removal" >&2
    exit 1
fi
assert_unrelated_untouched ordinary

run_helper fallback-valid 0 complete 0 "" unsupported valid
assert_status fallback-valid 0
grep -Fq "provider lacks 'config --volumes'" "$TMP_DIR/fallback-valid.err"
grep -Fq " config --volumes" "$TMP_DIR/fallback-valid.compose.log"
grep -Eq " config$" "$TMP_DIR/fallback-valid.compose.log"
assert_unrelated_untouched fallback-valid

run_helper fallback-empty 0 complete 0 "" unsupported empty
assert_status fallback-empty 0
assert_unrelated_untouched fallback-empty

run_helper primary-failure 0 complete 0 "" primary-failure valid
assert_status primary-failure 17
grep -Fq "mock unrelated config failure" "$TMP_DIR/primary-failure.err"
if grep -Eq " config$" "$TMP_DIR/primary-failure.compose.log"; then
    echo "FAIL: unrelated primary discovery failure invoked normalized-config fallback" >&2
    exit 1
fi

run_helper plain-failure 0 complete 0 "" plain-failure valid
assert_status plain-failure 19
grep -Fq "mock plain config failure" "$TMP_DIR/plain-failure.err"

for fixture in malformed duplicate ambiguous; do
    run_helper "fallback-$fixture" 0 complete 0 "" unsupported "$fixture"
    assert_status "fallback-$fixture" 1
    grep -Fq "malformed normalized top-level volumes mapping" "$TMP_DIR/fallback-$fixture.err"
done

run_helper provider-nonzero 0 nonzero 0 ""
assert_status provider-nonzero 23
grep -Fq "mock Compose down diagnostic" "$TMP_DIR/provider-nonzero.err"
grep -Fq "Compose down failed" "$TMP_DIR/provider-nonzero.err"
assert_unrelated_untouched provider-nonzero

run_helper partial 0 partial 0 ""
assert_status partial 0
grep -Fq "Cleanup residue: declared volume 'sync-local-compose-partial_database'" "$TMP_DIR/partial.err"
grep -Fq "Cleanup fallback: removing declared project volume 'sync-local-compose-partial_database'" "$TMP_DIR/partial.err"
grep -Fxq sync-local-compose-partial_database "$TMP_DIR/state-partial/volume-rm.log"
grep -Fxq sync-local-compose-partial_identity-provider-data "$TMP_DIR/state-partial/volume-rm.log"
if grep -Fq "sync-local-compose-partial_" "$TMP_DIR/state-partial/volumes"; then
    echo "FAIL: bounded fallback did not remove every declared project volume" >&2
    exit 1
fi
assert_unrelated_untouched partial

run_helper remove-failure 0 partial 0 sync-local-compose-remove-failure_database
assert_status remove-failure 31
grep -Fq "mock volume removal diagnostic: sync-local-compose-remove-failure_database" "$TMP_DIR/remove-failure.err"
grep -Fq "declared project volume remains after cleanup" "$TMP_DIR/remove-failure.err"
assert_unrelated_untouched remove-failure

run_helper journey-failure 0 nonzero 42 ""
assert_status journey-failure 42
grep -Fq "mock primary failure" "$TMP_DIR/journey-failure.err"
grep -Fq "cleanup could not establish an empty project" "$TMP_DIR/journey-failure.err"
assert_unrelated_untouched journey-failure

core_profiles="--profile explorer --profile workbench"
core_services="journal explorer workbench identity-provider gateway router"
grep -Fq " $core_profiles --profile file-system up $core_services file-system" "$TMP_DIR/ordinary.compose.log"
grep -Fq " $core_profiles --profile file-system down -v --remove-orphans" "$TMP_DIR/ordinary.compose.log"

run_helper skip-file-system 1 complete 0 ""
assert_status skip-file-system 0
grep -Fq " $core_profiles up $core_services" "$TMP_DIR/skip-file-system.compose.log"
grep -Fq " $core_profiles down -v --remove-orphans" "$TMP_DIR/skip-file-system.compose.log"
if grep ' up ' "$TMP_DIR/skip-file-system.compose.log" | grep -Fq 'file-system'; then
    echo "FAIL: skip-file-system startup still targets file-system" >&2
    exit 1
fi
if grep ' down ' "$TMP_DIR/skip-file-system.compose.log" | grep -Fq 'file-system'; then
    echo "FAIL: skip-file-system teardown still targets file-system profile" >&2
    exit 1
fi

printf '%s\n' "PASS: local Compose service targeting and bounded teardown behavior."
