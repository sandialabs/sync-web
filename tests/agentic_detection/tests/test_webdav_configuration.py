import os
import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def resolve_url(**environment):
    clean_environment = {
        key: value
        for key, value in os.environ.items()
        if key not in {"WEBDAV_HOST", "WEBDAV_PORT", "WEBDAV_PATH", "WEBDAV_URL"}
    }
    clean_environment.update(environment)
    return subprocess.run(
        ["bash", "-c", "source scripts/webdav_config.sh; resolve_webdav_url"],
        cwd=ROOT,
        env=clean_environment,
        text=True,
        capture_output=True,
        check=False,
    )


class WebDAVConfigurationTests(unittest.TestCase):
    def test_derives_default_url_from_host(self):
        result = resolve_url(WEBDAV_HOST="server.example")
        self.assertEqual(result.returncode, 0)
        self.assertEqual(
            result.stdout.strip(),
            "http://server.example:8192/webdav/stage/admin/",
        )

    def test_full_url_takes_precedence(self):
        result = resolve_url(
            WEBDAV_HOST="ignored.example",
            WEBDAV_URL="https://dav.example/custom",
        )
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout.strip(), "https://dav.example/custom/")

    def test_missing_configuration_fails(self):
        result = resolve_url()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Set WEBDAV_HOST", result.stderr)

    def test_invalid_port_fails(self):
        result = resolve_url(WEBDAV_HOST="server.example", WEBDAV_PORT="invalid")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("WEBDAV_PORT must be", result.stderr)


if __name__ == "__main__":
    unittest.main()
