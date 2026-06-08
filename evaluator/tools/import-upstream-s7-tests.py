#!/usr/bin/env python3
"""Import isolated upstream s7 `(test ...)` assertions into upstream-corpus.

The upstream `s7test.scm` file is enormous and contains timing tests, file/system
behavior, setup-dependent checks, and features outside sync-web's intended
subset. This importer keeps a conservative category: individual `(test EXPR
EXPECTED)` forms that pass under the C oracle when run in isolation.

Generated cases are snapshots for differential review; the C oracle remains the
source of truth.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SOURCE = Path.home() / "projects" / "miscellaneous" / "s7" / "s7test.scm"
DEFAULT_ORACLE = ROOT / "target" / "c-oracle" / "s7-oracle"
DEFAULT_OUTPUT = ROOT / "upstream-corpus"


@dataclass(frozen=True)
class ExtractedTest:
    index: int
    line: int
    source: str
    expr: str
    expected: str


def line_for_offset(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def matching_paren(text: str, start: int) -> int | None:
    depth = 0
    in_string = False
    in_bar_symbol = False
    escaped = False
    i = start
    while i < len(text):
        c = text[i]
        n = text[i + 1] if i + 1 < len(text) else ""
        if in_string:
            if escaped:
                escaped = False
            elif c == "\\":
                escaped = True
            elif c == '"':
                in_string = False
        elif in_bar_symbol:
            if escaped:
                escaped = False
            elif c == "\\":
                escaped = True
            elif c == "|":
                in_bar_symbol = False
        else:
            if c == ";":
                while i < len(text) and text[i] != "\n":
                    i += 1
                continue
            if c == "#" and n == "|":
                i += 2
                while i + 1 < len(text) and not (text[i] == "|" and text[i + 1] == "#"):
                    i += 1
                i += 1
            elif c == '"':
                in_string = True
            elif c == "|":
                in_bar_symbol = True
            elif c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    return i + 1
        i += 1
    return None


def skip_ws(s: str, i: int) -> int:
    while i < len(s):
        if s[i].isspace():
            i += 1
            continue
        if s[i] == ";":
            while i < len(s) and s[i] != "\n":
                i += 1
            continue
        break
    return i


def read_one(s: str, i: int) -> tuple[str, int] | None:
    i = skip_ws(s, i)
    if i >= len(s):
        return None
    if s[i] in "'`":
        tail = read_one(s, i + 1)
        if not tail:
            return None
        expr, end = tail
        return s[i:end].strip(), end
    if s[i] == ",":
        start = i
        i += 2 if i + 1 < len(s) and s[i + 1] == "@" else 1
        tail = read_one(s, i)
        if not tail:
            return None
        _, end = tail
        return s[start:end].strip(), end
    if s[i] == "(":
        end = matching_paren(s, i)
        if end is None:
            return None
        return s[i:end].strip(), end
    if s[i] == '"':
        start = i
        i += 1
        escaped = False
        while i < len(s):
            c = s[i]
            if escaped:
                escaped = False
            elif c == "\\":
                escaped = True
            elif c == '"':
                return s[start : i + 1], i + 1
            i += 1
        return None
    start = i
    while i < len(s) and not s[i].isspace() and s[i] != ")":
        i += 1
    return s[start:i], i


def split_test_form(form: str) -> tuple[str, str] | None:
    inner = form.strip()[1:-1]
    if not inner.startswith("test"):
        return None
    i = skip_ws(inner, 4)
    first = read_one(inner, i)
    if not first:
        return None
    expr, i = first
    second = read_one(inner, i)
    if not second:
        return None
    expected, i = second
    if skip_ws(inner, i) != len(inner):
        return None
    return expr, expected


def extract_tests(text: str) -> list[ExtractedTest]:
    tests: list[ExtractedTest] = []
    for match in re.finditer(r"\(test\s", text):
        start = match.start()
        end = matching_paren(text, start)
        if end is None:
            continue
        form = text[start:end]
        split = split_test_form(form)
        if not split:
            continue
        expr, expected = split
        tests.append(ExtractedTest(len(tests) + 1, line_for_offset(text, start), form, expr, expected))
    return tests


UNSUPPORTED_SUBSET_TOKENS = [
    # sync-web intentionally removes continuations and dynamic-wind.
    "call/cc",
    "call-with-current-continuation",
    "call-with-exit",
    "dynamic-wind",
    # Do not import tests that pressure filesystem/system/loading/C embedding APIs.
    "load",
    "autoload",
    "open-input-file",
    "open-output-file",
    "call-with-input-file",
    "call-with-output-file",
    "with-input-from-file",
    "with-output-to-file",
    "delete-file",
    "file-exists?",
    "file-mtime",
    "directory->list",
    "system",
    "getenv",
    "c-pointer",
    "c-object",
    "c-function",
    "c-macro",
    "c-define",
    # Upstream embedder/profiling/debug machinery and explicitly blacklisted helpers.
    "make-hook",
    "hook-functions",
    "profile-in",
    "gc",
    "random-state",
    "random",
]


def subset_supported(test: ExtractedTest) -> bool:
    form = test.source
    return not any(token in form for token in UNSUPPORTED_SUBSET_TOKENS)


def generated_program(test: ExtractedTest) -> str:
    return f""";; Imported from upstream s7test.scm line {test.line}.
;; Original form:
;; {test.source.replace(chr(10), chr(10) + ';; ')}

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () {test.expr})))
       (expected (upstream-safe (lambda () {test.expected})))
       (ok? (equal? actual expected)))
  (list 'upstream-test {test.line} actual expected ok?))
"""


def run_oracle(oracle: Path, test_file: Path, timeout: float) -> str | None:
    with tempfile.TemporaryDirectory(prefix="s7-upstream-oracle-") as work:
        work_dir = Path(work)
        try:
            result = subprocess.run(
                [str(oracle), str(test_file)],
                cwd=work_dir,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=timeout,
                check=False,
            )
        except subprocess.TimeoutExpired:
            return None
        side_effects = [path for path in work_dir.rglob("*")]
        if side_effects:
            return None
    if result.returncode != 0 or result.stderr:
        return None
    stdout = result.stdout if result.stdout.endswith("\n") else result.stdout + "\n"
    if not stdout.strip().endswith("#t)"):
        return None
    # Many upstream assertions depend on earlier s7test.scm state. When isolated,
    # some only pass because both sides throw an unbound-variable error. Those
    # are importer artifacts rather than useful subset behavior pressure.
    if "(error (unbound-variable" in stdout:
        return None
    return stdout


def safe_case_name(seq: int, line: int) -> str:
    return f"s7test-{seq:04d}-line-{line}"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--oracle", type=Path, default=DEFAULT_ORACLE)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--max-cases", type=int, default=800)
    parser.add_argument("--timeout", type=float, default=2.0)
    parser.add_argument("--window-lines", type=int, default=5000, help="source-line bucket size for coverage spreading")
    parser.add_argument("--max-per-window", type=int, default=100, help="maximum kept cases per source-line bucket; 0 disables cap")
    parser.add_argument("--replace", action="store_true", help="replace output directory before writing")
    args = parser.parse_args()

    if not args.source.is_file():
        print(f"upstream source not found: {args.source}")
        return 2
    if not args.oracle.is_file():
        print(f"oracle not found: {args.oracle}; run tools/build-s7-oracle.sh first")
        return 2
    if args.max_cases <= 0:
        print("--max-cases must be positive")
        return 2

    text = args.source.read_text(errors="replace")
    extracted = extract_tests(text)
    if args.replace and args.output.exists():
        shutil.rmtree(args.output)
    args.output.mkdir(parents=True, exist_ok=True)

    kept = 0
    tried = 0
    kept_by_window: dict[int, int] = {}
    with tempfile.TemporaryDirectory(prefix="s7-upstream-import-") as temp:
        temp_dir = Path(temp)
        for test in extracted:
            if kept >= args.max_cases:
                break
            if not subset_supported(test):
                continue
            window = (test.line // args.window_lines) * args.window_lines if args.window_lines > 0 else 0
            if args.max_per_window > 0 and kept_by_window.get(window, 0) >= args.max_per_window:
                continue
            tried += 1
            program = generated_program(test)
            temp_file = temp_dir / "test.scm"
            temp_file.write_text(program)
            expected = run_oracle(args.oracle.resolve(), temp_file, args.timeout)
            if expected is None:
                continue

            kept += 1
            kept_by_window[window] = kept_by_window.get(window, 0) + 1
            case_dir = args.output / safe_case_name(kept, test.line)
            case_dir.mkdir(parents=True, exist_ok=True)
            (case_dir / "test.scm").write_text(program)
            (case_dir / "expected.scm").write_text(expected)
            (case_dir / "meta.json").write_text(json.dumps({
                "status": "oracle-current",
                "category": "upstream-s7test-isolated",
                "source": str(args.source),
                "source_line": test.line,
                "source_form": test.source,
                "features": ["upstream-s7test", "isolated-assertion"],
                "notes": "Imported only if the C oracle passes the isolated assertion.",
            }, indent=2) + "\n")

    print(f"extracted: {len(extracted)}")
    print(f"tried: {tried}")
    print(f"kept: {kept}")
    print("kept-by-window:")
    for window, count in sorted(kept_by_window.items()):
        print(f"  {window}-{window + args.window_lines - 1}: {count}")
    print(f"output: {args.output}")
    return 0 if kept else 1


if __name__ == "__main__":
    raise SystemExit(main())
