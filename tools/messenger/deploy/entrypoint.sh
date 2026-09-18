#!/bin/sh
set -eu

: "${SYNC_GATEWAY_ORIGIN:?SYNC_GATEWAY_ORIGIN is required}"
: "${SYNC_GATEWAY_BROWSER_ORIGIN:?SYNC_GATEWAY_BROWSER_ORIGIN is required}"
: "${MESSENGER_PORT:=8280}"
: "${MESSENGER_POLL_ACTIVE_MS:=3000}"
: "${MESSENGER_POLL_IDLE_MS:=15000}"

if ! printf '%s' "$SYNC_GATEWAY_ORIGIN" | grep -Eq '^https?://[A-Za-z0-9._:-]+$'; then
  echo "SYNC_GATEWAY_ORIGIN must be an http(s) origin without a path" >&2
  exit 2
fi
if ! printf '%s' "$SYNC_GATEWAY_BROWSER_ORIGIN" | grep -Eq '^https?://localhost(:[0-9]+)?$'; then
  echo "SYNC_GATEWAY_BROWSER_ORIGIN must use localhost so the Kratos cookie is shared across ports" >&2
  exit 2
fi
if ! printf '%s' "$MESSENGER_PORT" | grep -Eq '^[0-9]+$' || [ "$MESSENGER_PORT" -lt 1024 ] || [ "$MESSENGER_PORT" -gt 65535 ]; then
  echo "MESSENGER_PORT must be an integer from 1024 to 65535" >&2
  exit 2
fi
for interval in "$MESSENGER_POLL_ACTIVE_MS" "$MESSENGER_POLL_IDLE_MS"; do
  if ! printf '%s' "$interval" | grep -Eq '^[0-9]+$' || [ "$interval" -lt 1000 ] || [ "$interval" -gt 300000 ]; then
    echo "Messenger poll intervals must be integers from 1000 to 300000" >&2
    exit 2
  fi
done

export SYNC_GATEWAY_ORIGIN SYNC_GATEWAY_BROWSER_ORIGIN MESSENGER_PORT
export MESSENGER_POLL_ACTIVE_MS MESSENGER_POLL_IDLE_MS

envsubst '${SYNC_GATEWAY_ORIGIN} ${SYNC_GATEWAY_BROWSER_ORIGIN} ${MESSENGER_PORT}' \
  < /opt/messenger/nginx.conf.template \
  > /etc/nginx/conf.d/default.conf

envsubst '${SYNC_GATEWAY_BROWSER_ORIGIN} ${MESSENGER_POLL_ACTIVE_MS} ${MESSENGER_POLL_IDLE_MS}' \
  < /usr/share/nginx/html/config.js.template \
  > /usr/share/nginx/html/config.js

exec nginx -g 'daemon off;'
