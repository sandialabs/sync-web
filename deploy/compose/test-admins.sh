#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
platform_version=$(cat "$repo_root/VERSION")

check_runner() {
    runner="$1"
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT INT TERM
    mkdir -p "$work/lisp"
    for file in root standard log-chain tree ledger federation authorization interface; do
        printf '#f\n' > "$work/lisp/$file.scm"
    done
    cp "$repo_root/$runner" "$work/run.sh"
    cat > "$work/journal-sdk" <<'MOCK'
#!/bin/sh
if [ "${1:-}" = "-e" ]; then
    input=$(cat)
    case "$input" in
        \(\*call\**) printf '#t\n' ;;
        *)
            printf '%s' "$input" > install-expression.scm
            mkdir -p database
            : > database/CURRENT
            printf '"Installed interface"\n'
            ;;
    esac
fi
exit 0
MOCK
    chmod +x "$work/journal-sdk"

    (cd "$work" && SECRET=test INTERFACE_SECRET=interface-test WINDOW=8 PERIOD=2 RUST_LOG=error \
        LISP_DIR="$work/lisp" INTERFACE_ADMINS=admin,alice \
        SYNC_WEB_VERSION="$platform_version" sh ./run.sh)

    expected='(admins ((*state* admin) (*state* alice)))'
    if ! grep -Fq "$expected" "$work/install-expression.scm"; then
        echo "FAIL: $runner did not render literal admin principals" >&2
        cat "$work/install-expression.scm" >&2
        exit 1
    fi
    if grep -Fq '(admins (list ' "$work/install-expression.scm"; then
        echo "FAIL: $runner persisted an unevaluated admin expression" >&2
        exit 1
    fi

    rm -rf "$work"
    trap - EXIT INT TERM
}

check_runner deploy/compose/general/run.sh
check_runner deploy/compose/ledger/run.sh

echo "PASS: compose runners render literal interface admin principals"
