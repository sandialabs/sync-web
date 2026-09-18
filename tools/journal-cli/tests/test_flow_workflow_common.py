from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from journal_cli.flow.workflow_common import (
    SOURCE_LAUNCHER_ENV, SOURCE_LAUNCHER_SHA_ENV, WorkflowError, allowed_ancestor_owner,
    canonical_json, isolated_git_environment, make_exclusive_dir, parse_canonical_json,
    parse_sha256sums, run_bounded, source_pull_once, validate_expected_handoff, validate_review,
    validate_review_package,
)


class CommonTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        os.chmod(self.root, 0o700)

    def tearDown(self):
        self.temp.cleanup()

    def review(self):
        return {
            "schema": "sync-source-flow-review-v1",
            "package": {"schema": 1},
            "repository": {"id": "sync-web"},
            "git": {
                "base": "1" * 40,
                "bundle": "candidate.bundle",
                "bundleBytes": 1,
                "bundleSha256": hashlib.sha256(b"b").hexdigest(),
                "changedFiles": "CHANGED-FILES.txt",
                "changedFilesSha256": hashlib.sha256(b"a\n").hexdigest(),
                "commitCount": 1,
                "commits": "COMMITS.txt",
                "commitsSha256": hashlib.sha256(b"c\n").hexdigest(),
                "head": "2" * 40,
                "kind": "correction",
                "prerequisite": "1" * 40,
                "tree": "3" * 40,
            },
            "evidence": [],
        }

    def test_canonical_json_rejects_duplicates_and_formatting(self):
        value = {"b": 2, "a": 1}
        self.assertEqual(parse_canonical_json(canonical_json(value)), value)
        for bad in (b'{"a":1,"a":2}\n', b'{"a": 1}\n', b'{"a":NaN}\n', b'{"a":1}'):
            with self.subTest(bad=bad), self.assertRaises(WorkflowError):
                parse_canonical_json(bad)

    def test_manifest_rejects_unsorted_duplicate_parent_and_self(self):
        good = b"0" * 64 + b"  A\n" + b"1" * 64 + b"  b\n"
        self.assertEqual(list(parse_sha256sums(good)), ["A", "b"])
        cases = (
            b"1" * 64 + b"  b\n" + b"0" * 64 + b"  A\n",
            b"0" * 64 + b"  a\n" + b"1" * 64 + b"  a\n",
            b"0" * 64 + b"  ../a\n",
            b"0" * 64 + b"  SHA256SUMS\n",
        )
        for bad in cases:
            with self.subTest(bad=bad), self.assertRaises(WorkflowError):
                parse_sha256sums(bad)

    def test_review_kind_and_prerequisite_binding(self):
        review = self.review()
        self.assertEqual(validate_review(review)["git"]["kind"], "correction")
        review["git"]["kind"] = "full"
        self.assertEqual(validate_review(review)["git"]["prerequisite"], "1" * 40)
        review["git"]["base"] = "4" * 40
        with self.assertRaises(WorkflowError):
            validate_review(review)

    def test_complete_flat_package(self):
        package = self.root / "package"
        package.mkdir(mode=0o700)
        files = {
            "candidate.bundle": b"b",
            "CHANGED-FILES.txt": b"a\n",
            "COMMITS.txt": b"c\n",
        }
        review = self.review()
        files["REVIEW.json"] = canonical_json(review)
        for name, data in files.items():
            (package / name).write_bytes(data)
        rows = b"".join(hashlib.sha256(data).hexdigest().encode() + b"  " + name.encode() + b"\n" for name, data in sorted(files.items()))
        (package / "SHA256SUMS").write_bytes(rows)
        observed, hashes = validate_review_package(package)
        self.assertEqual(observed, review)
        self.assertIn("candidate.bundle", hashes)

    def test_package_rejects_symlink_and_unsafe_mode(self):
        package = self.root / "package"
        package.mkdir(mode=0o700)
        target = package / "target"
        target.write_bytes(b"x")
        (package / "link").symlink_to("target")
        (package / "SHA256SUMS").write_bytes(b"")
        with self.assertRaises(WorkflowError):
            validate_review_package(package)
        (package / "link").unlink()
        os.chmod(package, 0o777)
        with self.assertRaises(WorkflowError):
            validate_review_package(package)

    def test_expected_handoff_is_exact_and_binds_embedded_review(self):
        review = self.review()
        handoff = {
            "fixedReference": {
                "artifact": {"bytes": 1, "path": "fixed-reference.scm", "sha256": "a" * 64},
                "descriptorBytes": 1,
                "descriptorPath": ["source", "project", "releases", "release", "release.scm"],
                "descriptorSha256": "b" * 64,
                "endpoint": "https://source.example/interface",
                "index": 1,
                "owner": "owner",
            },
            "grantPlans": [{"bytes": 1, "path": "grant-plans/reviewer.json", "sha256": "c" * 64}],
            "operationId": "d" * 32,
            "outcome": "verified",
            "package": {"aggregateBytes": 1, "chunks": 1, "entries": 5, "manifestSha256": "e" * 64, "treeSha256": "f" * 64},
            "phase": "ready",
            "readyResume": None,
            "receipts": {
                "publish": {"bytes": 1, "path": "receipts/publish.json", "sha256": "1" * 64},
                "ready": {"bytes": 1, "path": "receipts/ready.json", "sha256": "2" * 64},
            },
            "review": {"sha256": hashlib.sha256(canonical_json(review)).hexdigest(), "value": review},
            "schema": "journal-cli-source-flow-handoff-v2",
            "states": {"executed": False, "installed": False},
        }
        self.assertEqual(validate_expected_handoff(handoff)["outcome"], "verified")
        handoff["review"]["sha256"] = "0" * 64
        with self.assertRaises(WorkflowError):
            validate_expected_handoff(handoff)

    def test_source_pull_uses_verified_environment_binding_once(self):
        operation = make_exclusive_dir(self.root / "source-operation")
        launcher = operation / "launcher"
        launcher.write_text("#!/bin/sh\nprintf '%s\\n' '{\"outcome\":\"materialized\"}'\n")
        launcher.chmod(0o700)
        reference = operation / "reference.scm"
        reference.write_text("(source-fixed-v2)\n")
        environment = {
            SOURCE_LAUNCHER_ENV: str(launcher),
            SOURCE_LAUNCHER_SHA_ENV: hashlib.sha256(launcher.read_bytes()).hexdigest(),
        }
        with mock.patch.dict(os.environ, environment):
            result = source_pull_once(
                reference=reference,
                route=["galactica", "publisher"],
                destination=operation / "destination",
                receipt=operation / "receipt.json",
                evidence_dir=operation,
            )
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.value["outcome"], "materialized")
        self.assertTrue(result.stdout_path.exists())
        self.assertTrue(result.stderr_path.exists())

    def test_git_environment_removes_inherited_git_configuration(self):
        operation = make_exclusive_dir(self.root / "git-operation")
        environment, _options = isolated_git_environment(operation)
        with mock.patch.dict(os.environ, {"GIT_DIR": "/hostile", "GIT_CONFIG_COUNT": "1"}):
            result = run_bounded(
                ["python3", "-c", "import os;print(os.environ.get('GIT_DIR'));print(os.environ.get('GIT_CONFIG_COUNT'))"],
                environment=environment,
            )
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, b"None\nNone\n")

    def test_root_and_effective_user_ancestors_only(self):
        self.assertTrue(allowed_ancestor_owner(0, 1009))
        self.assertTrue(allowed_ancestor_owner(1009, 1009))
        self.assertFalse(allowed_ancestor_owner(1008, 1009))

    def test_exclusive_dir_and_git_isolation(self):
        operation = make_exclusive_dir(self.root / "operation")
        env, options = isolated_git_environment(operation)
        self.assertEqual(env["GIT_CONFIG_NOSYSTEM"], "1")
        self.assertEqual(env["GIT_TERMINAL_PROMPT"], "0")
        self.assertIn("submodule.recurse=false", options)
        with self.assertRaises(WorkflowError):
            make_exclusive_dir(operation)
        hostile = make_exclusive_dir(self.root / "hostile")
        (hostile / "target").mkdir(mode=0o700)
        (hostile / "git-home").symlink_to("target", target_is_directory=True)
        with self.assertRaises(WorkflowError):
            isolated_git_environment(hostile)


if __name__ == "__main__":
    unittest.main()
