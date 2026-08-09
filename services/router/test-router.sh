#!/bin/sh
set -eu

RUNTIME=${CONTAINER_RUNTIME:-}
if [ -z "$RUNTIME" ]; then
    if command -v podman >/dev/null 2>&1; then RUNTIME=podman
    elif command -v docker >/dev/null 2>&1; then RUNTIME=docker
    else echo "Docker or Podman is required" >&2; exit 1
    fi
fi

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
tag="sync-router-test-$$"
network="sync-router-test-$$"
gateway_mock="sync-router-gateway-$$"
service_mock="sync-router-services-$$"
ui_mock="sync-router-ui-$$"
occupier="sync-router-occupier-$$"
ui_occupier="sync-router-ui-occupier-$$"
router="sync-router-http-$$"
tls_router="sync-router-tls-$$"
tmp=$(mktemp -d)
cleanup() {
    "$RUNTIME" rm -f "$router" "$tls_router" "$gateway_mock" "$service_mock" "$ui_mock" "$occupier" "$ui_occupier" >/dev/null 2>&1 || true
    "$RUNTIME" network rm "$network" >/dev/null 2>&1 || true
    "$RUNTIME" image rm "$tag" >/dev/null 2>&1 || true
    rm -rf "$tmp"
}
trap cleanup EXIT INT TERM

assert_status() {
    expected=$1
    url=$2
    shift 2
    actual=$(curl -ksS -o /dev/null -w '%{http_code}' "$@" "$url")
    [ "$actual" = "$expected" ] || {
        echo "Expected HTTP $expected from $url, got $actual" >&2
        exit 1
    }
}

assert_body() {
    expected=$1
    url=$2
    shift 2
    actual=$(curl -ksS "$@" "$url")
    [ "$actual" = "$expected" ] || {
        echo "Expected body '$expected' from $url, got '$actual'" >&2
        exit 1
    }
}

assert_unavailable() {
    url=$1
    shift
    actual=$(curl -ksS -o /dev/null -w '%{http_code}' "$@" "$url")
    case "$actual" in
        502|504) ;;
        *) echo "Expected HTTP 502/504 from $url, got $actual" >&2; exit 1 ;;
    esac
}

wait_aliases_withdrawn() {
    old_ip=$1
    shift
    started=$(date +%s)
    deadline=$((started + 15))
    while [ "$(date +%s)" -lt "$deadline" ]; do
        withdrawn=true
        for alias in "$@"; do
            answers=$("$RUNTIME" exec "$router" getent hosts "$alias" 2>/dev/null |
                awk '{print $1}' | sort -u || true)
            [ -z "$answers" ] || withdrawn=false
        done
        if [ "$withdrawn" = true ]; then
            echo "Retired aliases withdrew old address $old_ip in $(( $(date +%s) - started )) seconds"
            return 0
        fi
        sleep 1
    done
    echo "Retired aliases still resolved after 15 seconds (old address $old_ip)" >&2
    "$RUNTIME" exec "$router" cat /etc/resolv.conf >&2 || true
    for alias in "$@"; do "$RUNTIME" exec "$router" getent hosts "$alias" >&2 || true; done
    "$RUNTIME" logs "$router" >&2 || true
    exit 1
}

wait_replacement_ready() {
    container=$1
    ip=$2
    path=$3
    expected=$4
    shift 4
    started=$(date +%s)
    deadline=$((started + 15))
    while [ "$(date +%s)" -lt "$deadline" ]; do
        direct=$("$RUNTIME" exec "$router" wget -qO- -T 2 "http://$ip$path" 2>/dev/null || true)
        dns_ready=true
        for alias in "$@"; do
            answers=$("$RUNTIME" exec "$router" getent hosts "$alias" 2>/dev/null |
                awk '{print $1}' | sort -u || true)
            [ "$answers" = "$ip" ] || dns_ready=false
        done
        if [ "$direct" = "$expected" ] && [ "$dns_ready" = true ]; then
            echo "Replacement $container became directly ready with current DNS in $(( $(date +%s) - started )) seconds"
            return 0
        fi
        sleep 1
    done
    echo "Replacement $container did not become directly ready with current DNS within 15 seconds" >&2
    "$RUNTIME" inspect "$container" >&2 || true
    "$RUNTIME" exec "$router" cat /etc/resolv.conf >&2 || true
    for alias in "$@"; do "$RUNTIME" exec "$router" getent hosts "$alias" >&2 || true; done
    "$RUNTIME" logs "$container" >&2 || true
    "$RUNTIME" logs "$router" >&2 || true
    exit 1
}

wait_body() {
    expected=$1
    url=$2
    backend=$3
    started=$(date +%s)
    deadline=$((started + 15))
    while [ "$(date +%s)" -lt "$deadline" ]; do
        if [ "$(curl -ksS --max-time 2 "$url" 2>/dev/null || true)" = "$expected" ]; then
            echo "Router recovered replacement $backend in $(( $(date +%s) - started )) seconds"
            return 0
        fi
        sleep 1
    done
    echo "Router did not resolve the replacement $backend within 15 seconds" >&2
    "$RUNTIME" exec "$router" cat /etc/resolv.conf >&2 || true
    "$RUNTIME" logs "$router" >&2 || true
    exit 1
}

"$RUNTIME" build -q -t "$tag" "$root/services/router" >/dev/null
"$RUNTIME" network create "$network" >/dev/null
"$RUNTIME" run -d --name "$service_mock" --network "$network" \
    --network-alias journal --network-alias file-system \
    --entrypoint sh nginx:stable-alpine -c \
    "printf 'server { listen 80; listen 8080; location / { return 200 \"service\\n\"; } }' > /etc/nginx/conf.d/default.conf; exec nginx -g 'daemon off;'" >/dev/null
"$RUNTIME" run -d --name "$ui_mock" --network "$network" \
    --network-alias explorer --network-alias workbench \
    --entrypoint sh nginx:stable-alpine -c \
    "printf 'server { listen 80; location / { return 200 \"old-ui:\$uri:\$request_method\\n\"; } }' > /etc/nginx/conf.d/default.conf; exec nginx -g 'daemon off;'" >/dev/null
"$RUNTIME" run -d --name "$gateway_mock" --network "$network" \
    --network-alias gateway --entrypoint sh nginx:stable-alpine -c \
    "printf 'server { listen 80; location / { return 200 \"old-gateway:\$request_uri:\$request_method:\$http_cookie\\n\"; } }' > /etc/nginx/conf.d/default.conf; exec nginx -g 'daemon off;'" >/dev/null

"$RUNTIME" run -d --name "$router" --network "$network" -p 127.0.0.1::80 "$tag" >/dev/null
http_port=$("$RUNTIME" port "$router" 80/tcp | tail -1 | sed 's/.*://')
assert_status 404 "http://127.0.0.1:$http_port/metrics"
assert_status 404 "http://127.0.0.1:$http_port/metrics?source=public"
assert_status 404 "http://127.0.0.1:$http_port/%6Detrics"
for path in healthz readyz api/v1/general/size interface explorer workbench webdav/; do
    assert_status 200 "http://127.0.0.1:$http_port/$path"
done
assert_body 'old-gateway:/api/v1/general/get?proof=true:GET:session=before' \
    "http://127.0.0.1:$http_port/api/v1/general/get?proof=true" -H 'Cookie: session=before'
assert_body 'old-gateway:/api/v1/general/pin:POST:session=before' \
    "http://127.0.0.1:$http_port/api/v1/general/pin" -X POST -H 'Cookie: session=before'
assert_body 'old-gateway:/auth/login?return=%2Fexplorer:GET:' \
    "http://127.0.0.1:$http_port/auth/login?return=%2Fexplorer"
assert_body 'old-gateway:/?source=router:GET:' \
    "http://127.0.0.1:$http_port/gateway?source=router"
assert_body 'old-gateway:/gateway-logo.png?v=1:GET:' \
    "http://127.0.0.1:$http_port/gateway-logo.png?v=1"
assert_body 'old-gateway:/docs?section=api:GET:' \
    "http://127.0.0.1:$http_port/docs?section=api"
assert_body 'old-gateway:/healthz?full=true:GET:' \
    "http://127.0.0.1:$http_port/healthz?full=true"
assert_body 'old-gateway:/readyz?full=true:GET:' \
    "http://127.0.0.1:$http_port/readyz?full=true"
"$RUNTIME" exec "$router" grep -A1 'location = /metrics' /etc/nginx/includes/nginx.routes.inc | grep -q 'return 404'
"$RUNTIME" exec "$router" grep -q '^resolver .* valid=5s ipv6=off;' /etc/nginx/includes/nginx.routes.inc
"$RUNTIME" exec "$router" grep -q 'proxy_pass http://\$gateway_upstream;' /etc/nginx/includes/nginx.routes.inc
"$RUNTIME" exec "$router" grep -q '^resolver .* valid=1s ipv6=off;' /etc/nginx/conf.d/sync-ui-upstreams.conf
"$RUNTIME" exec "$router" grep -q 'proxy_pass http://sync_explorer_upstream/;' /etc/nginx/includes/nginx.routes.inc
"$RUNTIME" exec "$router" grep -q 'proxy_pass http://sync_workbench_upstream/;' /etc/nginx/includes/nginx.routes.inc
"$RUNTIME" exec "$router" grep -q '^    zone sync_explorer_upstream 64k;' /etc/nginx/conf.d/sync-ui-upstreams.conf
"$RUNTIME" exec "$router" grep -q '^    server explorer:80 resolve max_fails=0;' /etc/nginx/conf.d/sync-ui-upstreams.conf
"$RUNTIME" exec "$router" grep -q '^    zone sync_workbench_upstream 64k;' /etc/nginx/conf.d/sync-ui-upstreams.conf
"$RUNTIME" exec "$router" grep -q '^    server workbench:80 resolve max_fails=0;' /etc/nginx/conf.d/sync-ui-upstreams.conf

# Recreate the gateway at a different address while the router and every other
# backend remain running. The failed pin is not retried against the replacement.
old_ip=$("$RUNTIME" inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$gateway_mock")
"$RUNTIME" rm -f "$gateway_mock" >/dev/null
assert_unavailable "http://127.0.0.1:$http_port/healthz"
assert_unavailable "http://127.0.0.1:$http_port/api/v1/general/pin" -X POST
for path in interface explorer workbench webdav/; do
    assert_status 200 "http://127.0.0.1:$http_port/$path"
done
wait_aliases_withdrawn "$old_ip" gateway
"$RUNTIME" run -d --name "$occupier" --network "$network" \
    --entrypoint sleep nginx:stable-alpine 60 >/dev/null
"$RUNTIME" run -d --name "$gateway_mock" --network "$network" \
    --network-alias gateway --entrypoint sh nginx:stable-alpine -c \
    "printf 'server { listen 80; location / { return 200 \"new-gateway:\$request_uri:\$request_method:\$http_cookie\\n\"; } }' > /etc/nginx/conf.d/default.conf; exec nginx -g 'daemon off;'" >/dev/null
new_ip=$("$RUNTIME" inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$gateway_mock")
[ "$old_ip" != "$new_ip" ] || {
    echo "Gateway recreation did not change its test address: $old_ip" >&2
    exit 1
}
wait_replacement_ready "$gateway_mock" "$new_ip" /healthz \
    'new-gateway:/healthz:GET:' gateway
wait_body 'new-gateway:/healthz:GET:' "http://127.0.0.1:$http_port/healthz" gateway
assert_body 'new-gateway:/api/v1/general/get?proof=true:GET:session=after' \
    "http://127.0.0.1:$http_port/api/v1/general/get?proof=true" -H 'Cookie: session=after'
assert_body 'new-gateway:/api/v1/general/pin:POST:session=after' \
    "http://127.0.0.1:$http_port/api/v1/general/pin" -X POST -H 'Cookie: session=after'
pin_requests=$("$RUNTIME" logs "$gateway_mock" 2>&1 | grep -c 'POST /api/v1/general/pin ' || true)
[ "$pin_requests" = 1 ] || {
    echo "Expected one replacement-gateway pin request, got $pin_requests" >&2
    exit 1
}

# Recreate both UI services at a different address without restarting Router.
old_ui_ip=$("$RUNTIME" inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$ui_mock")
"$RUNTIME" rm -f "$ui_mock" >/dev/null
assert_unavailable "http://127.0.0.1:$http_port/explorer/deep"
assert_unavailable "http://127.0.0.1:$http_port/workbench/deep"
wait_aliases_withdrawn "$old_ui_ip" explorer workbench
"$RUNTIME" run -d --name "$ui_occupier" --network "$network" \
    --entrypoint sleep nginx:stable-alpine 60 >/dev/null
"$RUNTIME" run -d --name "$ui_mock" --network "$network" \
    --network-alias explorer --network-alias workbench \
    --entrypoint sh nginx:stable-alpine -c \
    "printf 'server { listen 80; location / { return 200 \"new-ui:\$uri:\$request_method\\n\"; } }' > /etc/nginx/conf.d/default.conf; exec nginx -g 'daemon off;'" >/dev/null
new_ui_ip=$("$RUNTIME" inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$ui_mock")
[ "$old_ui_ip" != "$new_ui_ip" ] || {
    echo "UI recreation did not change its test address: $old_ui_ip" >&2
    exit 1
}
wait_replacement_ready "$ui_mock" "$new_ui_ip" /deep \
    'new-ui:/deep:GET' explorer workbench
wait_body 'new-ui:/deep:GET' "http://127.0.0.1:$http_port/explorer/deep" explorer
wait_body 'new-ui:/deep:GET' "http://127.0.0.1:$http_port/workbench/deep" workbench
assert_status 200 "http://127.0.0.1:$http_port/interface"
assert_status 200 "http://127.0.0.1:$http_port/webdav/"

openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj /CN=localhost \
    -keyout "$tmp/tls.key" -out "$tmp/tls.crt" >/dev/null 2>&1
"$RUNTIME" run -d --name "$tls_router" --network "$network" \
    -p 127.0.0.1::80 -p 127.0.0.1::443 \
    -v "$tmp/tls.crt:/etc/nginx/certs/tls.crt:ro,Z" \
    -v "$tmp/tls.key:/etc/nginx/certs/tls.key:ro,Z" "$tag" >/dev/null
plain_port=$("$RUNTIME" port "$tls_router" 80/tcp | tail -1 | sed 's/.*://')
tls_port=$("$RUNTIME" port "$tls_router" 443/tcp | tail -1 | sed 's/.*://')
assert_status 404 "http://127.0.0.1:$plain_port/metrics"
assert_status 404 "http://127.0.0.1:$plain_port/metrics?source=public"
assert_status 404 "http://127.0.0.1:$plain_port/%6Detrics"
assert_status 301 "http://127.0.0.1:$plain_port/healthz"
assert_status 404 "https://127.0.0.1:$tls_port/metrics"
assert_status 404 "https://127.0.0.1:$tls_port/metrics?source=public"
assert_status 404 "https://127.0.0.1:$tls_port/%6Detrics"
for path in healthz readyz api/v1/general/size explorer workbench webdav/; do
    assert_status 200 "https://127.0.0.1:$tls_port/$path"
done

# The checked-in include is the non-templated variant used by direct nginx builds.
grep -A1 'location = /metrics' "$root/services/router/nginx.routes.conf" | grep -q 'return 404'
# TLS plaintext must not redirect the exact metrics path.
grep -A1 'location = /metrics' "$root/services/router/nginx.tls.conf" | grep -q 'return 404'
# Gateway metrics stay reachable only through the private Compose network.
if awk '/^  gateway:/{inside=1; next} /^  [a-zA-Z0-9_-]+:/{inside=0} inside' \
    "$root/deploy/compose/general/compose.yaml" | grep -q '^    ports:'; then
    echo "Gateway must not publish a host port" >&2
    exit 1
fi

echo "Router HTTP/TLS boundary checks passed"
