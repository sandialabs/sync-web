import os
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from locustfile import HelloWorldUser


class _FakeResponse:
    def __init__(self, text):
        self.text = text


class _FakeClient:
    def __init__(self, response_text):
        self.response_text = response_text
        self.calls = []

    def post(self, path, json):
        self.calls.append((path, json))
        return _FakeResponse(self.response_text)


class _FakeUser:
    def __init__(self, client):
        self.client = client


class HelloWorldUserTests(unittest.TestCase):
    def test_posts_expected_payload_to_interface_json(self):
        client = _FakeClient('{"ok":true}')
        user = _FakeUser(client)

        with patch.dict(os.environ, {"SECRET": "test-secret"}, clear=False):
            with patch("locustfile.random.randint", side_effect=[111, 222]):
                with patch("builtins.print"):
                    HelloWorldUser.hello_world(user)

        self.assertEqual(len(client.calls), 1)
        path, payload = client.calls[0]
        self.assertEqual(path, "/interface/json")
        self.assertEqual(payload["function"], "set!")
        self.assertEqual(payload["arguments"]["path"], [["*state*", "locust", "key-111"]])
        self.assertEqual(
            payload["arguments"]["value"], {"*type/string*": "val-222"}
        )
        self.assertEqual(
            payload["authentication"], {"*type/string*": "test-secret"}
        )

    def test_prints_truncated_request_and_response(self):
        client = _FakeClient("x" * 120)
        user = _FakeUser(client)

        with patch.dict(os.environ, {"SECRET": "test-secret"}, clear=False):
            with patch("locustfile.random.randint", side_effect=[1, 2]):
                with patch("builtins.print") as print_mock:
                    HelloWorldUser.hello_world(user)

        printed = print_mock.call_args[0][0]
        self.assertTrue(printed.startswith("REQ: "))
        self.assertIn("RESP:", printed)
        self.assertIn("...", printed)


if __name__ == "__main__":
    unittest.main()
