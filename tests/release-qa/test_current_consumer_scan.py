from __future__ import annotations

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest


MODULE_PATH = Path(__file__).with_name("current_consumer_scan.py")
SPEC = importlib.util.spec_from_file_location("current_consumer_scan", MODULE_PATH)
assert SPEC and SPEC.loader
scan = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(scan)


class CurrentConsumerScanTests(unittest.TestCase):
    def test_active_repository_has_no_removed_resource_consumers(self) -> None:
        self.assertEqual(scan.findings(), [])

    def test_patterns_cover_routes_calls_grants_and_current_docs(self) -> None:
        removed_get = "g" + "et"
        removed_set = "s" + "et"
        removed_call = "c" + "all!"
        examples = (
            f'client.post("/api/v1/general/{removed_get}", json=body)',
            f'call_general("{removed_set}-batch", "{removed_set}-batch!", body)',
            f"((*journal* journal-1 '{removed_get}) path)",
            f'"{removed_call}": true',
            f'`{removed_get}-batch` is the current operation',
        )
        for example in examples:
            with self.subTest(example=example):
                self.assertTrue(any(pattern.search(example) for pattern in scan.PATTERNS))

    def test_scan_covers_canonical_scheme_envelopes_in_tracked_temp_root(self) -> None:
        removed_get = "g" + "et"
        removed_resolve_batch = "resolve" + "-batch"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            (root / "read.scm").write_text(
                f"((function {removed_get}) (arguments ((path (*state* value)))))\n",
                encoding="utf-8",
            )
            (root / "history.scm").write_text(
                f"((function {removed_resolve_batch})\n (arguments ((paths ((*state* value))))))\n",
                encoding="utf-8",
            )
            (root / "structural.scm").write_text(
                f"(define-method ({removed_get} self path) (deep-get self path))\n",
                encoding="utf-8",
            )
            subprocess.run(
                ["git", "-C", str(root), "add", "read.scm", "history.scm", "structural.scm"],
                check=True,
            )
            self.assertEqual(
                [finding.split(":", 1)[0] for finding in scan.findings(root)],
                ["history.scm", "read.scm"],
            )

    def test_scan_covers_rust_tex_and_extensionless_utf8_files(self) -> None:
        removed_get = "g" + "et"
        removed_set = "s" + "et"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "consumer.rs").write_text(
                f'client.call_general("{removed_get}", "{removed_get}", args);\n',
                encoding="utf-8",
            )
            (root / "guide.tex").write_text(f"({removed_get} #t)\n", encoding="utf-8")
            (root / "sync-client").write_text(
                f'curl "/api/v1/general/{removed_set}"\n',
                encoding="utf-8",
            )
            self.assertEqual(
                [finding.split(":", 1)[0] for finding in scan.findings(root)],
                ["consumer.rs", "guide.tex", "sync-client"],
            )

    def test_exclusions_are_limited_to_declared_noncurrent_protocol_surfaces(self) -> None:
        self.assertIn("docs/migrations/1.6-resource-api.md", scan.HISTORICAL_FILES)
        self.assertIn(
            "tools/journal-cli/tests/head-helper-qualification-v0.1.4/helper/head_expected_old_cas.py",
            scan.HISTORICAL_FILES,
        )
        self.assertEqual(scan.HISTORICAL_PREFIXES, {"records/tests/fixtures/"})
        self.assertIn("records/lisp/standard.scm", scan.STRUCTURAL_PROTOCOL_FILES)
        self.assertIn("records/lisp/tree.scm", scan.STRUCTURAL_PROTOCOL_FILES)


if __name__ == "__main__":
    unittest.main()
