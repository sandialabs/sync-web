import os
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
UPLOADER = ROOT / "scripts/cadaver_upload.sh"

class CadaverUploadTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir=ROOT / "tests")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.incoming = self.root / "incoming"
        self.uploaded = self.root / "uploaded"
        self.failed = self.root / "failed"
        self.calls = self.root / "cadaver_calls"
        bin_dir = self.root / "bin"
        bin_dir.mkdir()
        cadaver = bin_dir / "cadaver"
        cadaver.write_text(
            "#!/usr/bin/env python3\n"
            "import os, pathlib, sys, time\n"
            "sys.stdin.read()\n"
            "with pathlib.Path(os.environ['CADAVER_CALLS']).open('a') as calls:\n"
            "    calls.write('called\\n')\n"
            "time.sleep(float(os.environ.get('CADAVER_SLEEP', '0')))\n"
            "sys.exit(int(os.environ.get('CADAVER_EXIT', '0')))\n"
        )
        cadaver.chmod(0o755)
        self.environment = dict(os.environ)
        self.environment.update({
            "PATH": f"{bin_dir}:{os.environ['PATH']}",
            "WEBDAV_URL": "https://example.test/webdav/",
            "WATCH_DIR": str(self.incoming),
            "DONE_DIR": str(self.uploaded),
            "FAILED_DIR": str(self.failed),
            "LOCK_DIR": str(self.root / "locks"),
            "LOG_FILE": str(self.root / "uploader.log"),
            "CADAVER_CALLS": str(self.calls),
            "STABLE_CHECKS": "1",
            "STABLE_SLEEP_SECONDS": "0",
            "STABLE_ATTEMPTS": "3",
        })

    def prepare_artifacts(self):
        agent_dir = self.incoming / "cad_agent"
        agent_dir.mkdir(parents=True)
        (agent_dir / "model_20261018_120001.step").write_text("STEP")
        (agent_dir / "cad_agent_20261018_120001.jsonl").write_text("LOGS")
        (agent_dir / ".upload-ready").touch()
        return agent_dir

    def run_uploader(self, path, **environment):
        return subprocess.run(
            [str(UPLOADER), str(path)],
            env={**self.environment, **environment},
            text=True,
            capture_output=True,
            check=False,
            timeout=10,
        )

    def test_successful_directory_has_one_upload_and_uploaded_state(self):
        agent_dir = self.prepare_artifacts()
        result = self.run_uploader(agent_dir)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.calls.read_text().splitlines(), ["called"])
        self.assertTrue((self.uploaded / "cad_agent").is_dir())
        self.assertFalse((self.failed / "cad_agent").exists())

    def test_competing_worker_exits_without_duplicate_upload(self):
        agent_dir = self.prepare_artifacts()
        first = subprocess.Popen(
            [str(UPLOADER), str(agent_dir)],
            env={**self.environment, "CADAVER_SLEEP": "0.5"},
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            for _ in range(100):
                if self.calls.exists():
                    break
                time.sleep(0.02)
            self.assertTrue(self.calls.exists(), "first worker did not reach upload")
            second = self.run_uploader(agent_dir)
            self.assertEqual(second.returncode, 0, second.stdout + second.stderr)
            self.assertIn("already claimed", second.stdout)
            stdout, stderr = first.communicate(timeout=10)
            self.assertEqual(first.returncode, 0, stdout + stderr)
        finally:
            if first.poll() is None:
                first.kill()
                first.communicate()
        self.assertEqual(self.calls.read_text().splitlines(), ["called"])
        self.assertTrue((self.uploaded / "cad_agent").is_dir())
        self.assertFalse((self.failed / "cad_agent").exists())

    def test_stability_timeout_moves_directory_to_failed_without_upload(self):
        agent_dir = self.prepare_artifacts()
        result = self.run_uploader(agent_dir, STABLE_CHECKS="3", STABLE_ATTEMPTS="1")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("Timed out waiting for stability", result.stdout)
        self.assertTrue((self.failed / "cad_agent").is_dir())
        self.assertFalse((self.uploaded / "cad_agent").exists())
        self.assertFalse(self.calls.exists())

    def test_rejects_nonregular_entries(self):
        for kind in ("symlink", "nested_directory", "fifo", "marker_symlink"):
            with self.subTest(kind=kind):
                agent_dir = self.prepare_artifacts()
                if kind == "symlink":
                    (agent_dir / "extra.step").symlink_to(agent_dir / "model_20261018_120001.step")
                elif kind == "nested_directory":
                    (agent_dir / "nested").mkdir()
                elif kind == "fifo":
                    os.mkfifo(agent_dir / "pipe")
                else:
                    (agent_dir / ".upload-ready").unlink()
                    (agent_dir / ".upload-ready").symlink_to(agent_dir / "model_20261018_120001.step")
                result = self.run_uploader(agent_dir)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertFalse(agent_dir.exists())
                self.assertFalse(self.calls.exists())

    def test_failed_upload_has_only_failed_state(self):
        agent_dir = self.prepare_artifacts()
        result = self.run_uploader(agent_dir, CADAVER_EXIT="1")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(self.calls.read_text().splitlines(), ["called"])
        self.assertTrue((self.failed / "cad_agent").is_dir())
        self.assertFalse((self.uploaded / "cad_agent").exists())

if __name__ == "__main__":
    unittest.main()
