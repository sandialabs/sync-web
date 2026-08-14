#!/usr/bin/env python3

# reads test-ledger.scm and rewrites ledger expresses so each event is wrapped
# python3 inject_tracer.py <test-scm-file> [output-path] 

import re
import sys

LEDGER_CREATE_RE = re.compile(
    r"\(define\s+([A-Za-z0-9\-]+)\s+"
    r"\(sync-eval\s+\(make-ledger\s+'([A-Za-z0-9\-]+)\)\s+#f\)\)"
)


def inject(source: str):
    def replace(m: re.Match) -> str:
        define_name, ctor_name = m.group(1), m.group(2)
        if define_name != ctor_name:
            raise ValueError(
                f"target ledger name does not match make-ledger name"
            )
        return (
            f"(define {define_name} "
            f"(trace-wrap (sync-eval (make-ledger '{ctor_name}) #f) '{ctor_name} log!))"
        )

    patched, count = LEDGER_CREATE_RE.subn(replace, source)
    return patched, count


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: inject_tracer.py <test-scm-file> [output-path]", file=sys.stderr)
        sys.exit(1)

    with open(sys.argv[1], "r") as f:
        source = f.read()

    patched, count = inject(source)

    if count == 0:
        print(
            "No ledger creation events matched. Test files should create ledgers the way test-ledger.scm does.",
            file=sys.stderr,
        )
    else:
        print(f"Injected trace-wrap around {count} ledger creation site(s).", file=sys.stderr)

    if len(sys.argv) >= 3:
        with open(sys.argv[2], "w") as f:
            f.write(patched)
    else:
        sys.stdout.write(patched)


if __name__ == "__main__":
    main()