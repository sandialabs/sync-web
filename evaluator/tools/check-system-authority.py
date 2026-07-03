#!/usr/bin/env python3
"""Check that s7-rust does not expose direct host/system authority.

This is a candidate-only guardrail, separate from the C oracle.  The intent is
not to reject all s7 compatibility expansion, only names that would grant or
model filesystem, process, native-loading, direct network, nondeterministic, or
broad debug/profiling authority.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CANDIDATE = ROOT / "target" / "release" / "s7-rust"

DENIED_NAMES: dict[str, list[str]] = {
    "filesystem": [
        "open-input-file",
        "open-output-file",
        "call-with-input-file",
        "call-with-output-file",
        "with-input-from-file",
        "with-output-to-file",
        "load",
        "port-file",
        "port-filename",
    ],
    "process-system": [
        "system",
        "getenv",
        "exit",
        "abort",
        "emergency-exit",
    ],
    "dynamic-native-loading": [
        "autoload",
        "require",
        "*autoload*",
        "*autoload-hook*",
        "*load-hook*",
        "*load-path*",
        "*cload-directory*",
        "c-pointer",
        "c-pointer?",
        "c-pointer-info",
        "c-pointer-type",
        "c-pointer-weak1",
        "c-pointer-weak2",
        "c-pointer->list",
        "c-object?",
        "c-object-type",
    ],
    "nondeterminism": [
        "random",
        "random-state",
        "random-state?",
        "random-state->list",
    ],
    "debug-hooks-profiling": [
        "make-hook",
        "hook-functions",
        "*rootlet-redefinition-hook*",
        "*read-error-hook*",
        "*error-hook*",
        "*missing-close-paren-hook*",
        "*unbound-variable-hook*",
        "profile-in",
        "stacktrace",
        "gc",
    ],
}

DENIED = {name for names in DENIED_NAMES.values() for name in names}


def registered_names() -> set[str]:
    names: set[str] = set()
    for rel in ["src/builtins.rs", "src/lib.rs"]:
        text = (ROOT / rel).read_text()
        # builtin!("name", ...)
        names.update(re.findall(r'builtin!\("([^"]+)"', text))
        # Static rootlet metadata arrays contain quoted names, one per line.
        for match in re.finditer(r'"([^"]+)"\s*,', text):
            names.add(match.group(1))
    return names


def scheme_variable_ref(name: str) -> str:
    # All denied names are readable as variable references with vertical bars if necessary.
    escaped = name.replace("\\", "\\\\").replace("|", "\\|")
    return f"|{escaped}|"


def run_candidate(candidate: Path, expr: str, timeout: float) -> tuple[int, str, str]:
    with tempfile.NamedTemporaryFile("w", suffix=".scm", delete=False) as f:
        f.write(expr)
        f.write("\n")
        path = Path(f.name)
    try:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [str(candidate), str(path)],
                cwd=tmp,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=timeout,
                check=False,
            )
        return (
            result.returncode,
            result.stdout.decode("utf-8", "replace").strip(),
            result.stderr.decode("utf-8", "replace").strip(),
        )
    finally:
        path.unlink(missing_ok=True)


def runtime_exposed(candidate: Path, name: str, timeout: float) -> tuple[bool, str]:
    # Evaluate the variable only; do not call potentially dangerous procedures.
    expr = f"(catch #t (lambda () {scheme_variable_ref(name)}) (lambda args 'authority-name-unbound))"
    code, out, err = run_candidate(candidate, expr, timeout)
    if code != 0:
        return True, f"candidate exited {code}: {err or out}"
    if out == "authority-name-unbound" or out.startswith("(error (unbound-variable"):
        return False, out
    return True, out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", type=Path, default=DEFAULT_CANDIDATE)
    parser.add_argument("--source-only", action="store_true", help="only scan registered names; skip runtime probes")
    parser.add_argument("--timeout", type=float, default=5.0)
    parser.add_argument("--failures", type=int, default=50)
    args = parser.parse_args()

    failures: list[str] = []

    registered = registered_names()
    source_hits = sorted(DENIED & registered)
    if source_hits:
        failures.append("source denied names registered: " + ", ".join(source_hits))

    runtime_hits: list[str] = []
    if not args.source_only:
        candidate = args.candidate.resolve()
        if not candidate.is_file():
            raise SystemExit(f"candidate not found: {candidate}")
        for category, names in DENIED_NAMES.items():
            for name in names:
                exposed, detail = runtime_exposed(candidate, name, args.timeout)
                if exposed:
                    runtime_hits.append(f"{category}:{name} => {detail}")
                    if len(runtime_hits) >= args.failures:
                        break
            if len(runtime_hits) >= args.failures:
                break
        if runtime_hits:
            failures.append("runtime denied names exposed:\n  " + "\n  ".join(runtime_hits))

    if failures:
        print("system-authority-check: FAIL")
        for failure in failures:
            print(failure)
        return 1

    print("system-authority-check: ok")
    print(f"denied-name-count: {len(DENIED)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
