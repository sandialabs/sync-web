#!/usr/bin/python3
import io
import json
import unittest
from unittest import mock

from journal_cli.profile import commands as p

ENDPOINT = "https://publisher.example/interface"
LOCAL = "http://127.0.0.1:8192/interface"


def profile(extra=b""):
    return (
        b'((schema sync-agent-profile-v2) (endpoint "' + ENDPOINT.encode() + b'")'
        b' (journal rocky) (owner rocky) (revision 1) (display-name "Rocky")'
        b' (bio "An Eridian engineer.")' + extra + b')'
    )


def namespace(**values):
    class Namespace:
        pass
    result = Namespace()
    for key, value in values.items():
        setattr(result, key, value)
    return result


class ReaderTests(unittest.TestCase):
    def validate(self, raw):
        return p.validate_profile(p.parse_profile(raw), ENDPOINT, "rocky", "rocky")

    def test_required_and_optional_fields(self):
        value = self.validate(profile(b' (pronouns ("he/him" "they/them"))'))
        self.assertEqual(value.endpoint, ENDPOINT)
        self.assertEqual(value.revision, 1)
        self.assertEqual(value.display_name, "Rocky")
        self.assertEqual(value.pronouns, ("he/him", "they/them"))
        self.assertEqual(value.bio, "An Eridian engineer.")

    def test_round_trip(self):
        value = p.parse_profile(profile())
        self.assertEqual(p.parse_profile(p.scheme(value).encode()), value)

    def test_exact_fields_and_bindings(self):
        with self.assertRaisesRegex(p.ProfileError, "missing"):
            self.validate(b"((schema sync-agent-profile-v2))")
        with self.assertRaisesRegex(p.ProfileError, "duplicate"):
            self.validate(profile(b' (display-name "Other")'))
        with self.assertRaisesRegex(p.ProfileError, "unknown"):
            self.validate(profile(b" (role agent)"))
        value = p.parse_profile(profile())
        with self.assertRaisesRegex(p.ProfileError, "endpoint"):
            p.validate_profile(value, "https://other.example/interface", "rocky", "rocky")
        with self.assertRaisesRegex(p.ProfileError, "Journal"):
            p.validate_profile(value, ENDPOINT, "other", "rocky")
        with self.assertRaisesRegex(p.ProfileError, "owner"):
            p.validate_profile(value, ENDPOINT, "rocky", "other")

    def test_clean_break_rejects_old_identity_field(self):
        raw = profile().replace(b'(endpoint "https://publisher.example/interface")', b'(identity-id #u(1))')
        with self.assertRaises(p.ProfileError):
            self.validate(raw)

    def test_endpoint_must_be_canonical_and_secure(self):
        for value in ["http://publisher.example/interface", "https://Publisher.example/interface", "https://publisher.example:443/interface", "https://publisher.example/other"]:
            raw = profile().replace(ENDPOINT.encode(), value.encode())
            with self.subTest(value=value), self.assertRaises(p.ProfileError):
                p.validate_profile(p.parse_profile(raw), value, "rocky", "rocky")

    def test_revision_and_bio_rules(self):
        for revision in [b"0", b"-1"]:
            with self.subTest(revision=revision), self.assertRaisesRegex(p.ProfileError, "revision"):
                self.validate(profile().replace(b"(revision 1)", b"(revision " + revision + b")"))
        with self.assertRaisesRegex(p.ProfileError, "non-whitespace"):
            self.validate(profile().replace(b'An Eridian engineer.', b'   '))
        self.assertIn("line two", self.validate(profile().replace(b"An Eridian engineer.", b"line one\\nline two")).bio)
        with self.assertRaisesRegex(p.ProfileError, "2048"):
            self.validate(profile().replace(b"An Eridian engineer.", b"x" * 2049))

    def test_string_controls_and_bounds(self):
        for text in ["bad\nname", "bad\x7fname", "bad\u202ename", "bad\u2066name"]:
            with self.subTest(text=text), self.assertRaisesRegex(p.ProfileError, "control"):
                p.validate_string(text, "field", 128)
        with self.assertRaisesRegex(p.ProfileError, "128"):
            p.validate_string("x" * 129, "display-name", 128)

    def test_reader_bounds_and_forbidden_forms(self):
        for raw in [b"'x", b"#t", b"(a . b)", b"#(a)", b"(a ; comment\n b)", b"(a) trailing", b"`(a)"]:
            with self.subTest(raw=raw), self.assertRaises(p.ProfileError):
                p.parse_profile(raw)
        with self.assertRaisesRegex(p.ProfileError, "nesting"):
            p.parse_profile(b"(" * 9 + b")" * 9)
        with self.assertRaisesRegex(p.ProfileError, "4096"):
            p.parse_profile(b"x" * 4097)


class TransportTests(unittest.TestCase):
    def test_byte_vector_and_nothing(self):
        self.assertEqual(p.transport_value("#u(0 1 255)"), b"\x00\x01\xff")
        self.assertIsNone(p.transport_value(" (nothing)\n"))

    def test_committed_get_uses_retrieve_index_projection(self):
        response = '((content #u(1 2)) (indexes (7)))'
        with mock.patch.object(p, "post_bounded", return_value=response) as post:
            self.assertEqual(p.committed_get(LOCAL, "secret", "rocky", "rocky"), (b"\x01\x02", 7))
        request = post.call_args.args[1]
        self.assertIn("(path (-1 *state* rocky profile.scm))", request)
        self.assertIn("(index? #t)", request)
        self.assertNotIn("(function size)", request)

    def test_federated_get_shape(self):
        with mock.patch.object(p, "post_bounded", return_value="#u(1 2)") as post:
            self.assertEqual(p.federated_get(LOCAL, "secret", "rocky", "galactica/grace", "grace"), b"\x01\x02")
        request = post.call_args.args[1]
        self.assertIn("(route-target (galactica grace))", request)
        self.assertIn("(function use!)", request)

    def test_public_endpoint_requires_exact_self_identification(self):
        client = mock.Mock()
        client.post.return_value = [[p.WireSymbol("interface"), [[p.WireSymbol("endpoint"), ENDPOINT]]]]
        with mock.patch.object(p, "InterfaceClient", return_value=client):
            self.assertEqual(p.observe_public_endpoint(ENDPOINT), ENDPOINT)
        client.post.return_value = [[p.WireSymbol("interface"), [[p.WireSymbol("endpoint"), "https://other.example/interface"]]]]
        with mock.patch.object(p, "InterfaceClient", return_value=client), self.assertRaisesRegex(p.RemoteError, "self-identify"):
            p.observe_public_endpoint(ENDPOINT)


class CommandTests(unittest.TestCase):
    def validated(self):
        value = p.parse_profile(profile())
        return value, p.validate_profile(value, ENDPOINT, "rocky", "rocky")

    def test_create_only_request_and_endpoint_match(self):
        args = namespace(file="ignored", publisher_endpoint=LOCAL, journal="rocky", owner="rocky", config="config",
                         state_dir="state", apply=True, confirm_owner="rocky", raw_output=None)
        raw = profile().replace(ENDPOINT.encode(), LOCAL.encode())
        value = p.parse_profile(raw)
        validated = p.validate_profile(value, LOCAL, "rocky", "rocky")
        with mock.patch.object(p, "load_validated", return_value=(raw, value, validated)), \
             mock.patch.object(p, "read_config", return_value=({"id": "rocky"}, LOCAL)), \
             mock.patch.object(p, "observe_public_endpoint", return_value=LOCAL), \
             mock.patch.object(p, "read_secret", return_value="secret"), \
             mock.patch.object(p, "local_get", side_effect=[None, raw]), \
             mock.patch.object(p, "post_bounded", return_value="#t") as post, mock.patch("sys.stdout"):
            p.command_publish(args)
        request = post.call_args.args[1]
        self.assertIn("(expected (nothing))", request)
        self.assertIn("(expression? #t)", request)
        self.assertIn("(function put!)", request)

    def test_existing_profile_stops_before_write(self):
        args = namespace(file="ignored", publisher_endpoint=LOCAL, journal="rocky", owner="rocky", config="config",
                         state_dir="state", apply=True, confirm_owner="rocky", raw_output=None)
        raw = profile().replace(ENDPOINT.encode(), LOCAL.encode())
        value = p.parse_profile(raw); validated = p.validate_profile(value, LOCAL, "rocky", "rocky")
        with mock.patch.object(p, "load_validated", return_value=(raw, value, validated)), \
             mock.patch.object(p, "read_config", return_value=({"id": "rocky"}, LOCAL)), \
             mock.patch.object(p, "observe_public_endpoint", return_value=LOCAL), \
             mock.patch.object(p, "read_secret", return_value="secret"), \
             mock.patch.object(p, "local_get", return_value=raw), mock.patch.object(p, "post_bounded") as post:
            with self.assertRaisesRegex(p.ProfileError, "already exists"):
                p.command_publish(args)
        post.assert_not_called()

    def test_fetch_keeps_publisher_and_entry_endpoints_distinct(self):
        args = namespace(route="galactica/rocky", owner="rocky", journal="rocky", publisher_endpoint=ENDPOINT,
                         config="config", state_dir="state", evidence_json=True, agent_json=False, raw_output=None)
        output = io.StringIO()
        with mock.patch("sys.stdout", output), mock.patch.object(p, "observe_public_endpoint", return_value=ENDPOINT), \
             mock.patch.object(p, "read_config", return_value=({"id": "observer"}, LOCAL)), \
             mock.patch.object(p, "read_secret", return_value="secret"), \
             mock.patch.object(p, "federated_get", return_value=profile()):
            p.command_fetch(args)
        result = json.loads(output.getvalue())
        self.assertEqual(result["publisherEndpoint"], ENDPOINT)
        self.assertEqual(result["entryEndpoint"], LOCAL)
        self.assertEqual(result["route"], ["galactica", "rocky"])
        self.assertNotIn("displayName", result)
        self.assertNotIn("bio", result)

    def test_agent_view_labels_profile_untrusted(self):
        value = p.validate_profile(p.parse_profile(profile()), ENDPOINT, "rocky", "rocky")
        result = p.agent_record(value, profile(), "galactica/rocky", LOCAL)
        self.assertFalse(result["instructions"])
        self.assertFalse(result["trustedForAuthority"])
        self.assertEqual(result["source"]["publisherEndpoint"], ENDPOINT)
        self.assertEqual(result["source"]["entryEndpoint"], LOCAL)
        self.assertEqual(result["profile"]["bio"], "An Eridian engineer.")

    def test_wait_commit_uses_returned_absolute_index(self):
        raw = profile().replace(ENDPOINT.encode(), LOCAL.encode())
        args = namespace(expected_sha256=__import__("hashlib").sha256(raw).hexdigest(), publisher_endpoint=LOCAL,
                         journal="rocky", owner="rocky", config="config", state_dir="state", timeout=1,
                         interval=0, raw_output=None)
        output = io.StringIO()
        with mock.patch("sys.stdout", output), mock.patch.object(p, "read_config", return_value=({"id": "rocky"}, LOCAL)), \
             mock.patch.object(p, "read_secret", return_value="secret"), mock.patch.object(p, "committed_get", return_value=(raw, 42)):
            p.command_wait_commit(args)
        result = json.loads(output.getvalue())
        self.assertEqual(result["historyIndexes"], [42])
        self.assertEqual(result["outcome"], "verified")


class DisplayTests(unittest.TestCase):
    def test_identity_always_visible_and_isolated(self):
        value = p.validate_profile(p.parse_profile(profile(b' (pronouns ("he/him"))')), ENDPOINT, "rocky", "rocky")
        rendered = p.render(value)
        self.assertIn("rocky@rocky", rendered)
        self.assertIn("Rocky", rendered)
        self.assertIn("he/him", rendered)
        self.assertIn(p.FSI, rendered)
        self.assertIn(p.PDI, rendered)


if __name__ == "__main__":
    unittest.main()
