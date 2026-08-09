import unittest

import generate


class ProjectScopedArtifactsTest(unittest.TestCase):
    def test_social_agent_uses_project_scoped_runtime_artifacts(self):
        service = generate.make_social_agent_service(
            2,
            period=4,
            size=16,
            activity=4,
            users=2,
            segments=2,
            words=8,
            clients=1,
            admin_username="admin",
            admin_password="password",
            run_path="runs/sync-test",
        )

        self.assertEqual(service["environment"]["USERS"], "2")
        self.assertEqual(service["environment"]["SEGMENTS"], "2")
        self.assertEqual(service["environment"]["ACTIVITY_DISABLED"], "0")
        self.assertNotIn("BATCH", service["environment"])
        self.assertEqual(service["networks"], ["public", "private-2"])
        self.assertIn(
            "./runs/sync-test/peers.json:/srv/peers.json:ro,z",
            service["volumes"],
        )
        self.assertIn(
            "./runs/sync-test/metrics/social-agent-2:/var/lib/node_exporter/textfile:Z",
            service["volumes"],
        )
        self.assertIn(
            "./runs/sync-test/results/social-agent-2:/srv/results:Z",
            service["volumes"],
        )

    def test_social_agent_passes_optional_batch_only_when_configured(self):
        service = generate.make_social_agent_service(
            0, 2, 8, 4, 1, 2, 8, 1, "admin", "password",
            "runs/sync-test", batch=4,
        )
        self.assertEqual(service["environment"]["BATCH"], "4")

    def test_batch_validation_uses_access_group_capacity_not_only_size(self):
        generate.validate_batch(8, None)
        generate.validate_batch(8, 4)
        generate.validate_batch(1, 1)
        for size, batch in ((8, 5), (0, 1), (8, 0), (4096, 1025)):
            with self.assertRaises(SystemExit):
                generate.validate_batch(size, batch)

    def test_social_agent_supports_explicit_setup_only_mode(self):
        service = generate.make_social_agent_service(
            0, 2, 4, 0, 1, 2, 8, 1, "admin", "password",
            "runs/sync-test", activity_disabled="1",
        )
        self.assertEqual(service["environment"]["ACTIVITY"], "0")
        self.assertEqual(service["environment"]["ACTIVITY_DISABLED"], "1")

    def test_fixture_identity_provider_keeps_supported_password_policy(self):
        environment = generate.rewrite_service_environment(
            "identity-provider", 1, {}, "root", "interface", 2, 8, "admin", "password"
        )
        self.assertNotIn("KRATOS_PASSWORD_MIN_LENGTH", environment)

    def test_journal_and_gateway_receive_only_their_required_credentials(self):
        journal = generate.rewrite_service_environment(
            "journal", 1, {}, "root", "interface", 2, 8, "admin", "password"
        )
        gateway = generate.rewrite_service_environment(
            "gateway", 1, {}, "root", "interface", 2, 8, "admin", "password"
        )
        self.assertEqual(journal["SECRET"], "root")
        self.assertEqual(journal["INTERFACE_SECRET"], "interface")
        self.assertEqual(gateway["JOURNAL_SECRET"], "interface")
        self.assertNotIn("SECRET", gateway)

    def test_ui_healthchecks_use_bundle_independent_endpoint(self):
        original = {
            "test": ["CMD-SHELL", "wget http://127.0.0.1/"],
            "interval": "10s",
        }
        expected = [
            "CMD-SHELL",
            "wget -q -O- http://127.0.0.1/healthz >/dev/null",
        ]

        for service in ("explorer", "workbench"):
            healthcheck = generate.rewrite_service_healthcheck(service, original)
            self.assertEqual(healthcheck["test"], expected)
            self.assertEqual(healthcheck["interval"], "10s")
        self.assertEqual(original["test"][1], "wget http://127.0.0.1/")

    def test_aggregate_reads_only_the_selected_project_results(self):
        service = generate.make_aggregate_results_service("runs/sync-test")

        self.assertEqual(
            service["volumes"],
            [
                "./aggregate_results.py:/workspace/aggregate_results.py:ro,z",
                "./runs/sync-test/results:/workspace/results:z",
            ],
        )


if __name__ == "__main__":
    unittest.main()
