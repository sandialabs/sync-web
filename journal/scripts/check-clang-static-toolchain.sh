#!/bin/sh
set -eu

real_llvm_config=/usr/lib/llvm21/bin/llvm-config
: "${LLVM_CONFIG_PATH:=/src/scripts/llvm-config-musl-static}"
test "$LLVM_CONFIG_PATH" = /src/scripts/llvm-config-musl-static
test -x "$LLVM_CONFIG_PATH"
test -x "$real_llvm_config"
test "$("$LLVM_CONFIG_PATH" --version)" = 21.1.2
test "$("$LLVM_CONFIG_PATH" --prefix)" = /usr/lib/llvm21
libdir=$("$LLVM_CONFIG_PATH" --libdir)
test "$libdir" = /usr/lib/llvm21/lib
test "$("$LLVM_CONFIG_PATH" --shared-mode)" = shared
test -f "$libdir/libclang.a"

clang_archives=$(find "$libdir" -maxdepth 1 -type f -name 'libclang*.a' -print)
test -n "$clang_archives"
llvm_archives=$("$LLVM_CONFIG_PATH" --link-static --libfiles)
test -n "$llvm_archives"
for archive in $llvm_archives; do
  test -f "$archive"
done

component_flags=$("$real_llvm_config" --libs --link-static)
system_flags=$("$real_llvm_config" --system-libs --link-static)
transitive_flags='-llzma -lpthread'
llvm_flags=$("$LLVM_CONFIG_PATH" --libs --link-static)
expected_flags=$(printf '%s\n%s\n%s\n' "$component_flags" "$system_flags" "$transitive_flags")
test "$llvm_flags" = "$expected_flags" || {
  echo "llvm-config wrapper did not append the static system closure after LLVM" >&2
  exit 1
}
clang_system_flags='-lffi -lncursesw -lstdc++ -lz'
for flag in $system_flags $transitive_flags $clang_system_flags; do
  case "$flag" in
    -l*) name=${flag#-l} ;;
    *) continue ;;
  esac
  archive=$(cc -print-file-name="lib$name.a")
  test "$archive" != "lib$name.a" && test -f "$archive" || {
    echo "static linker cannot resolve -l$name to an archive" >&2
    exit 1
  }
done

clang_flags=
for archive in $(printf '%s\n' "$clang_archives" | sort); do
  name=${archive##*/lib}
  clang_flags="$clang_flags -l${name%.a}"
done
probe_dir=$(mktemp -d)
trap 'rm -rf "$probe_dir"' EXIT
cat >"$probe_dir/probe.c" <<'EOF'
typedef void *CXIndex;
extern CXIndex clang_createIndex(int, int);
extern void clang_disposeIndex(CXIndex);
int main(void) {
  CXIndex index = clang_createIndex(0, 0);
  if (!index) return 1;
  clang_disposeIndex(index);
  return 0;
}
EOF
cc -static-pie -o "$probe_dir/probe" "$probe_dir/probe.c" -L"$libdir" \
  -Wl,--start-group \
  $clang_flags $llvm_flags $clang_system_flags \
  -Wl,--end-group
"$probe_dir/probe"
if readelf -d "$probe_dir/probe" | grep -q '(NEEDED)'; then
  echo "static PIE probe has a dynamic dependency" >&2
  exit 1
fi

printf 'llvm-config=%s\n' "$LLVM_CONFIG_PATH"
printf 'llvm-prefix=%s\n' "$("$LLVM_CONFIG_PATH" --prefix)"
printf 'llvm-libdir=%s\n' "$libdir"
printf 'llvm-shared-mode=%s\n' "$("$LLVM_CONFIG_PATH" --shared-mode)"
printf 'clang-static-archives=%s\n' "$(printf '%s\n' "$clang_archives" | wc -l)"
printf 'llvm-static-archives=%s\n' "$(printf '%s\n' $llvm_archives | wc -l)"
printf 'llvm-system-libs=%s\n' "$system_flags"
printf 'static-pie-link-probe=pass\n'
printf 'static-pie-dt-needed=0\n'
