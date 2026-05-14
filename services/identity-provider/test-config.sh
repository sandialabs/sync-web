#!/usr/bin/env sh
set -eu

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

cp "$(dirname "$0")/identity.schema.json" "$tmp_dir/identity.schema.json"

DOMAIN=localhost:8192 \
KRATOS_CONFIG_DIR="$tmp_dir" \
KRATOS_DATA_DIR="$tmp_dir" \
KRATOS_CONFIG_DRY_RUN=1 \
sh "$(dirname "$0")/docker-entrypoint.sh" >/tmp/sync-identity-provider-config-test.log

config="$tmp_dir/kratos.yml"

assert_contains() {
    needle="$1"
    if ! grep -Fq "$needle" "$config"; then
        echo "FAIL: expected generated Kratos config to contain: $needle" >&2
        echo "--- generated config ---" >&2
        cat "$config" >&2
        exit 1
    fi
}

assert_not_contains() {
    needle="$1"
    if grep -Fq "$needle" "$config"; then
        echo "FAIL: expected generated Kratos config not to contain: $needle" >&2
        echo "--- generated config ---" >&2
        cat "$config" >&2
        exit 1
    fi
}

assert_contains "base_url: http://localhost:8192/auth/.ory/"
assert_contains "ui_url: http://localhost:8192/auth/recovery"
assert_contains "enabled: false"
assert_not_contains "courier:"
assert_not_contains "connection_uri:"

if grep -Fq '"email"' "$(dirname "$0")/identity.schema.json"; then
    echo "FAIL: identity schema should not contain an email trait" >&2
    exit 1
fi
if ! grep -Fq '"required": ["username"]' "$(dirname "$0")/identity.schema.json"; then
    echo "FAIL: identity schema should require only username" >&2
    exit 1
fi

echo "PASS: identity-provider username-only config is generated."
