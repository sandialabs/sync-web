#!/bin/sh
set -eu

# Generate runtime environment configuration atomically.
cat > /usr/share/nginx/html/env-config.js.tmp << EOF
window._env_ = {
  SYNC_WORKBENCH_ENDPOINT: "${SYNC_WORKBENCH_ENDPOINT:-}",
};
EOF
mv /usr/share/nginx/html/env-config.js.tmp /usr/share/nginx/html/env-config.js

exec /docker-entrypoint.sh "$@"
