#!/bin/sh
set -eu

ORIGIN="${ORIGIN:?ORIGIN is required (e.g. https://example.com or http://localhost:8192)}"
SCHEME="${ORIGIN%%://*}"
DOMAIN="${ORIGIN#*://}"
COOKIE_DOMAIN="${DOMAIN%%:*}"
KRATOS_COOKIE_SECRET="${KRATOS_COOKIE_SECRET:-changeme-32-chars-minimum-here}"
KRATOS_CONFIG_DIR="${KRATOS_CONFIG_DIR:-/etc/config/kratos}"
KRATOS_CONFIG_FILE="${KRATOS_CONFIG_FILE:-$KRATOS_CONFIG_DIR/kratos.yml}"
KRATOS_DATA_DIR="${KRATOS_DATA_DIR:-/var/lib/kratos}"
KRATOS_IDENTITY_SCHEMA_URL="${KRATOS_IDENTITY_SCHEMA_URL:-file://$KRATOS_CONFIG_DIR/identity.schema.json}"

if [ "$SCHEME" = "https" ]; then
    DEV_FLAG=""
    echo "Identity provider mode: production (https)"
else
    DEV_FLAG="--dev"
    echo "Identity provider mode: dev (http)"
fi

mkdir -p "$KRATOS_CONFIG_DIR"

echo "Identity provider recovery disabled: username-only identities require administrator-mediated password resets"

cat > "$KRATOS_CONFIG_FILE" <<EOF
version: v0.13.0

dsn: sqlite://$KRATOS_DATA_DIR/db.sqlite?_fk=true

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
    - ${SCHEME}://${DOMAIN}/gateway
    - ${SCHEME}://${DOMAIN}/explorer
    - ${SCHEME}://${DOMAIN}/workbench
    - ${SCHEME}://${DOMAIN}/api/v1/docs

  methods:
    password:
      enabled: true

  flows:
    error:
      ui_url: ${SCHEME}://${DOMAIN}/auth/error
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
      enabled: false
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
      url: $KRATOS_IDENTITY_SCHEMA_URL
EOF

if [ "${KRATOS_CONFIG_DRY_RUN:-0}" = "1" ]; then
    echo "Wrote Kratos config to $KRATOS_CONFIG_FILE"
    exit 0
fi

kratos migrate sql -c "$KRATOS_CONFIG_FILE" -e -y

ADMIN_USERNAME="${ADMIN_USERNAME:-}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-${SECRET:-}}"

if [ -n "$ADMIN_USERNAME" ] && [ -n "$ADMIN_PASSWORD" ]; then
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
            ESC_PASS=$(printf '%s' "$ADMIN_PASSWORD" | sed 's/\\/\\\\/g; s/"/\\"/g')
            BODY="{\"schema_id\":\"default\",\"traits\":{\"username\":\"${ESC_USER}\"},\"credentials\":{\"password\":{\"config\":{\"password\":\"${ESC_PASS}\"}}}}"
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
