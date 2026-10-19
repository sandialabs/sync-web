import argparse
import hashlib
import json
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path
from unittest.mock import patch

from scripts import fetch_verifier_inputs as fetcher

ROOT = Path(__file__).resolve().parents[1]
BOUNDARY = datetime(2026, 10, 18, 12, 0, 0, tzinfo=timezone.utc)

def pair(timestamp="20261018_120001", modified=BOUNDARY):
    return [
        fetcher.RemoteFile(f"model_{timestamp}.step", 4, modified),
        fetcher.RemoteFile(f"cad_agent_{timestamp}.jsonl", 4, modified),
    ]

class FetchVerifierInputsTests(unittest.TestCase):
    def test_requires_current_filename_and_modification_times(self):
        boundary = BOUNDARY.timestamp()
        with self.assertRaises(fetcher.FetchError):
            fetcher.select_latest_pair(pair("20261018_115959", BOUNDARY), boundary)
        with self.assertRaises(fetcher.FetchError):
            fetcher.select_latest_pair(
                pair("20261018_120001", datetime(2026, 10, 18, 11, 59, 59, tzinfo=timezone.utc)),
                boundary,
            )
        self.assertEqual(fetcher.select_latest_pair(pair(), boundary)[0], "20261018_120001")

    def test_timeout_does_not_select_old_artifacts(self):
        with tempfile.TemporaryDirectory(dir=ROOT / "tests") as temporary:
            arguments = argparse.Namespace(
                spec=str(ROOT / "cad_agents/specifications/verifier_specification.json"),
                output_dir=temporary,
                base_url="https://example.test/webdav/",
                agents=["cad_agent", "adversarial_cad_agent"],
                not_before_epoch=BOUNDARY.timestamp(),
                poll_timeout=0,
                poll_interval=0,
                request_timeout=1,
                retries=0,
            )
            with patch.object(fetcher, "WebDAVClient") as client_type:
                client_type.return_value.list_files.return_value = pair("20261018_115959")
                with self.assertRaisesRegex(fetcher.FetchError, "timed out waiting"):
                    fetcher.fetch_inputs(arguments)
                client_type.return_value.download.assert_not_called()

    def test_writes_manifest_for_complete_current_pairs(self):
        class Client:
            base_url = "https://example.test/webdav/"

            def list_files(self, agent_name):
                return pair()

            def download(self, agent_name, remote_file, destination):
                content = b"STEP" if destination.suffix == ".step" else b"LOGS"
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(content)
                return {
                    "filename": remote_file.name,
                    "local_path": destination.as_posix(),
                    "webdav_url": f"{self.base_url}{agent_name}/{remote_file.name}",
                    "size": len(content),
                    "sha256": hashlib.sha256(content).hexdigest(),
                    "last_modified": remote_file.last_modified.isoformat(),
                }

        with tempfile.TemporaryDirectory(dir=ROOT / "tests") as temporary:
            arguments = argparse.Namespace(
                spec=str(ROOT / "cad_agents/specifications/verifier_specification.json"),
                output_dir=temporary,
                base_url="https://example.test/webdav/",
                agents=["cad_agent", "adversarial_cad_agent"],
                not_before_epoch=BOUNDARY.timestamp(),
                poll_timeout=0,
                poll_interval=0,
                request_timeout=1,
                retries=0,
            )
            with patch.object(fetcher, "WebDAVClient", return_value=Client()):
                manifest_path = ROOT / fetcher.fetch_inputs(arguments)
            manifest = json.loads(manifest_path.read_text())
            self.assertEqual(set(manifest["agents"]), set(arguments.agents))
            for agent_name in arguments.agents:
                records = manifest["agents"][agent_name]
                for key in ("step_file", "session_log"):
                    record = records[key]
                    self.assertEqual(
                        hashlib.sha256((ROOT / record["local_path"]).read_bytes()).hexdigest(),
                        record["sha256"],
                    )

    def test_changed_remote_file_is_not_published_in_manifest(self):
        class ChangingClient:
            base_url = "https://example.test/webdav/"

            def __init__(self):
                self.listing_count = 0

            def list_files(self, agent_name):
                self.listing_count += 1
                if self.listing_count > 2:
                    files = pair()
                    files[0] = fetcher.RemoteFile(files[0].name, 5, BOUNDARY)
                    return files
                return pair()

            def download(self, agent_name, remote_file, destination):
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(b"data")
                return {"filename": remote_file.name, "local_path": destination.as_posix()}

        with tempfile.TemporaryDirectory(dir=ROOT / "tests") as temporary:
            arguments = argparse.Namespace(
                spec=str(ROOT / "cad_agents/specifications/verifier_specification.json"),
                output_dir=temporary,
                base_url="https://example.test/webdav/",
                agents=["cad_agent", "adversarial_cad_agent"],
                not_before_epoch=BOUNDARY.timestamp(),
                poll_timeout=0,
                poll_interval=0,
                request_timeout=1,
                retries=0,
            )
            with patch.object(fetcher, "WebDAVClient", return_value=ChangingClient()):
                with self.assertRaisesRegex(fetcher.FetchError, "changed during retrieval"):
                    fetcher.fetch_inputs(arguments)
            self.assertEqual(list(Path(temporary).rglob("verifier_input.json")), [])

if __name__ == "__main__":
    unittest.main()
