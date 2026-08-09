#!/bin/sh
set -eu

CERT_FILE="${TLS_CERT_FILE:-/etc/nginx/certs/tls.crt}"
KEY_FILE="${TLS_KEY_FILE:-/etc/nginx/certs/tls.key}"
JOURNAL_HOST="${ROUTER_JOURNAL_HOST:-journal}"
GATEWAY_HOST="${ROUTER_GATEWAY_HOST:-gateway}"
EXPLORER_HOST="${ROUTER_EXPLORER_HOST:-explorer}"
WORKBENCH_HOST="${ROUTER_WORKBENCH_HOST:-workbench}"
FILE_SYSTEM_HOST="${ROUTER_FILE_SYSTEM_HOST:-file-system:8080}"
DNS_RESOLVERS=$(awk '
    $1 == "nameserver" {
        address = $2
        if (index(address, ":")) address = "[" address "]"
        resolvers = resolvers (resolvers ? " " : "") address
    }
    END { print resolvers }
' /etc/resolv.conf)
if [ -z "$DNS_RESOLVERS" ]; then
    echo "Router requires a container DNS resolver" >&2
    exit 1
fi

cat > /etc/nginx/conf.d/sync-ui-upstreams.conf <<EOF
resolver ${DNS_RESOLVERS} valid=1s ipv6=off;
resolver_timeout 1s;

upstream sync_explorer_upstream {
    zone sync_explorer_upstream 64k;
    server ${EXPLORER_HOST}:80 resolve max_fails=0;
}

upstream sync_workbench_upstream {
    zone sync_workbench_upstream 64k;
    server ${WORKBENCH_HOST}:80 resolve max_fails=0;
}
EOF

cat > /etc/nginx/includes/nginx.routes.inc <<EOF
resolver ${DNS_RESOLVERS} valid=5s ipv6=off;
resolver_timeout 2s;
set \$gateway_upstream "${GATEWAY_HOST}";

location = / {
    try_files /index.html =404;
}

location = /webdav-guide {
    try_files /webdav-guide.html =404;
}

location /interface {
    proxy_pass http://${JOURNAL_HOST}/interface;
}

location /api/ {
    proxy_pass http://\$gateway_upstream;
}

location /auth/ {
    proxy_pass http://\$gateway_upstream;
}

location = /gateway {
    proxy_pass http://\$gateway_upstream/\$is_args\$args;
}

location = /gateway-logo.png {
    proxy_pass http://\$gateway_upstream/gateway-logo.png\$is_args\$args;
}

location = /docs {
    proxy_pass http://\$gateway_upstream/docs\$is_args\$args;
}

location = /healthz {
    proxy_pass http://\$gateway_upstream/healthz\$is_args\$args;
}

location = /readyz {
    proxy_pass http://\$gateway_upstream/readyz\$is_args\$args;
}

location = /metrics {
    return 404;
}

location /explorer {
    proxy_pass http://sync_explorer_upstream/;
}

location /workbench {
    proxy_pass http://sync_workbench_upstream/;
}

location = /webdav {
    proxy_pass http://${FILE_SYSTEM_HOST};
}

location /webdav/ {
    proxy_pass http://${FILE_SYSTEM_HOST};
}
EOF

if [ -f "$CERT_FILE" ] \
    && [ -f "$KEY_FILE" ] \
    && grep -q "BEGIN CERTIFICATE" "$CERT_FILE" \
    && grep -Eq "BEGIN (EC |RSA |)PRIVATE KEY" "$KEY_FILE"; then
    cp /etc/nginx/templates/nginx.tls.conf /etc/nginx/conf.d/default.conf
    echo "Router mode: TLS (cert: $CERT_FILE, key: $KEY_FILE)"
else
    cp /etc/nginx/templates/nginx.http.conf /etc/nginx/conf.d/default.conf
    echo "Router mode: HTTP (TLS cert/key not found)"
fi

exec "$@"
