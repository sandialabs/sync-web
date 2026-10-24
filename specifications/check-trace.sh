#!/bin/bash
# To run: ./check-trace.sh [output-dir], otherwise outputs to specifications/check-trace-output

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)" 
ROOT_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd)"
TESTS_DIR="$ROOT_DIR/records/tests"
SPECS_DIR="$ROOT_DIR/specifications"

if [ -z "${0:-}" ]; then
    echo "Usage: $0 [output-dir]"
    exit 1
fi

sdk="$ROOT_DIR/journal/target/release/journal-sdk" # journal-sdk location
tla2tools="$SCRIPT_DIR/tla2tools.jar"
out_dir="${3:-$SCRIPT_DIR/check-trace-output}"
mkdir -p "$out_dir"

echo "Step 1/3: Collecting traced logs"
echo "----------------------------------------------------------"
"$SPECS_DIR/run-traced-test.sh" "$sdk" "$out_dir"

echo ""
echo "Step 2/3: Converting traces to TLA+"
echo "----------------------------------------------------------"
python3 "$SPECS_DIR/gen_trace.py" "$out_dir/trace.log" "$out_dir"

echo ""
echo "Step 3/3: Checking traces with MultiJournalSyncWeb.tla with TLC"
echo "----------------------------------------------------------"
cp "$SPECS_DIR/MultiJournalSyncWebJournal.tla" "$out_dir/"
cp "$SPECS_DIR/LedgerTrace.tla" "$out_dir/"

cd "$out_dir"
java -XX:+UseParallelGC -cp "$tla2tools" tlc2.TLC -config LedgerTrace.cfg LedgerTrace.tla -workers auto \
    | tee tlc-output.txt

if grep -q "Error: Deadlock reached" tlc-output.txt; then
    echo ""
    echo "Deadlock reached. See $out_dir/trace.log and $out_dir/trace-symbols.json."
    exit 1
fi

if grep -qE "^Error:" tlc-output.txt; then
    echo ""
    echo "FAIL: TLC reported an error. See $out_dir/tlc-output.txt"
    exit 1
fi

echo ""
echo "Recorded traces properly follow MultiJournalSyncWeb.tla specifications!"