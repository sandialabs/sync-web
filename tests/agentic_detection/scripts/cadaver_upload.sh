#!/usr/bin/env bash
set -u

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
project_root=$(cd -- "$script_dir/.." && pwd)
source "$script_dir/webdav_config.sh"

WATCH_DIR="${WATCH_DIR:-$project_root/incoming}"
DONE_DIR="${DONE_DIR:-$project_root/uploaded}"
FAILED_DIR="${FAILED_DIR:-$project_root/failed}"
LOG_FILE="${LOG_FILE:-$project_root/logs/cadaver_upload.log}"
LOCK_DIR="${LOCK_DIR:-$project_root/logs/upload_locks}"
WEBDAV_URL=$(resolve_webdav_url) || exit 1

# Directory upload readiness marker.
READY_MARKER=".upload-ready"

# Require directories to contain READY_MARKER before upload.
# Legacy mode can relax this for non-CAD directories. CAD artifact validation
# always requires a regular readiness marker.
REQUIRE_READY_MARKER_FOR_DIRS="${REQUIRE_READY_MARKER_FOR_DIRS:-1}"

# Number of consecutive identical signatures required before upload.
STABLE_CHECKS="${STABLE_CHECKS:-3}"

# Seconds between stability checks.
STABLE_SLEEP_SECONDS="${STABLE_SLEEP_SECONDS:-1}"
STABLE_ATTEMPTS="${STABLE_ATTEMPTS:-120}"

# Periodic rescan interval, in seconds.
# This catches directories that become ready after the initial create event.
RESCAN_SECONDS="${RESCAN_SECONDS:-5}"

mkdir -p "$WATCH_DIR" "$DONE_DIR" "$FAILED_DIR" "$LOCK_DIR" "$(dirname "$LOG_FILE")"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" | tee -a "$LOG_FILE"
}

cadaver_quote() {
    local s="$1"

    s="${s//\\/\\\\}"
    s="${s//\"/\\\"}"

    printf '"%s"' "$s"
}

safe_move_to_dir() {
    local src="$1"
    local dest_dir="$2"
    local base
    local dest
    local stamp
    local n

    base="$(basename "$src")"
    dest="$dest_dir/$base"

    if [ -e "$dest" ] || [ -L "$dest" ]; then
        stamp="$(date '+%Y%m%d-%H%M%S')"
        dest="$dest_dir/${base}.${stamp}"

        n=1
        while [ -e "$dest" ] || [ -L "$dest" ]; do
            dest="$dest_dir/${base}.${stamp}.${n}"
            n=$((n + 1))
        done
    fi

    mv "$src" "$dest"
}

path_signature() {
    local path="$1"

    python3 - "$path" <<'PY'
import os
import stat
import sys


def describe(path):
    metadata = os.lstat(path)
    print(f"{path}|{metadata.st_mode}|{metadata.st_size}|{metadata.st_mtime_ns}")
    if stat.S_ISDIR(metadata.st_mode):
        for entry in sorted(os.scandir(path), key=lambda item: item.name):
            describe(entry.path)


describe(sys.argv[1])
PY
}

wait_until_stable() {
    local path="$1"
    local last=""
    local current=""
    local stable_count=0

    for _ in $(seq 1 "$STABLE_ATTEMPTS"); do
        [ -e "$path" ] || return 1

        current="$(path_signature "$path")" || return 1

        if [ "$current" = "$last" ]; then
            stable_count=$((stable_count + 1))

            if [ "$stable_count" -ge "$STABLE_CHECKS" ]; then
                return 0
            fi
        else
            stable_count=0
        fi

        last="$current"
        sleep "$STABLE_SLEEP_SECONDS"
    done

    log "Timed out waiting for stability: $path"
    return 1
}

should_skip() {
    local path="$1"
    local base

    base="$(basename "$path")"

    case "$base" in
        .*|*.swp|*.tmp|*.part|*.crdownload)
            return 0
            ;;
    esac

    return 1
}

directory_is_ready() {
    local dir="$1"

    if [ "$REQUIRE_READY_MARKER_FOR_DIRS" -eq 0 ]; then
        return 0
    fi

    [ -f "$dir/$READY_MARKER" ] && [ ! -L "$dir/$READY_MARKER" ]
}

validate_artifact_contract() {
    local dir="$1"
    local base
    local item
    local relative
    local step_count=0
    local jsonl_count=0
    local marker_count=0

    base="$(basename "$dir")"
    case "$base" in
        cad_agent|adversarial_cad_agent)
            ;;
        *)
            return 0
            ;;
    esac

    while IFS= read -r -d '' item; do
        if [ -L "$item" ] || [ ! -f "$item" ]; then
            log "Artifact contract violation: non-regular directory entry: $item"
            return 1
        fi
        relative="${item#$dir/}"
        case "$relative" in
            "$READY_MARKER")
                marker_count=$((marker_count + 1))
                ;;
            *.step|*.stp)
                step_count=$((step_count + 1))
                ;;
            *.jsonl)
                jsonl_count=$((jsonl_count + 1))
                ;;
            *)
                log "Artifact contract violation: unexpected file: $item"
                return 1
                ;;
        esac
    done < <(find "$dir" -mindepth 1 -maxdepth 1 -print0)

    if [ "$step_count" -ne 1 ] || [ "$jsonl_count" -ne 1 ] || [ "$marker_count" -ne 1 ]; then
        log "Artifact contract violation: $base requires one STEP/STP, one JSONL, and one regular $READY_MARKER; found STEP/STP=$step_count JSONL=$jsonl_count marker=$marker_count"
        return 1
    fi

    return 0
}

generate_cadaver_commands_for_file() {
    local file="$1"
    local remote_name="$2"

    printf 'put %s %s\n' \
        "$(cadaver_quote "$file")" \
        "$(cadaver_quote "$remote_name")"
}

generate_cadaver_commands_for_dir() {
    local dir="$1"
    local base
    local item
    local rel
    local remote_path

    base="$(basename "$dir")"

    # Create the top-level remote directory.
    # If the directory already exists, some WebDAV servers may report an error,
    # but cadaver usually continues to process subsequent commands.
    printf 'mkcol %s\n' "$(cadaver_quote "$base")"

    # Create subdirectories, shallowest first.
    while IFS= read -r -d '' item; do
        rel="${item#$dir/}"
        remote_path="$base/$rel"

        printf 'mkcol %s\n' "$(cadaver_quote "$remote_path")"
    done < <(find "$dir" -mindepth 1 -type d -print0 | sort -z)

    # Upload files, excluding the readiness marker.
    while IFS= read -r -d '' item; do
        rel="${item#$dir/}"

        if [ "$rel" = "$READY_MARKER" ]; then
            continue
        fi

        remote_path="$base/$rel"

        printf 'put %s %s\n' \
            "$(cadaver_quote "$item")" \
            "$(cadaver_quote "$remote_path")"
    done < <(find "$dir" -type f -print0 | sort -z)
}

upload_claimed_path() {
    local path="$1"
    local base
    local cmdfile
    local status

    [ -e "$path" ] || [ -L "$path" ] || return 0

    base="$(basename "$path")"

    if should_skip "$path"; then
        log "Skipping temporary/hidden item: $path"
        return 0
    fi

    if [ -L "$path" ] || { [ ! -f "$path" ] && [ ! -d "$path" ]; }; then
        log "Unsupported upload entry; moving to failed: $path"
        safe_move_to_dir "$path" "$FAILED_DIR" || log "Could not move failed upload entry: $path"
        return 1
    fi

    if [ -d "$path" ] && { [ -L "$path/$READY_MARKER" ] || { [ -e "$path/$READY_MARKER" ] && [ ! -f "$path/$READY_MARKER" ]; }; }; then
        log "Invalid readiness marker; moving to failed: $path"
        safe_move_to_dir "$path" "$FAILED_DIR" || log "Could not move failed upload entry: $path"
        return 1
    fi

    if [ -d "$path" ] && ! directory_is_ready "$path"; then
        log "Directory is not ready yet; missing $READY_MARKER: $path"
        return 0
    fi

    log "Waiting for item to become stable: $path"

    if ! wait_until_stable "$path"; then
        log "Stability check failed: $path"
        if [ -e "$path" ] || [ -L "$path" ]; then
            safe_move_to_dir "$path" "$FAILED_DIR" || log "Could not move failed upload entry: $path"
        fi
        return 1
    fi

    if [ -d "$path" ] && ! validate_artifact_contract "$path"; then
        log "Moving invalid artifact directory to failed: $path"
        safe_move_to_dir "$path" "$FAILED_DIR" || log "Could not move failed upload entry: $path"
        return 1
    fi

    cmdfile="$(mktemp)"

    if [ -f "$path" ]; then
        log "Preparing file upload: $path"
        generate_cadaver_commands_for_file "$path" "$base" > "$cmdfile"
    elif [ -d "$path" ]; then
        log "Preparing recursive directory upload: $path"
        generate_cadaver_commands_for_dir "$path" > "$cmdfile"
    else
        log "Skipping non-file/non-directory: $path"
        rm -f "$cmdfile"
        return 0
    fi

    printf 'quit\n' >> "$cmdfile"

    log "Uploading with cadaver: $path"

    cadaver "$WEBDAV_URL" < "$cmdfile" >> "$LOG_FILE" 2>&1
    status=$?

    rm -f "$cmdfile"

    if [ "$status" -eq 0 ]; then
        log "Upload succeeded: $path"
        if safe_move_to_dir "$path" "$DONE_DIR"; then
            return 0
        fi
        log "Upload succeeded remotely, but could not move local entry to uploaded: $path"
        return 1
    else
        log "Upload failed: $path"
        safe_move_to_dir "$path" "$FAILED_DIR" || log "Could not move failed upload entry: $path"
        return 1
    fi
}

upload_path() {
    local path="$1"
    local status

    [ -e "$path" ] || [ -L "$path" ] || return 0
    python3 "$script_dir/claim_upload.py" "$LOCK_DIR" "$script_dir/cadaver_upload.sh" "$path"
    status=$?
    if [ "$status" -eq 75 ]; then
        log "Upload already claimed by another worker: $path"
        return 0
    fi
    return "$status"
}

scan_watch_dir() {
    local path

    for path in "$WATCH_DIR"/*; do
        [ -e "$path" ] || continue
        upload_path "$path"
    done
}

rescan_loop() {
    while true; do
        sleep "$RESCAN_SECONDS"
        scan_watch_dir
    done
}

cleanup() {
    local code=$?

    if [ -n "${RESCAN_PID:-}" ]; then
        kill "$RESCAN_PID" >/dev/null 2>&1 || true
    fi

    exit "$code"
}

run_watch_mode() {
    local path

    log "Watching directory: $WATCH_DIR"
    log "Uploading to: $WEBDAV_URL"
    log "Directory ready marker: $READY_MARKER"
    log "Require ready marker for directories: $REQUIRE_READY_MARKER_FOR_DIRS"
    log "Stability checks: $STABLE_CHECKS"
    log "Stability sleep seconds: $STABLE_SLEEP_SECONDS"
    log "Stability attempts: $STABLE_ATTEMPTS"
    log "Rescan seconds: $RESCAN_SECONDS"

    if ! command -v inotifywait >/dev/null 2>&1; then
        log "Error: inotifywait is not installed. Install with: sudo apt-get install inotify-tools"
        exit 1
    fi

    if ! command -v cadaver >/dev/null 2>&1; then
        log "Error: cadaver is not installed."
        exit 1
    fi

    # Upload anything already present at startup.
    scan_watch_dir

    # Periodically rescan in case an event was missed or a directory became ready
    # after the top-level directory create event.
    rescan_loop &
    RESCAN_PID=$!

    trap cleanup EXIT INT TERM

    inotifywait -m \
        -e close_write,moved_to,create \
        --format '%w%f' \
        "$WATCH_DIR" |
    while IFS= read -r path; do
        upload_path "$path"
    done
}

main() {
    local path

    if [ "${1:-}" = "--claimed" ]; then
        if [ "$#" -ne 2 ]; then
            echo "Usage: cadaver_upload.sh --claimed <path>" >&2
            return 2
        fi
        upload_claimed_path "$2"
        return $?
    fi

    if [ $# -gt 0 ]; then
        if ! command -v cadaver >/dev/null 2>&1; then
            log "Error: cadaver is not installed."
            exit 1
        fi

        for path in "$@"; do
            upload_path "$path"
        done
    else
        run_watch_mode
    fi
}

main "$@"
