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

    def test_fixture_identity_provider_keeps_supported_password_policy(self):
        environment = generate.rewrite_service_environment(
            "identity-provider", 1, {}, "secret", 2, 8, "admin", "password"
        )
        self.assertNotIn("KRATOS_PASSWORD_MIN_LENGTH", environment)

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
