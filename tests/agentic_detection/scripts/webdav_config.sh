#!/usr/bin/env bash

# Resolve the WebDAV endpoint from environment configuration. WEBDAV_URL is an
# optional escape hatch for deployments whose scheme, port, or path differs.
resolve_webdav_url() {
    if [[ -n "${WEBDAV_URL:-}" ]]; then
        case "$WEBDAV_URL" in
            http://*|https://*)
                printf '%s\n' "${WEBDAV_URL%/}/"
                return 0
                ;;
            *)
                echo "WEBDAV_URL must begin with http:// or https://." >&2
                return 1
                ;;
        esac
    fi

    if [[ -z "${WEBDAV_HOST:-}" ]]; then
        echo "WebDAV configuration is missing. Set WEBDAV_HOST to the server IP or hostname, or set WEBDAV_URL to the full endpoint." >&2
        return 1
    fi

    case "$WEBDAV_HOST" in
        *://*|*/*|*[[:space:]]*)
            echo "WEBDAV_HOST must contain only an IP address or hostname; use WEBDAV_URL for a full URL." >&2
            return 1
            ;;
    esac

    local webdav_port="${WEBDAV_PORT:-8192}"
    local webdav_path="${WEBDAV_PATH:-webdav/stage/admin}"
    case "$webdav_port" in
        ''|*[!0-9]*)
            echo "WEBDAV_PORT must be a number between 1 and 65535." >&2
            return 1
            ;;
    esac
    if (( webdav_port < 1 || webdav_port > 65535 )); then
        echo "WEBDAV_PORT must be a number between 1 and 65535." >&2
        return 1
    fi
    webdav_path="${webdav_path#/}"
    webdav_path="${webdav_path%/}"
    if [[ -z "$webdav_path" ]]; then
        echo "WEBDAV_PATH must not be empty." >&2
        return 1
    fi

    printf 'http://%s:%s/%s/\n' \
        "$WEBDAV_HOST" \
        "$webdav_port" \
        "$webdav_path"
}
