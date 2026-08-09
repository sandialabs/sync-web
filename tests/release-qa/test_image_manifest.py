#!/usr/bin/env python3

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


MODULE_PATH = Path(__file__).with_name("image_manifest.py")
SPEC = importlib.util.spec_from_file_location("image_manifest", MODULE_PATH)
assert SPEC and SPEC.loader
manifest = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = manifest
SPEC.loader.exec_module(manifest)


class ImageManifestTests(unittest.TestCase):
    def args(self, path: Path) -> argparse.Namespace:
        return argparse.Namespace(manifest=str(path), source_root=".", runtime="podman")

    def test_verify_accepts_exact_source_and_image_identity(self) -> None:
        expected_image = {"reference": "example:test", "id": "sha256:one", "digest": "sha256:two", "repo_digests": []}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "manifest.json"
            path.write_text(json.dumps({"schema": 1, "source": {"commit": "c", "tree": "t"}, "images": [expected_image]}))
            with patch.object(manifest, "source", return_value={"commit": "c", "tree": "t"}), patch.object(
                manifest, "image", return_value=expected_image
            ):
                manifest.verify_manifest(self.args(path))

    def test_verify_rejects_source_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "manifest.json"
            path.write_text(json.dumps({"schema": 1, "source": {"commit": "c", "tree": "t"}, "images": []}))
            with patch.object(manifest, "source", return_value={"commit": "changed", "tree": "t"}):
                with self.assertRaisesRegex(RuntimeError, "image manifest source mismatch"):
                    manifest.verify_manifest(self.args(path))

    def test_verify_rejects_mutated_tag_identity(self) -> None:
        expected_image = {"reference": "example:test", "id": "sha256:one", "digest": "", "repo_digests": []}
        actual_image = {**expected_image, "id": "sha256:changed"}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "manifest.json"
            path.write_text(json.dumps({"schema": 1, "source": {"commit": "c", "tree": "t"}, "images": [expected_image]}))
            with patch.object(manifest, "source", return_value={"commit": "c", "tree": "t"}), patch.object(
                manifest, "image", return_value=actual_image
            ):
                with self.assertRaisesRegex(RuntimeError, "image manifest mismatch"):
                    manifest.verify_manifest(self.args(path))


if __name__ == "__main__":
    unittest.main()
