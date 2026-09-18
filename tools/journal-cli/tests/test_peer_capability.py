#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path
import unittest

from journal_cli.peer import pi_sync_capability_card as card


EXAMPLE = Path(__file__).with_name("capability-card.example.json")


class CapabilityCardTest(unittest.TestCase):
    def load(self):
        encoded = EXAMPLE.read_bytes()
        return json.loads(encoded), len(encoded)

    def test_example_is_valid_and_small(self):
        value, size = self.load()
        self.assertEqual(card.validate(value, size), [])
        self.assertLess(size, card.MAX_BYTES)

    def test_expected_publisher_binding_is_checked(self):
        value, size = self.load()
        errors = card.validate(value, size, expected_identity="other", expected_journal="other", expected_owner="other")
        self.assertEqual(sum("does not match expected" in error for error in errors), 3)

    def test_unknown_and_secret_fields_fail(self):
        value, size = self.load()
        value["privateEndpoint"] = "http://private.invalid"
        errors = card.validate(value, size)
        self.assertTrue(any("unknown fields" in error for error in errors))
        self.assertTrue(any("prohibited" in error for error in errors))

    def test_nested_credentials_fail(self):
        value, size = self.load()
        value["features"]["credential"] = "do-not-publish"
        errors = card.validate(value, size)
        self.assertTrue(any("features has unknown fields" in error for error in errors))
        self.assertTrue(any("$.features.credential" in error for error in errors))

    def test_package_fields_reject_free_text_paths_and_urls(self):
        value, size = self.load()
        for invalid in ("credential=SUPERSECRET", "http://internal.invalid", "/private/package"):
            value["platform"]["syncPackage"] = invalid
            self.assertTrue(any("syncPackage" in error for error in card.validate(value, size)))

    def test_rpm_caret_package_identity_is_allowed(self):
        value, size = self.load()
        value["platform"]["syncPackage"] = "pi-agent-sync-1.5.0^git1-1.fc44.x86_64"
        self.assertEqual(card.validate(value, size), [])

    def test_receipt_states_are_exact_and_unique(self):
        value, size = self.load()
        value["features"]["receiptStates"] = ["prompt-delivered", "prompt-delivered"]
        self.assertTrue(any("receiptStates" in error for error in card.validate(value, size)))
        value["features"]["receiptStates"] = ["model-completed"]
        self.assertTrue(any("receiptStates" in error for error in card.validate(value, size)))
        value["features"]["receiptStates"] = [{"state": "prompt-delivered"}]
        self.assertTrue(any("receiptStates" in error for error in card.validate(value, size)))

    def test_timestamp_requires_strict_rfc3339_separator_and_timezone(self):
        value, size = self.load()
        for invalid in ("2026-08-11 07:48:00+00:00", "2026-08-11T07:48:00"):
            value["updatedAt"] = invalid
            self.assertTrue(any("updatedAt" in error for error in card.validate(value, size)))

    def test_size_and_previous_digest_are_bounded(self):
        value, _ = self.load()
        value["previousCardDigest"] = "A" * 64
        errors = card.validate(value, card.MAX_BYTES + 1)
        self.assertTrue(any("exceeds" in error for error in errors))
        self.assertTrue(any("previousCardDigest" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
