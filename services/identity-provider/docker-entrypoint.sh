#!/bin/sh
set -eu

DOMAIN="${DOMAIN:?DOMAIN is required}"
COOKIE_DOMAIN="${DOMAIN%%:*}"
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
    - ${SCHEME}://${DOMAIN}/api/v1/docs

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
      after:
        password:
          hooks:
            - hook: session
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
  domain: ${COOKIE_DOMAIN}
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

ADMIN_USERNAME="${ADMIN_USERNAME:-}"
ADMIN_EMAIL="${ADMIN_EMAIL:-}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-${SECRET:-}}"

if [ -n "$ADMIN_USERNAME" ] && [ -n "$ADMIN_EMAIL" ] && [ -n "$ADMIN_PASSWORD" ]; then
    echo "Seeding admin identity '${ADMIN_USERNAME}'..."

    "$@" $DEV_FLAG &
    KRATOS_SEED_PID=$!

    # Wait for Kratos admin API (up to 60s)
    ATTEMPTS=0
    until wget -qO- http://localhost:4434/health/ready > /dev/null 2>&1; do
        ATTEMPTS=$((ATTEMPTS + 1))
        if [ "$ATTEMPTS" -ge 30 ]; then
            echo "Timed out waiting for Kratos admin API; skipping admin seed" >&2
            break
        fi
        sleep 2
    done

    if wget -qO- http://localhost:4434/health/ready > /dev/null 2>&1; then
        EXISTING=$(wget -qO- "http://localhost:4434/admin/identities?credentials_identifier=${ADMIN_USERNAME}" 2>/dev/null || echo "[]")
        if [ "$EXISTING" = "[]" ]; then
            ESC_USER=$(printf '%s' "$ADMIN_USERNAME" | sed 's/\\/\\\\/g; s/"/\\"/g')
            ESC_EMAIL=$(printf '%s' "$ADMIN_EMAIL" | sed 's/\\/\\\\/g; s/"/\\"/g')
            ESC_PASS=$(printf '%s' "$ADMIN_PASSWORD" | sed 's/\\/\\\\/g; s/"/\\"/g')
            BODY="{\"schema_id\":\"default\",\"traits\":{\"email\":\"${ESC_EMAIL}\",\"username\":\"${ESC_USER}\"},\"credentials\":{\"password\":{\"config\":{\"password\":\"${ESC_PASS}\"}}}}"
            if wget -qO- --header="Content-Type: application/json" --post-data="$BODY" http://localhost:4434/admin/identities > /dev/null 2>&1; then
                echo "Admin identity '${ADMIN_USERNAME}' created"
            else
                echo "Failed to create admin identity '${ADMIN_USERNAME}'" >&2
            fi
        else
            echo "Admin identity '${ADMIN_USERNAME}' already exists"
        fi
    fi

    kill "$KRATOS_SEED_PID" 2>/dev/null || true
    wait "$KRATOS_SEED_PID" 2>/dev/null || true
fi

exec "$@" $DEV_FLAG
