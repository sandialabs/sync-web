from __future__ import annotations

import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

from journal_cli.flow import reviewer_workflow as reviewer
from journal_cli.flow import workflow_common as common


def git(repo: Path, *arguments: str) -> bytes:
    environment = {
        "PATH": "/usr/bin:/bin", "HOME": str(repo.parent / "author-home"),
        "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull, "LC_ALL": "C",
    }
    Path(environment["HOME"]).mkdir(exist_ok=True)
    return subprocess.run(
        ["/usr/bin/git", "-C", str(repo), *arguments], env=environment,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True,
    ).stdout


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def make_repo(root: Path, *, symlink: bool = False, attributes: bool = False):
    repo = root / "author"
    repo.mkdir()
    git(repo, "init", "-q")
    git(repo, "config", "user.name", "Fixture")
    git(repo, "config", "user.email", "fixture@example.invalid")
    (repo / "base.txt").write_text("base\n")
    git(repo, "add", "base.txt")
    git(repo, "commit", "-q", "-m", "base")
    base = git(repo, "rev-parse", "HEAD").decode().strip()
    (repo / "change.txt").write_text("one\n")
    if attributes:
        (repo / ".gitattributes").write_text("change.txt filter=owned\n")
    if symlink:
        os.symlink("base.txt", repo / "linked")
    git(repo, "add", "-A")
    git(repo, "commit", "-q", "-m", "first change")
    first = git(repo, "rev-parse", "HEAD").decode().strip()
    (repo / "change.txt").write_text("two\n")
    git(repo, "add", "change.txt")
    git(repo, "commit", "-q", "-m", "second change")
    second = git(repo, "rev-parse", "HEAD").decode().strip()
    return repo, base, first, second


def commit_row(repo: Path, oid: str) -> str:
    return git(repo, "show", "-s", "--format=%H %T %s", oid).decode().rstrip("\n")


def rewrite_manifest(package: Path) -> None:
    rows = []
    for path in sorted(
        (item for item in package.iterdir() if item.name != "SHA256SUMS"),
        key=lambda item: item.name.encode("ascii"),
    ):
        rows.append(f"{sha(path.read_bytes())}  {path.name}\n")
    (package / "SHA256SUMS").write_text("".join(rows))


def build_package(
    root: Path, repo: Path, base: str, head: str, *, kind: str = "full",
    prerequisite: str | None = None, extra_ref: bool = False,
) -> tuple[Path, dict]:
    package = root / f"package-{kind}-{head[:8]}"
    package.mkdir()
    bundle = package / "candidate.bundle"
    primary_ref = f"refs/fixture/{head}"
    git(repo, "update-ref", primary_ref, head)
    refs = [primary_ref]
    if extra_ref:
        secondary = f"refs/fixture/extra-{head}"
        git(repo, "update-ref", secondary, head)
        refs.append(secondary)
    prerequisite = base if prerequisite is None else prerequisite
    command = ["bundle", "create", str(bundle), *refs, f"^{prerequisite}"]
    git(repo, *command)

    changed = git(repo, "diff", "--name-only", base, head).decode().splitlines()
    changed_data = ("\n".join(sorted(changed, key=lambda item: item.encode())) + "\n").encode()
    commits = git(repo, "rev-list", "--reverse", f"{base}..{head}").decode().splitlines()
    commits_data = ("\n".join(commit_row(repo, oid) for oid in commits) + "\n").encode()
    evidence = b"fixture evidence\n"
    (package / "CHANGED-FILES.txt").write_bytes(changed_data)
    (package / "COMMITS.txt").write_bytes(commits_data)
    (package / "TESTS.md").write_bytes(evidence)
    tree = git(repo, "rev-parse", f"{head}^{{tree}}").decode().strip()
    review = {
        "evidence": [{"bytes": len(evidence), "path": "TESTS.md", "sha256": sha(evidence)}],
        "git": {
            "base": base,
            "bundle": "candidate.bundle",
            "bundleBytes": bundle.stat().st_size,
            "bundleSha256": sha(bundle.read_bytes()),
            "changedFiles": "CHANGED-FILES.txt",
            "changedFilesSha256": sha(changed_data),
            "commitCount": len(commits),
            "commits": "COMMITS.txt",
            "commitsSha256": sha(commits_data),
            "head": head,
            "kind": kind,
            "prerequisite": prerequisite,
            "tree": tree,
        },
        "package": {"schema": 1},
        "repository": {"id": "fixture-repo"},
        "schema": "sync-source-flow-review-v1",
    }
    (package / "REVIEW.json").write_bytes(common.canonical_json(review))
    rewrite_manifest(package)
    for ref in refs:
        git(repo, "update-ref", "-d", ref)
    return package, review


def source_value(package: Path) -> dict:
    return {
        "aggregateBytes": sum(path.stat().st_size for path in package.iterdir()),
        "chunks": 1,
        "entries": len(list(package.iterdir())),
        "fixedReference": {
            "descriptorBytes": 1,
            "descriptorPath": ["source", "fixture", "releases", "fixture", "release.scm"],
            "descriptorSha256": "2" * 64,
            "endpoint": "https://source.example/interface",
            "index": 1,
            "owner": "fixture",
        },
        "outcome": "materialized",
        "states": {"executed": False, "installed": False, "materialized": True},
        "treeSha256": "1" * 64,
    }


def handoff_value(package: Path, review: dict, reference: bytes) -> dict:
    source = source_value(package)
    artifact = lambda path: {"bytes": 1, "path": path, "sha256": "3" * 64}
    return {
        "fixedReference": {
            **source["fixedReference"],
            "artifact": {
                "bytes": len(reference), "path": "fixed-reference.scm",
                "sha256": sha(reference),
            },
        },
        "grantPlans": [],
        "operationId": "4" * 32,
        "outcome": "verified",
        "package": {
            "aggregateBytes": source["aggregateBytes"],
            "chunks": source["chunks"],
            "entries": source["entries"],
            "manifestSha256": sha((package / "SHA256SUMS").read_bytes()),
            "treeSha256": source["treeSha256"],
        },
        "phase": "ready",
        "readyResume": None,
        "receipts": {"publish": artifact("receipts/publish.json"), "ready": artifact("receipts/ready.json")},
        "review": {"sha256": sha(common.canonical_json(review)), "value": review},
        "schema": "journal-cli-source-flow-handoff-v2",
        "states": {"executed": False, "installed": False},
    }


def seed_cache(cache_root: Path, repo: Path, oid: str) -> None:
    cache_root.mkdir(mode=0o700, exist_ok=True)
    os.chmod(cache_root, 0o700)
    cache = cache_root / "fixture-repo.git"
    if not cache.exists():
        git(cache_root, "init", "-q", "--bare", str(cache))
        os.chmod(cache, 0o700)
    ref = f"refs/sync-source-flow/candidates/{oid}"
    existing = subprocess.run(
        ["/usr/bin/git", "--git-dir", str(cache), "show-ref", "--verify", "--quiet", ref],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    ).returncode
    if existing:
        git(cache_root, "--git-dir", str(cache), "fetch", "-q", "--no-tags", str(repo), f"{oid}:{ref}")


class ReviewerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="reviewer-common-test-")
        self.root = Path(self.temporary.name)
        os.chmod(self.root, 0o700)
        self.repo, self.base, self.first, self.second = make_repo(self.root)
        self.reference = self.root / "reference.scm"
        self.reference.write_text("(source-fixed-v2 fixture)\n")
        self.reference.chmod(0o600)
        self.cache = self.root / "cache"
        self.package: Path | None = None
        self.source_calls = 0
        self.original_pull = getattr(common, "source_pull_once", None)

        def pull_once(**values):
            self.source_calls += 1
            shutil.copytree(self.package, values["destination"])
            value = source_value(self.package)
            stdout = common.canonical_json(value)
            common.write_exclusive(values["receipt"], stdout)
            stdout_path = values["evidence_dir"] / "source-pull.stdout.json"
            stderr_path = values["evidence_dir"] / "source-pull.stderr"
            common.write_exclusive(stdout_path, stdout)
            common.write_exclusive(stderr_path, b"")
            return common.SourceResult(
                0, value, stdout_path, stderr_path, sha(stdout), sha(b""),
            )

        common.source_pull_once = pull_once

    def tearDown(self):
        if self.original_pull is None:
            delattr(common, "source_pull_once")
        else:
            common.source_pull_once = self.original_pull
        self.temporary.cleanup()

    def run_flow(self, package: Path, name: str = "operation", **values):
        self.package = package
        if values.get("seed", True):
            review = common.load_canonical_json(package / "REVIEW.json")
            seed_cache(self.cache, values.get("source_repo", self.repo), review["git"]["prerequisite"])
        return reviewer.run_reviewer(
            self.reference, ["galactica", "fixture"], self.root / name,
            self.cache, values.get("expected_handoff"),
        )

    def test_full_then_correction_uses_same_private_cache(self):
        full, _ = build_package(self.root, self.repo, self.base, self.first)
        full_receipt = self.run_flow(full, "full")
        correction, _ = build_package(
            self.root, self.repo, self.first, self.second,
            kind="correction", prerequisite=self.first,
        )
        correction_receipt = self.run_flow(correction, "correction")
        self.assertEqual(full_receipt["outcome"], "accepted")
        self.assertEqual(correction_receipt["git"]["prerequisite"], self.first)
        self.assertEqual(self.source_calls, 2)
        worktree = Path(correction_receipt["worktree"])
        self.assertEqual(git(worktree, "rev-parse", "HEAD").decode().strip(), self.second)
        self.assertEqual(git(worktree, "status", "--porcelain"), b"")

    def test_missing_prerequisite_stops_before_fetch(self):
        package, _ = build_package(
            self.root, self.repo, self.first, self.second,
            kind="correction", prerequisite=self.first,
        )
        with self.assertRaisesRegex(common.WorkflowError, "prerequisite"):
            self.run_flow(package, seed=False)
        receipt = common.load_canonical_json(self.root / "operation/review-receipt.json")
        self.assertEqual(receipt["error"]["code"], "missing-prerequisite")
        command_files = (self.root / "operation/commands").glob("bundle-import.json")
        self.assertEqual(list(command_files), [])

    def test_multiple_advertised_heads_reject_before_fetch(self):
        package, _ = build_package(
            self.root, self.repo, self.base, self.first, extra_ref=True,
        )
        with self.assertRaisesRegex(common.WorkflowError, "ambiguous"):
            self.run_flow(package)

    def test_symlink_and_gitlink_reject_before_worktree(self):
        for kind in ("symlink", "gitlink"):
            with self.subTest(kind=kind):
                isolated = self.root / kind
                isolated.mkdir()
                repo, base, first, _ = make_repo(isolated, symlink=kind == "symlink")
                if kind == "gitlink":
                    git(repo, "reset", "--hard", base)
                    git(repo, "update-index", "--add", "--cacheinfo", f"160000,{base},vendor")
                    git(repo, "commit", "-q", "-m", "gitlink")
                    first = git(repo, "rev-parse", "HEAD").decode().strip()
                package, _ = build_package(isolated, repo, base, first)
                self.package = package
                cache = self.root / f"{kind}-cache"
                seed_cache(cache, repo, base)
                with self.assertRaisesRegex(common.WorkflowError, "prohibited mode"):
                    reviewer.run_reviewer(
                        self.reference, ["galactica"], self.root / f"{kind}-op",
                        cache, None,
                    )
                self.assertFalse((self.root / f"{kind}-op/worktree").exists())

    def test_hostile_global_filter_is_not_executed(self):
        isolated = self.root / "filter"
        isolated.mkdir()
        repo, base, first, _ = make_repo(isolated, attributes=True)
        marker = self.root / "executed"
        hostile_home = self.root / "hostile-home"
        hostile_home.mkdir()
        (hostile_home / ".gitconfig").write_text(
            f"[filter \"owned\"]\n\tclean = touch {marker}\n"
            f"\tsmudge = sh -c 'touch {marker}; cat'\n\trequired = true\n"
        )
        package, _ = build_package(isolated, repo, base, first)
        receipt = self.run_flow(package, source_repo=repo)
        self.assertEqual(receipt["outcome"], "accepted")
        self.assertFalse(marker.exists())

    def test_package_extra_hardlink_and_wrong_tree_reject(self):
        package, review = build_package(self.root, self.repo, self.base, self.first)
        (package / "EXTRA").write_text("extra")
        rewrite_manifest(package)
        with self.assertRaises(common.WorkflowError):
            self.run_flow(package, "extra")

        package.unlink if False else None
        package2, review2 = build_package(self.root, self.repo, self.base, self.second)
        review2["git"]["tree"] = "0" * 40
        (package2 / "REVIEW.json").write_bytes(common.canonical_json(review2))
        rewrite_manifest(package2)
        with self.assertRaisesRegex(common.WorkflowError, "tree"):
            self.run_flow(package2, "tree")

    def test_correction_count_and_promisor_cache_reject(self):
        correction, review = build_package(
            self.root, self.repo, self.first, self.second,
            kind="correction", prerequisite=self.first,
        )
        review["git"]["commitCount"] = 2
        (correction / "REVIEW.json").write_bytes(common.canonical_json(review))
        rewrite_manifest(correction)
        with self.assertRaisesRegex(common.WorkflowError, "exactly one"):
            self.run_flow(correction, "count")

        full, _ = build_package(self.root, self.repo, self.base, self.first)
        seed_cache(self.cache, self.repo, self.base)
        marker = self.cache / "fixture-repo.git/objects/pack/hostile.promisor"
        marker.write_bytes(b"")
        with self.assertRaisesRegex(common.WorkflowError, "promisor"):
            self.run_flow(full, "promisor")

    def test_expected_handoff_binding_and_main_output(self):
        package, review = build_package(self.root, self.repo, self.base, self.first)
        handoff = self.root / "handoff.json"
        handoff.write_bytes(common.canonical_json(
            handoff_value(package, review, self.reference.read_bytes())
        ))
        handoff.chmod(0o600)
        receipt = self.run_flow(package, expected_handoff=handoff)
        self.assertEqual(receipt["outcome"], "accepted")

    def test_main_emits_one_bounded_verdict_line(self):
        package, _ = build_package(self.root, self.repo, self.base, self.first)
        self.package = package
        seed_cache(self.cache, self.repo, self.base)
        class Output:
            def __init__(self):
                self.buffer = io.BytesIO()
        output = Output()
        with mock.patch.object(reviewer.sys, "stdout", output):
            status = reviewer.main([
                "--reference", str(self.reference),
                "--route", '["galactica","fixture"]',
                "--workspace", str(self.root / "main-operation"),
                "--cache", str(self.cache),
            ])
        self.assertEqual(status, 0)
        lines = output.buffer.getvalue().splitlines()
        self.assertEqual(len(lines), 1)
        self.assertLessEqual(len(lines[0]), 4096)
        self.assertEqual(json.loads(lines[0])["outcome"], "verified")

    def test_existing_workspace_stops_before_source_dispatch(self):
        package, _ = build_package(self.root, self.repo, self.base, self.first)
        self.package = package
        workspace = self.root / "exists"
        workspace.mkdir()
        with self.assertRaises(common.WorkflowError):
            reviewer.run_reviewer(
                self.reference, ["galactica"], workspace, self.cache, None,
            )
        self.assertEqual(self.source_calls, 0)

    def test_relative_overlap_and_unsafe_route_stop_before_dispatch(self):
        with self.assertRaisesRegex(common.WorkflowError, "absolute"):
            reviewer.run_reviewer(Path("reference"), ["galactica"], self.root / "relative", self.cache, None)
        with self.assertRaisesRegex(common.WorkflowError, "overlap"):
            reviewer.run_reviewer(self.reference, ["galactica"], self.root / "overlap", self.root / "overlap/cache", None)
        with self.assertRaisesRegex(common.WorkflowError, "route"):
            reviewer._route('["../peer"]')
        self.assertEqual(self.source_calls, 0)

    def test_unexpected_failure_writes_sanitized_terminal_receipt(self):
        package, _ = build_package(self.root, self.repo, self.base, self.first)
        self.package = package
        workspace = self.root / "unexpected"
        with mock.patch.object(common, "validate_review_package", side_effect=UnicodeError("host detail")):
            with self.assertRaisesRegex(common.WorkflowError, "package-verification"):
                reviewer.run_reviewer(self.reference, ["galactica"], workspace, self.cache, None)
        receipt = common.load_canonical_json(workspace / "review-receipt.json")
        self.assertEqual(receipt["outcome"], "failed")
        self.assertEqual(receipt["error"], {"code": "internal-failure", "message": "UnicodeError"})

    def test_interruption_records_phase_truth(self):
        package, _ = build_package(self.root, self.repo, self.base, self.first)
        self.package = package
        def interrupted(**_values):
            self.source_calls += 1
            raise KeyboardInterrupt()
        common.source_pull_once = interrupted
        with self.assertRaises(KeyboardInterrupt):
            reviewer.run_reviewer(
                self.reference, ["galactica"], self.root / "interrupt",
                self.cache, None,
            )
        receipt = common.load_canonical_json(self.root / "interrupt/review-receipt.json")
        self.assertEqual(receipt["outcome"], "interrupted")
        self.assertEqual(receipt["phase"], "source-pull")
        self.assertEqual(self.source_calls, 1)


if __name__ == "__main__":
    unittest.main()
