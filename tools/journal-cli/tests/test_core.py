from __future__ import annotations

import argparse
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from journal_cli import message
from journal_cli.cli import GROUPS, _journal, parser
from journal_cli.client import InvalidRequest, JournalClient, operation_may_mutate, request_expression
from journal_cli.config import Config, ConfigError, load_config


class CoreTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        root = Path(self.temp.name)
        self.secret = root / "secret"
        self.secret.write_text("private\n")
        self.secret.chmod(0o600)
        self.config = Config(
            endpoint="http://127.0.0.1:8192/interface", owner="alice", journal="alice",
            credential_file=self.secret, timeout_seconds=3, state_dir=root, inbox_config=root / "inbox.json",
        )

    def tearDown(self):
        self.temp.cleanup()

    def test_surface_is_one_frozen_command_tree(self):
        root = parser()
        choices = next(action.choices for action in root._actions if getattr(action, "choices", None))
        self.assertEqual(tuple(choices), GROUPS)

    def test_config_accepts_fixed_agent_shape_and_rejects_external_http(self):
        path = Path(self.temp.name) / "agent.json"
        path.write_text(json.dumps({"version": 1, "id": "alice", "sync": {"name": "alice", "localInterface": "http://127.0.0.1:8192/interface"}, "credentialFile": str(self.secret)}))
        self.assertEqual(load_config(path).owner, "alice")
        path.write_text(json.dumps({"version": 1, "id": "alice", "endpoint": "http://example.test/interface"}))
        with self.assertRaises(ConfigError):
            load_config(path)

    def test_request_rejects_every_legacy_operation(self):
        for operation in ("get", "get-batch", "set", "set!", "set-batch!", "call!", "resolve", "resolve-batch"):
            with self.subTest(operation=operation), self.assertRaises(InvalidRequest):
                request_expression(operation, {}, self.config)

    def test_mutation_classification_includes_mutating_use(self):
        self.assertFalse(operation_may_mutate("use!", {"read-only?": True}))
        self.assertTrue(operation_may_mutate("use!", {"read-only?": False}))
        self.assertTrue(operation_may_mutate("use-batch!", {}))
        self.assertTrue(operation_may_mutate("put!", {}))
        self.assertFalse(operation_may_mutate("retrieve", {}))

    def test_current_operations_encode_credentials_only_in_body(self):
        expression = request_expression("put!", {"path": ["x"], "value": b"abc", "expression?": False}, self.config)
        self.assertIn("(function put!)", expression)
        self.assertIn('credentials "private"', expression)
        self.assertNotIn("private", self.config.endpoint)

    def test_local_admin_operations_use_interface_admin_and_wire_names(self):
        admins = request_expression("admins", {}, self.config)
        self.assertIn("(function *admins-get*)", admins)
        self.assertIn('(authentication ((credentials "private")))', admins)
        self.assertNotIn("(identity", admins)

        bridge = request_expression("bridge!", {"name": "bob"}, self.config)
        self.assertIn("(function bridge!)", bridge)
        self.assertNotIn("(identity", bridge)

        owner = request_expression("authorize!", {"user": ["*state*", "alice"]}, self.config)
        self.assertIn("(identity (*state* alice))", owner)

    def test_json_local_admin_uses_wire_name_without_owner_identity(self):
        client = JournalClient(self.config)
        with mock.patch.object(client, "_post", return_value=b"{}") as post:
            client.call_json("admins", {})
        request = json.loads(post.call_args.args[0])
        self.assertEqual(request["function"], "*admins-get*")
        self.assertNotIn("identity", request["authentication"])
        self.assertEqual(request["authentication"]["credentials"], {"*type/string*": "private"})

    def test_peer_bridge_uses_interface_admin_without_owner_identity(self):
        from journal_cli.peer import operations
        client = JournalClient(self.config)
        with mock.patch.object(client, "post_scheme", return_value="#t") as post:
            self.assertEqual(operations.bridge(client, "bob", "https://bob.test/interface", "alice"), 0)
        expression = post.call_args.args[0]
        self.assertIn('(authentication ((credentials "private")))', expression)
        self.assertNotIn("(identity", expression)

    def test_json_scheme_error_is_explicit_rejection_and_secret_is_redacted(self):
        client = JournalClient(self.config)
        with mock.patch.object(client, "_post", return_value=json.dumps([
            "error", {"*type/quoted*": "authorization-error"},
            {"*type/string*": "Not authorized private"},
        ]).encode()):
            from journal_cli.client import ExplicitReject
            with self.assertRaises(ExplicitReject) as caught:
                client.post_json({"function": "use!"})
        self.assertNotIn("private", str(caught.exception))
        self.assertIn("<redacted>", str(caught.exception))

    def test_identity_shorthand_and_exact_message_recipient(self):
        config = {"recipients": [{"identity": "bob", "journal": "bob", "owner": "bob", "route": ["galactica", "bob"]}]}
        self.assertEqual(message.recipient(config, "bob")["journal"], "bob")
        self.assertEqual(message.recipient(config, "bob@bob")["journal"], "bob")
        with self.assertRaises(InvalidRequest):
            message.recipient(config, "bob@elsewhere")

    def test_message_send_uses_put_and_preserves_envelope(self):
        self.config.inbox_config.write_text(json.dumps({
            "version": 1, "owner": "alice", "localJournal": "alice", "maxMessageBytes": 65536,
            "peers": [{"identity": "bob", "journal": "bob"}],
            "recipients": [{"identity": "bob", "journal": "bob", "owner": "bob", "route": ["galactica", "bob"]}],
        }))
        client = mock.create_autospec(JournalClient, instance=True)
        client.config = self.config
        client.post_scheme.return_value = "#t"
        result = message.send(client, "bob", "hello")
        expression = client.post_scheme.call_args.args[0]
        self.assertIn("(function put!)", expression)
        self.assertNotIn("(function set!)", expression)
        self.assertEqual(result["from"], "alice@alice")
        self.assertEqual(result["to"], "bob@bob")
        self.assertEqual(result["outcome"], "write-accepted")

    def test_conditional_put_false_is_rejected_without_retry(self):
        args = argparse.Namespace(
            command="request", function="put!", arguments_json="{}", identity=None, route=None,
        )
        client = mock.create_autospec(JournalClient, instance=True)
        client.call_json.return_value = False
        output = io.StringIO()
        with mock.patch("sys.stdout", output):
            self.assertEqual(_journal(args, client), 3)
        self.assertEqual(client.call_json.call_count, 1)
        self.assertEqual(json.loads(output.getvalue())["outcome"], "rejected")

    def test_routed_conditional_put_scheme_false_is_rejected_without_retry(self):
        args = argparse.Namespace(
            command="request", function="put!", arguments_json="{}", identity=None, route="peer",
        )
        client = mock.create_autospec(JournalClient, instance=True)
        client.call.return_value = "#f\n"
        output = io.StringIO()
        with mock.patch("sys.stdout", output):
            self.assertEqual(_journal(args, client), 3)
        client.call.assert_called_once_with("put!", {}, identity=None, route=["peer"])
        self.assertEqual(json.loads(output.getvalue())["outcome"], "rejected")

    def test_peer_authorization_uses_only_16_grants_and_redacts_secret(self):
        from journal_cli.peer import operations
        client = JournalClient(self.config)
        args = argparse.Namespace(
            owner="alice", remote_route="galactica/bob", remote_identity="bob",
            path="mailbox/inbox/alice/alice", read_only=False, retrieve=False,
            run=False, revoke=False, dry_run=True,
        )
        output = io.StringIO()
        with mock.patch("sys.stdout", output):
            self.assertEqual(operations.authorize(client, args), 0)
        text = output.getvalue()
        self.assertIn('"put!": true', text)
        self.assertIn('"use!"', text)
        self.assertIn('"run!": false', text)
        self.assertIn('"retrieve": false', text)
        self.assertNotIn("private", text)
        self.assertNotIn("(get ", text)
        self.assertNotIn("(set! ", text)

    def test_recipient_route_replace_is_exact_and_does_not_change_trust(self):
        from journal_cli.peer import operations
        value = {
            "version": 1, "owner": "alice", "localJournal": "alice",
            "peers": [{"identity": "bob", "journal": "bob"}],
            "recipients": [{"identity": "bob", "journal": "bob", "owner": "bob", "route": ["old", "bob"]}],
        }
        self.config.inbox_config.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
        digest = hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        args = argparse.Namespace(
            journal="bob", identity="bob", owner="bob", from_route="old/bob",
            to_route="new/bob", expect_config_digest=digest, apply=True,
        )
        output = io.StringIO()
        with mock.patch("sys.stdout", output):
            self.assertEqual(operations.recipient_route_replace(JournalClient(self.config), args), 0)
        result = json.loads(output.getvalue())
        self.assertFalse(result["trustChanged"])
        written = json.loads(self.config.inbox_config.read_text())
        self.assertEqual(written["peers"], value["peers"])
        self.assertEqual(written["recipients"][0]["route"], ["new", "bob"])

    def test_group_reports_independent_outcomes_without_retry(self):
        recipients = [
            {"identity": "bob", "journal": "bob", "owner": "bob", "route": ["g", "bob"]},
            {"identity": "carol", "journal": "carol", "owner": "carol", "route": ["g", "carol"]},
        ]
        self.config.inbox_config.write_text(json.dumps({
            "version": 1, "owner": "alice", "localJournal": "alice", "maxMessageBytes": 65536,
            "peers": recipients, "recipients": recipients,
        }))
        client = mock.create_autospec(JournalClient, instance=True)
        client.config = self.config
        client.post_scheme.side_effect = ["#t", OSError("uncertain")]
        result = message.send_group(client, ["bob", "carol"], "hello", "223e4567-e89b-42d3-a456-426614174000")
        self.assertEqual(client.post_scheme.call_count, 2)
        self.assertEqual([item["outcome"] for item in result["outcomes"]], ["write-accepted", "failed-or-ambiguous"])


if __name__ == "__main__":
    unittest.main()
