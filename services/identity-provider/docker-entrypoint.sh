#!/bin/sh
set -eu

DOMAIN="${DOMAIN:-localhost}"
KRATOS_COOKIE_SECRET="${KRATOS_COOKIE_SECRET:-changeme-32-chars-minimum-here}"
CERT_FILE="${TLS_CERT_FILE:-/etc/kratos/certs/tls.crt}"
KEY_FILE="${TLS_KEY_FILE:-/etc/kratos/certs/tls.key}"

if [ -f "$CERT_FILE" ] \
    && [ -f "$KEY_FILE" ] \
    && grep -q "BEGIN CERTIFICATE" "$CERT_FILE" \
    && grep -Eq "BEGIN (EC |RSA |)PRIVATE KEY" "$KEY_FILE"; then
    SCHEME="https"
    DEV_FLAG=""
    echo "Identity provider mode: production (TLS detected)"
else
    SCHEME="http"
    DEV_FLAG="--dev"
    echo "Identity provider mode: dev (no TLS certs, HTTP only)"
fi

cat > /etc/config/kratos/kratos.yml <<EOF
version: v0.13.0

dsn: sqlite:///var/lib/kratos/db.sqlite?_fk=true

serve:
  public:
    base_url: ${SCHEME}://${DOMAIN}/auth/.ory/
    cors:
      enabled: true
      allowed_origins:
        - ${SCHEME}://${DOMAIN}
  admin:
    base_url: http://identity-provider:4434/

selfservice:
  default_browser_return_url: ${SCHEME}://${DOMAIN}/
  allowed_return_urls:
    - ${SCHEME}://${DOMAIN}/

  methods:
    password:
      enabled: true

  flows:
    login:
      ui_url: ${SCHEME}://${DOMAIN}/auth/login
      lifespan: 10m
    registration:
      ui_url: ${SCHEME}://${DOMAIN}/auth/registration
      lifespan: 10m
    recovery:
      enabled: true
      ui_url: ${SCHEME}://${DOMAIN}/auth/recovery
    settings:
      ui_url: ${SCHEME}://${DOMAIN}/auth/settings

log:
  level: info
  format: text

secrets:
  cookie:
    - ${KRATOS_COOKIE_SECRET}

cookies:
  domain: ${DOMAIN}
  path: /
  same_site: Lax

identity:
  default_schema_id: default
  schemas:
    - id: default
      url: file:///etc/config/kratos/identity.schema.json

courier:
  smtp:
    connection_uri: smtps://test:test@mailslurper:1025/?skip_ssl_verify=true
EOF

kratos migrate sql -c /etc/config/kratos/kratos.yml -e -y

exec "$@" $DEV_FLAG
