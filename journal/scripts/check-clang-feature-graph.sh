#!/bin/sh
set -eu

mode=${1:-}
case "$mode" in
  musl|non-musl) ;;
  *) echo "usage: $0 musl|non-musl" >&2; exit 2 ;;
esac

host=$(rustc -vV | awk '/^host:/ { print $2 }')
case "$mode:$host" in
  musl:*-musl) ;;
  musl:*) echo "musl feature check requires a native musl Rust host, got $host" >&2; exit 1 ;;
  non-musl:*-musl) echo "non-musl feature check cannot run on a musl Rust host" >&2; exit 1 ;;
  non-musl:*) ;;
esac

tree=$(mktemp)
trap 'rm -f "$tree"' EXIT
cargo tree --locked --features wasmer-evaluator -e features \
  -i clang-sys@1.9.1 > "$tree"

case "$mode" in
  musl)
    if grep -Fq 'clang-sys feature "runtime"' "$tree"; then
      echo "musl clang-sys graph unexpectedly enables runtime linkage" >&2
      cat "$tree" >&2
      exit 1
    fi
    grep -Fq 'clang-sys feature "static"' "$tree" || {
      echo "musl clang-sys graph does not enable static linkage" >&2
      cat "$tree" >&2
      exit 1
    }
    ;;
  non-musl)
    if grep -Fq 'clang-sys feature "static"' "$tree"; then
      echo "non-musl clang-sys graph unexpectedly enables static linkage" >&2
      cat "$tree" >&2
      exit 1
    fi
    grep -Fq 'clang-sys feature "runtime"' "$tree" || {
      echo "non-musl clang-sys graph does not enable runtime linkage" >&2
      cat "$tree" >&2
      exit 1
    }
    ;;
esac

cat "$tree"
