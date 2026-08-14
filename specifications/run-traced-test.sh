#!/bin/bash
# Goes through test-ledger.scm with tracer.scm, extracts operation traces to logs
# 
# Single usage: ./run-traced-test.sh 
#
# Outputs raw-output.txt (printed journal-sdk) and trace.log (operation events)

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd)"
TESTS_DIR="$ROOT_DIR/records/tests"
LISP_DIR="$ROOT_DIR/records/lisp"

if [ -z "${1:-}" ]; then
    echo "Usage: $0 <path-to-journal-sdk-or-docker-invocation> [output-dir]"
    exit 1
fi

sdk="$ROOT_DIR/journal/target/release/journal-sdk" # journal-sdk location
out_dir="${2:-$SCRIPT_DIR/trace-output}"
mkdir -p "$out_dir"

tracer=$(cat "$SCRIPT_DIR/tracer.scm")
assertions=$(cat "$ROOT_DIR/records/tests/support.scm")
standard=$(cat "$LISP_DIR/standard.scm")
log_chain=$(cat "$LISP_DIR/log-chain.scm")
tree=$(cat "$LISP_DIR/tree.scm")
ledger=$(cat "$LISP_DIR/ledger.scm")
document=$(cat "$LISP_DIR/document.scm")

patched_test=$(python3 "$SCRIPT_DIR/inject_tracer.py" "$TESTS_DIR/test-ledger.scm")

expr="(begin
  ${tracer}
  (let* ((log (list))
         (log! (lambda (event) (set! log (cons event log))))
         (result ($patched_test '$assertions '$standard '$log_chain '$tree '$ledger '$document)))
    (display result)
    (newline)
    (display \"------------------TLA_TRACE_BEGIN------------------\")
    (newline)
    (display (trace-serialize (reverse log)))
    (display \"------------------TLA_TRACE_END------------------\")
    (newline)))"

echo "Running trace script on test-ledger"

set +e
printf '%s' "$expr" | $sdk -e - > "$out_dir/raw-output.txt" 2>&1
status=$?
set -e

if [ "$status" -ne 0 ]; then
    echo "SDK invocation failed (exit $status). See $out_dir/raw-output.txt"
    exit "$status"
fi

first_line=$(sed -n '/./{p;q;}' "$out_dir/raw-output.txt")
if [[ "$first_line" == "(error "* ]]; then
    echo "FAIL: test returned an error form. See $out_dir/raw-output.txt"
    exit 1
fi

# copy everything between to trace.log
awk '/------------------TLA_TRACE_BEGIN------------------/{flag=1;next}/------------------TLA_TRACE_END------------------/{flag=0}flag' \
    "$out_dir/raw-output.txt" > "$out_dir/trace.log"

if [ ! -s "$out_dir/trace.log" ]; then
    echo "trace.log is empty. See $out_dir/raw-output.txt."
    exit 1
fi

event_count=$(grep -c '^[[:space:]]*(event\>' "$out_dir/trace.log")
echo "Success, $event_count traced events written to $out_dir/trace.log."
