#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

from journal_cli.peer import operations, pi_sync_mailbox as mailbox


AUTHORIZATION = """(((principal (galactica rocky *state* rocky))
 (key-index (0 -1))
 (path (mailbox inbox rocky rocky))
 (use! ((read-only? #t))) (put! #t) (run! #f) (retrieve #f)))
"""


class FixtureRunner:
    def __init__(self, *, route_ok=True, authorization=AUTHORIZATION, runtime=None, package_drift=False):
        self.route_ok = route_ok
        self.authorization = authorization
        self.runtime = runtime or {
            "enrollments": [{"file": "rocky-rocky.json", "status": "complete"}],
            "runtime": {
                "active": True,
                "available": True,
                "configDigest": "abc123",
                "lastSuccessfulPollAt": "2026-08-11T00:00:00.000Z",
                "pending": 0,
                "queued": 0,
                "deliveredInSession": 1,
                "badEntries": [],
                "peers": [{"identity": "rocky", "journal": "rocky"}],
                "recipients": [{
                    "identity": "rocky", "journal": "rocky", "owner": "rocky",
                    "route": ["galactica", "rocky"],
                }],
            }
        }
        self.package_drift = package_drift

    def __call__(self, argv, *, check=True):
        argv = [str(item) for item in argv]
        command = Path(argv[0]).name
        if command == "journal-cli" and argv[1:3] == ["peer", "route"]:
            if self.route_ok:
                return subprocess.CompletedProcess(argv, 0, "((route-source (local galactica)))\n", "")
            return subprocess.CompletedProcess(argv, 1, "(error 'bridge-error \"missing first hop\")\n", "")
        if command == "journal-cli" and argv[1:3] == ["peer", "authorizations"]:
            return subprocess.CompletedProcess(argv, 0, self.authorization, "")
        if command == "journal-cli" and argv[1:4] == ["peer", "enrollment", "status"]:
            return subprocess.CompletedProcess(argv, 0, json.dumps(self.runtime), "")
        if command == "rpm" and argv[1] == "-q":
            return subprocess.CompletedProcess(argv, 0, f"{argv[2]}-1.0-1.x86_64\n", "")
        if command == "rpm" and argv[1] == "-V":
            output = "S.5....T. /usr/bin/pi-sync\n" if self.package_drift else ""
            return subprocess.CompletedProcess(argv, 1 if output else 0, output, "")
        raise AssertionError(argv)


class HostilePackageRunner(FixtureRunner):
    def __call__(self, argv, *, check=True):
        argv = [str(item) for item in argv]
        if Path(argv[0]).name == "rpm" and argv[1] == "-q":
            return subprocess.CompletedProcess(argv, 0, "CREDENTIAL=query-do-not-print\n", "")
        if Path(argv[0]).name == "rpm" and argv[1] == "-V":
            return subprocess.CompletedProcess(argv, 1, "S.5....T. /tmp/CREDENTIAL=verify-do-not-print\n", "")
        return super().__call__(argv, check=check)


def bounded_route_output(*, suffix=""):
    header = "((route-source (local galactica)) (terminal-index 42) (roots ("
    payload = header + "x" * (mailbox.MAX_COMMAND_OUTPUT - len(header) - len(suffix)) + suffix
    return mailbox.OUTPUT_LIMIT_MARKER + payload


class BoundedRouteRunner(FixtureRunner):
    def __init__(self, suffix=""):
        super().__init__()
        self.suffix = suffix

    def __call__(self, argv, *, check=True):
        argv = [str(item) for item in argv]
        if Path(argv[0]).name == "journal-cli" and argv[1:3] == ["peer", "route"]:
            return subprocess.CompletedProcess(argv, 125, bounded_route_output(suffix=self.suffix), "")
        return super().__call__(argv, check=check)


class LegacyRunner(FixtureRunner):
    def __call__(self, argv, *, check=True):
        argv = [str(item) for item in argv]
        command = Path(argv[0]).name
        if command == "journal-cli" and argv[1:3] == ["peer", "route"]:
            return subprocess.CompletedProcess(
                argv, 1,
                "(error 'bridge-error \"Bridge is not committed at the selected local index: galactica/rocky -1\")\n",
                "",
            )
        if command == "journal-cli" and argv[1:3] == ["peer", "authorizations"]:
            raise mailbox.CommandError(argv, 2, "invalid choice: authorizations")
        if command == "journal-cli" and argv[1:4] == ["peer", "enrollment", "status"]:
            raise FileNotFoundError("/usr/bin/pi-sync-inbox")
        return super().__call__(argv, check=check)


class UnknownTrustRunner(LegacyRunner):
    def __call__(self, argv, *, check=True):
        argv = [str(item) for item in argv]
        if Path(argv[0]).name == "journal-cli" and argv[1:4] == ["peer", "enrollment", "status"]:
            return FixtureRunner().__call__(argv, check=check)
        return super().__call__(argv, check=check)


class MailboxDoctorTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        root = Path(self.temp.name)
        self.agent = root / "agent.json"
        self.inbox = root / "inbox.json"
        self.receipts = root / "receipts"
        self.receipts.mkdir()
        self.agent.write_text(json.dumps({
            "version": 1,
            "id": "kilgore",
            "sync": {"name": "kilgore"},
        }))
        self.inbox.write_text(json.dumps({
            "version": 1,
            "agent": "kilgore",
            "owner": "kilgore",
            "localJournal": "kilgore",
            "secretFile": "/private/do-not-print",
            "internalCredential": "never-print-this-value",
            "peers": [{"identity": "rocky", "journal": "rocky"}],
            "recipients": [{
                "identity": "rocky", "journal": "rocky", "owner": "rocky",
                "route": ["galactica", "rocky"],
            }],
        }))
        descriptor = {
            "localAgent": "kilgore",
            "localJournal": "kilgore",
            "peerJournal": "rocky",
            "peerIdentity": "rocky",
            "bridgePeer": "galactica",
            "bridgePeerSigningKeySha256": "b" * 64,
            "bridgePeerInterface": "http://galactica.invalid/interface",
            "route": ["galactica", "rocky"],
            "remoteOwner": "rocky",
            "incomingAuthorizedPath": "mailbox/inbox/rocky/rocky",
        }
        fingerprint = hashlib.sha256(json.dumps(descriptor, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        (self.receipts / "ambiguous-filename.json").write_text(json.dumps({
            "version": 1,
            "status": "complete",
            "fingerprint": fingerprint,
            "phases": ["validated", "runtime-config-published"],
            "descriptor": descriptor,
            "packagedExtensionSha256": {},
            "piRestartRequired": True,
        }))

    def tearDown(self):
        self.temp.cleanup()

    def args(self, **changes):
        values = {
            "address": "rocky@rocky",
            "route": "galactica/rocky",
            "remote_owner": "rocky",
            "local_owner": "kilgore",
            "agent_config": self.agent,
            "inbox_config": self.inbox,
            "journal_cli": Path("/usr/bin/journal-cli"),
            "enrollment_root": self.receipts,
            "rpm": Path("/usr/bin/rpm"),
            "packages": ["pi-agent-core", "pi-agent-sync", "pi-agent-sync-inbox"],
        }
        values.update(changes)
        return argparse.Namespace(**values)

    def test_healthy_relationship_joins_five_planes(self):
        output = mailbox.diagnose(self.args(), FixtureRunner())
        self.assertTrue(output["ok"])
        self.assertEqual(output["exitCode"], 0)
        self.assertEqual(output["topology"]["expectedInboundPrincipal"],
                         ["galactica", "rocky", "*state*", "rocky"])
        self.assertEqual(output["topology"]["inboundMailboxPrefix"],
                         ["mailbox", "inbox", "rocky", "rocky"])
        checks = {item["name"]: item for item in output["checks"]}
        self.assertEqual(checks["inbound-authorization"]["detail"]["capabilities"],
                         {"use!": {"read-only?": True}, "put!": True, "run!": False, "retrieve": False})

    def test_missing_peer_isolated_from_route_and_authorization(self):
        config = json.loads(self.inbox.read_text())
        config["peers"] = []
        self.inbox.write_text(json.dumps(config))
        output = mailbox.diagnose(self.args(), FixtureRunner())
        checks = {item["name"]: item for item in output["checks"]}
        self.assertEqual(output["exitCode"], mailbox.EXIT_LOCAL)
        self.assertEqual(checks["peer-config"]["status"], "fail")
        self.assertEqual(checks["route"]["status"], "pass")
        self.assertEqual(checks["inbound-authorization"]["status"], "pass")
        self.assertIn("no command was generated", checks["peer-config"]["remedy"])

    def test_bounded_valid_route_prefix_is_sufficient_without_materializing_object(self):
        output = mailbox.diagnose(self.args(), BoundedRouteRunner())
        check = next(item for item in output["checks"] if item["name"] == "route")
        self.assertEqual(check["status"], "pass")
        self.assertEqual(check["detail"]["inspectionStatus"], "ready-bounded-prefix")
        self.assertEqual(output["exitCode"], 0)

    def test_bounded_route_validation_keeps_the_runner_marker_overhead_tail(self):
        output = mailbox.diagnose(self.args(), BoundedRouteRunner(suffix="(error bridge-error)"))
        check = next(item for item in output["checks"] if item["name"] == "route")
        self.assertEqual(check["status"], "fail")
        self.assertFalse(output["ready"])
        self.assertEqual(output["exitCode"], mailbox.EXIT_ROUTE)

    def test_wrong_route_has_specific_category(self):
        output = mailbox.diagnose(self.args(route="rocky"), FixtureRunner(route_ok=False, authorization="()"))
        checks = {item["name"]: item for item in output["checks"]}
        self.assertEqual(checks["route"]["status"], "fail")
        self.assertIn("missing first hop", checks["route"]["detail"]["error"])
        self.assertEqual(output["exitCode"], mailbox.EXIT_ROUTE)
        self.assertNotIn("pi-sync authorize", checks["inbound-authorization"]["remedy"])
        self.assertIn("Blocked until route topology", checks["inbound-authorization"]["remedy"])
        self.assertNotIn("pi-sync-peer", checks["recipient-config"]["remedy"])

    def test_missing_authorization_reports_narrow_rule_without_secret_bearing_dry_run(self):
        output = mailbox.diagnose(self.args(), FixtureRunner(authorization="()"))
        checks = {item["name"]: item for item in output["checks"]}
        self.assertEqual(output["exitCode"], mailbox.EXIT_AUTHORIZATION)
        remedy = checks["inbound-authorization"]["remedy"]
        self.assertIn("mailbox/inbox/rocky/rocky", remedy)
        self.assertNotIn("--dry-run", remedy)
        self.assertNotIn("path ()", remedy)
        self.assertEqual(checks["inbound-authorization"]["detail"]["proposedRule"]["capabilities"],
                         {"use!": {"read-only?": True}, "put!": True, "run!": False, "retrieve": False})

    def test_output_is_repeatable_and_redacts_unread_config_fields(self):
        runner = FixtureRunner()
        first = mailbox.diagnose(self.args(), runner)
        second = mailbox.diagnose(self.args(), runner)
        self.assertEqual(first, second)
        encoded = json.dumps(first)
        self.assertNotIn("never-print-this-value", encoded)
        self.assertNotIn("/private/do-not-print", encoded)
        self.assertNotIn("credentials", encoded.lower())

    def test_peer_qualified_malformed_entry_fails_but_unrelated_entry_warns(self):
        runtime = FixtureRunner().runtime
        runtime["runtime"]["badEntries"] = [{"path": "rocky/rocky/00000000-0000-4000-8000-000000000001", "error": "invalid JSON"}]
        output = mailbox.diagnose(self.args(), FixtureRunner(runtime=runtime))
        checks = {item["name"]: item for item in output["checks"]}
        self.assertEqual(output["exitCode"], mailbox.EXIT_PROTOCOL)
        self.assertEqual(checks["envelope-diagnostics"]["status"], "fail")

        runtime["runtime"]["badEntries"] = [{"path": "other/other/00000000-0000-4000-8000-000000000002", "error": "invalid JSON"}]
        output = mailbox.diagnose(self.args(), FixtureRunner(runtime=runtime))
        checks = {item["name"]: item for item in output["checks"]}
        self.assertEqual(output["exitCode"], 0)
        self.assertEqual(checks["envelope-diagnostics"]["status"], "warn")

    def test_hot_added_peer_gets_explicit_receipt_warning(self):
        for path in self.receipts.iterdir():
            path.unlink()
        output = mailbox.diagnose(self.args(), FixtureRunner())
        checks = {item["name"]: item for item in output["checks"]}
        self.assertEqual(output["exitCode"], 0)
        self.assertEqual(checks["enrollment-metadata"]["status"], "warn")
        self.assertIn("not proof-bearing", checks["enrollment-metadata"]["detail"]["authority"])

    def test_receipt_matches_descriptor_content_not_ambiguous_filename(self):
        output = mailbox.diagnose(self.args(), FixtureRunner())
        check = next(item for item in output["checks"] if item["name"] == "enrollment-metadata")
        self.assertEqual(check["status"], "pass")
        self.assertNotIn("file", check["detail"]["matches"][0])
        self.assertRegex(check["detail"]["matches"][0]["fingerprint"], r"^[0-9a-f]{64}$")

    def test_legacy_missing_inspectors_preserve_partial_structured_diagnostics(self):
        output = mailbox.diagnose(self.args(), LegacyRunner())
        checks = {item["name"]: item for item in output["checks"]}
        self.assertEqual(output["exitCode"], mailbox.EXIT_RUNTIME)
        self.assertEqual(checks["route"]["status"], "warn")
        self.assertFalse(checks["route"]["detail"]["inspectionSupported"])
        self.assertEqual(checks["inbound-authorization"]["status"], "warn")
        self.assertFalse(checks["inbound-authorization"]["detail"]["inspectionSupported"])
        self.assertEqual(checks["runtime"]["status"], "fail")
        self.assertEqual(checks["envelope-diagnostics"]["status"], "warn")

    def test_unknown_trust_is_indeterminate_not_ready_or_exit_zero(self):
        output = mailbox.diagnose(self.args(), UnknownTrustRunner())
        self.assertTrue(output["healthy"])
        self.assertFalse(output["conclusive"])
        self.assertFalse(output["ready"])
        self.assertFalse(output["ok"])
        self.assertEqual(output["exitCode"], mailbox.EXIT_INDETERMINATE)
        self.assertEqual(output["unknownChecks"], ["route", "inbound-authorization"])

    def test_malformed_detail_exposes_code_not_message_body(self):
        runtime = FixtureRunner().runtime
        runtime["runtime"]["badEntries"] = [{
            "path": "rocky/rocky/00000000-0000-4000-8000-000000000001",
            "error": "Unexpected token H in private message Hello Alyosha; CREDENTIAL=do-not-print",
        }]
        output = mailbox.diagnose(self.args(), FixtureRunner(runtime=runtime))
        encoded = json.dumps(output)
        self.assertIn("envelope-json-invalid", encoded)
        self.assertNotIn("Hello Alyosha", encoded)
        self.assertNotIn("do-not-print", encoded)

        runtime["runtime"]["badEntries"][0]["path"] = "rocky/rocky/CREDENTIAL=path-do-not-print"
        output = mailbox.diagnose(self.args(), FixtureRunner(runtime=runtime))
        encoded = json.dumps(output)
        self.assertIn("<invalid-or-redacted>", encoded)
        self.assertNotIn("path-do-not-print", encoded)

    def test_unhashable_receipt_status_and_phases_are_invalid_not_tracebacks(self):
        path = next(self.receipts.iterdir())
        original = json.loads(path.read_text())
        for field, value in (("status", {}), ("status", []), ("status", None),
                             ("phases", [{}]), ("phases", [[]]), ("phases", [None])):
            receipt = json.loads(json.dumps(original))
            receipt[field] = value
            path.write_text(json.dumps(receipt))
            output = mailbox.diagnose(self.args(), FixtureRunner())
            check = next(item for item in output["checks"] if item["name"] == "enrollment-metadata")
            self.assertEqual(check["status"], "warn")
            self.assertEqual(check["detail"]["invalidReceiptCount"], 1)

    def test_untrusted_receipt_strings_are_rejected_and_never_emitted(self):
        for path in self.receipts.iterdir():
            receipt = json.loads(path.read_text())
            receipt["fingerprint"] = "CREDENTIAL=receipt-do-not-print"
            receipt["status"] = "CREDENTIAL=status-do-not-print"
            receipt["phases"] = ["CREDENTIAL=phase-do-not-print"]
            path.unlink()
            (self.receipts / "CREDENTIAL=filename-do-not-print.json").write_text(json.dumps(receipt))
        output = mailbox.diagnose(self.args(), FixtureRunner())
        encoded = json.dumps(output)
        self.assertNotIn("do-not-print", encoded)
        check = next(item for item in output["checks"] if item["name"] == "enrollment-metadata")
        self.assertEqual(check["status"], "warn")
        self.assertEqual(check["detail"]["invalidReceiptCount"], 1)

    def test_hostile_package_output_is_redacted(self):
        output = mailbox.diagnose(self.args(), HostilePackageRunner())
        encoded = json.dumps(output)
        self.assertNotIn("query-do-not-print", encoded)
        self.assertNotIn("verify-do-not-print", encoded)
        self.assertIn("<redacted>", encoded)

    def test_hostile_runtime_selected_strings_are_redacted(self):
        runtime = FixtureRunner().runtime
        runtime["runtime"]["configDigest"] = "CREDENTIAL=digest-do-not-print"
        runtime["runtime"]["lastSuccessfulPollAt"] = "CREDENTIAL=time-do-not-print"
        output = mailbox.diagnose(self.args(), FixtureRunner(runtime=runtime))
        encoded = json.dumps(output)
        self.assertNotIn("do-not-print", encoded)
        self.assertIn("<invalid-or-redacted>", encoded)

    def test_package_drift_is_reported_separately(self):
        output = mailbox.diagnose(self.args(), FixtureRunner(package_drift=True))
        checks = {item["name"]: item for item in output["checks"]}
        self.assertEqual(output["exitCode"], mailbox.EXIT_PACKAGE)
        self.assertEqual(checks["packages"]["status"], "fail")
        self.assertIn("/usr/bin/pi-sync", checks["packages"]["detail"]["pi-agent-core"]["verifyLines"][0])


    def test_json_mode_keeps_envelope_for_fatal_input_error(self):
        result = subprocess.run([
            sys.executable, "-m", "journal_cli", "peer", "mailbox-doctor", "doctor", "rocky@rocky", "--route", "galactica/rocky",
            "--agent-config", str(Path(self.temp.name) / "missing.json"), "--json",
        ], text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
           env={"PYTHONDONTWRITEBYTECODE": "1", "PYTHONPATH": str(Path(__file__).resolve().parents[1])})
        self.assertEqual(result.returncode, 2)
        output = json.loads(result.stdout)
        self.assertEqual(output["exitCode"], 2)
        self.assertEqual(output["fatal"]["category"], "input")


class CommandRunnerTest(unittest.TestCase):
    def test_missing_executable_and_resource_bounded_output(self):
        missing = mailbox.run_command(["/definitely/missing"], check=False)
        self.assertEqual(missing.returncode, 127)
        large = mailbox.run_command([sys.executable, "-c", "print('x' * 70000)"], check=False)
        self.assertEqual(large.returncode, 125)
        self.assertLessEqual(len(large.stdout), mailbox.MAX_COMMAND_OUTPUT + 100)
        self.assertIn("output exceeded", large.stdout)

    def test_timeout_applies_after_helper_closes_stdout(self):
        previous = mailbox.COMMAND_TIMEOUT
        mailbox.COMMAND_TIMEOUT = 0.1
        try:
            result = mailbox.run_command([sys.executable, "-c", "import os,time;os.close(1);time.sleep(5)"], check=False)
        finally:
            mailbox.COMMAND_TIMEOUT = previous
        self.assertEqual(result.returncode, 124)

    def test_timeout_kills_descendant_process_group(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "child-survived"
            child = f"import time,pathlib;time.sleep(.3);pathlib.Path({str(marker)!r}).write_text('survived')"
            parent = ("import subprocess,sys,time;"
                      f"subprocess.Popen([sys.executable,'-c',{child!r}]);"
                      "time.sleep(5)")
            previous = mailbox.COMMAND_TIMEOUT
            mailbox.COMMAND_TIMEOUT = 0.1
            try:
                result = mailbox.run_command([sys.executable, "-c", parent], check=False)
            finally:
                mailbox.COMMAND_TIMEOUT = previous
            self.assertEqual(result.returncode, 124)
            time.sleep(0.4)
            self.assertFalse(marker.exists())


class ParserTest(unittest.TestCase):
    def test_bounded_route_prefix_rejects_lookalike_error_sensitive_and_incomplete_output(self):
        marker = mailbox.OUTPUT_LIMIT_MARKER
        valid = bounded_route_output()
        self.assertTrue(mailbox.bounded_route_success(125, valid))
        self.assertFalse(mailbox.bounded_route_success(125, marker + "((route-source-ish (local))" + "x" * (mailbox.MAX_COMMAND_OUTPUT - 28)))
        self.assertFalse(mailbox.bounded_route_success(125, marker + "((route-source (local))" + "x" * (mailbox.MAX_COMMAND_OUTPUT - 22)))
        self.assertFalse(mailbox.bounded_route_success(125, valid[:1000]))
        self.assertFalse(mailbox.bounded_route_success(125, bounded_route_output(suffix="(error 'bridge-error)")))
        hostile = "CREDENTIAL=do-not-print\n"
        self.assertFalse(mailbox.bounded_route_success(125, marker + hostile + "x" * (mailbox.MAX_COMMAND_OUTPUT - len(hostile))))
        self.assertFalse(mailbox.bounded_route_success(125, "wrong marker\n" + valid[len(marker):]))
        self.assertFalse(mailbox.bounded_route_success(125, valid + "x"))

    def test_authorization_parser(self):
        rules = mailbox.authorization_rules(AUTHORIZATION)
        self.assertEqual(rules[0]["principal"], ["galactica", "rocky", "*state*", "rocky"])
        self.assertEqual(mailbox.association(rules[0]["use!"]), {"read-only?": True})
        self.assertIs(rules[0]["put!"], True)
        self.assertIs(rules[0]["run!"], False)
        self.assertIs(rules[0]["retrieve"], False)

    def test_address_and_route_validation(self):
        self.assertEqual(mailbox.parse_address("rocky@rocky"), ("rocky", "rocky"))
        self.assertEqual(mailbox.parse_route("galactica/rocky"), ["galactica", "rocky"])
        with self.assertRaises(ValueError):
            mailbox.parse_address("rocky")
        with self.assertRaises(ValueError):
            mailbox.parse_route("galactica//rocky")


class PeerSigningKeyTests(unittest.TestCase):
    def test_digest_hashes_public_key_not_removed_identity(self):
        public_key = bytes(range(64))
        client = mock.Mock()
        client.call_json.return_value = {"public-key": {"*type/byte-vector*": public_key.hex()}}
        output = io.StringIO()
        with mock.patch("sys.stdout", output):
            self.assertEqual(operations.signing_key_digest(client), 0)
        result = json.loads(output.getvalue())
        digest = hashlib.sha256(public_key).digest()
        self.assertEqual(result["base64"], base64.b64encode(digest).decode())
        self.assertEqual(result["sha256"], digest.hex())
        self.assertNotIn("identity", output.getvalue().lower())


if __name__ == "__main__":
    unittest.main()
