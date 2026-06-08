#!/bin/sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
CC="${CC:-cc}"
OUT_DIR="$ROOT/target/c-oracle"
OUT="$OUT_DIR/s7-oracle"
STAMP="$OUT_DIR/s7-oracle.sha256"
mkdir -p "$OUT_DIR"

CFLAGS="-std=c99 -O3 -DDEFAULT_PRINT_LENGTH=9223372036854775807 -DWITH_PURE_S7=1 -DWITH_SYSTEM_EXTRAS=0 -DWITH_C_LOADER=0 -I$ROOT/vendor/s7"
INPUTS="$ROOT/tools/s7_oracle.c $ROOT/vendor/s7/s7.c $ROOT/vendor/s7/s7.h $ROOT/tools/build-s7-oracle.sh"

fingerprint() {
  {
    printf 'cc=%s\n' "$CC"
    printf 'cflags=%s\n' "$CFLAGS"
    for path in $INPUTS; do
      sha256sum "$path"
    done
  } | sha256sum | awk '{print $1}'
}

NEW_HASH="$(fingerprint)"
OLD_HASH=""
if [ -f "$STAMP" ]; then
  OLD_HASH="$(cat "$STAMP")"
fi

if [ "${FORCE_REBUILD:-0}" != "1" ] && [ -x "$OUT" ] && [ "$NEW_HASH" = "$OLD_HASH" ]; then
  printf '%s\n' "$OUT"
  exit 0
fi

# Behavior-relevant flags mirror sync-web's journal/build.rs.
# shellcheck disable=SC2086
"$CC" \
  $CFLAGS \
  "$ROOT/tools/s7_oracle.c" \
  "$ROOT/vendor/s7/s7.c" \
  -lm \
  -o "$OUT"

printf '%s\n' "$NEW_HASH" > "$STAMP"
printf '%s\n' "$OUT"
