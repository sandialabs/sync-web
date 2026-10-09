from datetime import datetime, timezone
import unittest
from unittest.mock import Mock, patch

from journal_cli import message


class MessageTimestampTests(unittest.TestCase):
    def config(self):
        return {
            "owner": "alice", "localJournal": "alice",
            "recipients": [{"identity": name, "journal": name, "owner": name, "route": [name]}
                           for name in ("bob", "carol")],
        }

    def test_direct_always_uses_canonical_milliseconds(self):
        for microsecond, fraction in [(123456, "123"), (0, "000")]:
            with self.subTest(microsecond=microsecond), \
                 patch.object(message, "load_message_config", return_value=self.config()), \
                 patch.object(message, "_send") as send, patch.object(message, "datetime") as clock:
                clock.now.return_value = datetime(2026, 1, 2, 3, 4, 5, microsecond, timezone.utc)
                envelope = message.send(Mock(), "bob", "hello")
                expected = f"2026-01-02T03:04:05.{fraction}Z"
                self.assertEqual(envelope["createdAt"], expected)
                self.assertEqual(send.call_args.args[3]["createdAt"], expected)

    def test_group_copies_share_canonical_millisecond_timestamp(self):
        with patch.object(message, "load_message_config", return_value=self.config()), \
             patch.object(message, "_send") as send, patch.object(message, "datetime") as clock:
            clock.now.return_value = datetime(2026, 1, 2, 3, 4, 5, 123456, timezone.utc)
            result = message.send_group(Mock(), ["bob", "carol"], "hello", "223e4567-e89b-42d3-a456-426614174000")
            self.assertEqual(result["createdAt"], "2026-01-02T03:04:05.123Z")
            self.assertEqual([call.args[3]["createdAt"] for call in send.call_args_list],
                             [result["createdAt"], result["createdAt"]])


if __name__ == "__main__":
    unittest.main()
