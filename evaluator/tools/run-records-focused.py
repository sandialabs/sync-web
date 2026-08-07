#!/usr/bin/env python3
"""Run the active unmodified Standard, Tree, and Document Records tests."""
import argparse
import subprocess
from pathlib import Path

CASES = (
    ("standard", "standard", '"Success (30 checks)"'),
    ("tree", "tree", '"Success (45 checks)"'),
    ("document", "document", "(passed 30 assertions)"),
)

def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")

def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", default="target/release/rust_journal_sdk")
    parser.add_argument("--records-root", default="/home/tdinh/projects/sync-web/records")
    parser.add_argument("--timeout", type=float, default=120.0)
    args = parser.parse_args()
    root = Path(args.records_root)
    assertions = read(root / "tests/support.scm")
    standard = read(root / "lisp/standard.scm")
    passed = 0
    for test_name, module_name, expected in CASES:
        test = read(root / f"tests/test-{test_name}.scm")
        sources = [assertions, standard]
        if module_name != "standard":
            sources.append(read(root / f"lisp/{module_name}.scm"))
        expression = f"({test} " + " ".join("'" + source for source in sources) + ")"
        run = subprocess.run(
            [args.candidate, "-e", "-"], input=expression, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=args.timeout,
        )
        output = run.stdout.strip()
        if run.returncode != 0 or output != expected:
            print(f"{test_name}: FAIL")
            print(output or run.stderr.strip())
            return 1
        print(f"{test_name}: {output}")
        passed += 1
    print(f"records-focused: {passed}/{len(CASES)}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
