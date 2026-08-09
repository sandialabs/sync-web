#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
platform_version=$(cat "$repo_root/VERSION")
journal_sdk=${JOURNAL_SDK:-$repo_root/journal/target/debug/journal-sdk}

if [ ! -x "$journal_sdk" ]; then
    echo "Build journal-sdk or set JOURNAL_SDK to an exact native binary" >&2
    exit 1
fi
journal_sdk=$(CDPATH= cd -- "$(dirname "$journal_sdk")" && pwd)/$(basename "$journal_sdk")

check_runner() {
    runner=$1
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT INT TERM
    mkdir "$work/lisp"
    cp "$repo_root/$runner" "$work/run.sh"
    for file in root standard log-chain tree ledger federation authorization interface; do
        cp "$repo_root/records/lisp/$file.scm" "$work/lisp/"
    done
    cp "$work/lisp/interface.scm" "$work/canonical-interface.scm"
    cat > "$work/journal-sdk" <<EOF
#!/bin/sh
if [ "\${1:-}" = "-e" ]; then
    printf 'evaluate\n' >> calls.log
    if [ -n "\${FORCE_EVAL_RESULT+x}" ]; then
        cat >/dev/null
        printf '%s\n' "\$FORCE_EVAL_RESULT"
        exit "\${FORCE_EVAL_STATUS:-0}"
    fi
    exec "$journal_sdk" "\$@"
fi
printf 'server\n' >> calls.log
EOF
    chmod +x "$work/journal-sdk"

    run() {
        (cd "$work" && SYNC_WEB_VERSION="$platform_version" \
            SECRET=root-test-secret INTERFACE_SECRET=interface-test-secret \
            LISP_DIR="$work/lisp" WINDOW=8 PERIOD=2 RUST_LOG=error \
            JOURNAL_UPDATE=${UPDATE:-0} sh ./run.sh)
    }

    # Fresh exact success publishes the marker and launches. J0 reopens without
    # another evaluator call. J1's real zero-status Scheme error cannot launch.
    run
    test "$(cat "$work/database/.sync-web-version")" = "$platform_version"
    test "$(grep -c '^evaluate$' "$work/calls.log")" = 1
    test "$(grep -c '^server$' "$work/calls.log")" = 1
    run
    test "$(grep -c '^evaluate$' "$work/calls.log")" = 1
    test "$(grep -c '^server$' "$work/calls.log")" = 2
    if UPDATE=1 run >"$work/update.out" 2>"$work/update.err"; then
        echo "FAIL: $runner accepted fresh-only J1 update" >&2
        exit 1
    fi
    test "$(grep -c '^server$' "$work/calls.log")" = 2
    grep -Fq 'Journal record installation failed; result omitted' "$work/update.err"
    ! grep -Fq 'root-test-secret' "$work/update.err"
    ! grep -Fq 'interface-test-secret' "$work/update.err"

    # A real fresh Scheme error returned with process status zero cannot write
    # the marker or launch, and its result body stays omitted.
    rm -rf "$work/database"
    : > "$work/calls.log"
    printf '%s\n' "(macro args (error 'upgrade-error \"installer-payload\"))" \
        > "$work/lisp/interface.scm"
    if run >"$work/error.out" 2>"$work/error.err"; then
        echo "FAIL: $runner accepted fresh Scheme error" >&2
        exit 1
    fi
    test ! -f "$work/database/.sync-web-version"
    ! grep -q '^server$' "$work/calls.log"
    ! grep -Fq 'installer-payload' "$work/error.err"
    ! grep -Fq 'root-test-secret' "$work/error.err"
    cp "$work/canonical-interface.scm" "$work/lisp/interface.scm"

    # Malformed and process-level evaluator results fail at the same boundary.
    rm -rf "$work/database"
    : > "$work/calls.log"
    if FORCE_EVAL_RESULT='(malformed result)' run >"$work/malformed.out" 2>"$work/malformed.err"; then
        echo "FAIL: $runner accepted malformed installer result" >&2
        exit 1
    fi
    test ! -f "$work/database/.sync-web-version"
    ! grep -q '^server$' "$work/calls.log"
    if FORCE_EVAL_RESULT='"Installed interface"' FORCE_EVAL_STATUS=9 run \
        >"$work/status.out" 2>"$work/status.err"; then
        echo "FAIL: $runner accepted evaluator process failure" >&2
        exit 1
    fi
    test ! -f "$work/database/.sync-web-version"
    ! grep -q '^server$' "$work/calls.log"

    rm -rf "$work"
    trap - EXIT INT TERM
    echo "PASS: $runner fresh installer boundary and ordinary J0 reopen"
}

check_runner deploy/compose/general/run.sh
check_runner deploy/compose/ledger/run.sh
