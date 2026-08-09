import unittest

import aggregate_results


class AggregateResultsTests(unittest.TestCase):
    def test_aggregate_preserves_requests_and_adds_logical_path_operations(self):
        snapshots = [
            {
                "node_name": "journal-0",
                "requests_total": 10,
                "requests_failed_total": 0,
                "requests_succeeded_total": 10,
                "activity_cycles_total": 3,
                "activity_requests_total": 6,
                "activity_requests_success_total": 6,
                "activity_path_operations_total": 24,
                "activity_path_operations_success_total": 24,
            },
            {
                "node_name": "journal-1",
                "requests_total": 8,
                "requests_failed_total": 0,
                "requests_succeeded_total": 8,
                "activity_cycles_total": 2,
                "activity_requests_total": 4,
                "activity_requests_success_total": 4,
                "activity_path_operations_total": 16,
                "activity_path_operations_success_total": 16,
            },
        ]
        previous = {
            "activity_requests_total": 6,
            "activity_requests_success_total": 6,
            "activity_path_operations_total": 24,
            "activity_path_operations_success_total": 24,
        }
        result = aggregate_results.aggregate_snapshots(
            snapshots, 100.0, previous=previous
        )
        self.assertEqual(result["activity_requests_total"], 10)
        self.assertEqual(result["activity_path_operations_total"], 40)
        self.assertEqual(result["activity_path_operations_success_total"], 40)
        self.assertEqual(result["activity_path_operation_success_rate"], 100.0)

    def test_windowed_rates_keep_http_requests_separate_from_path_operations(self):
        state = aggregate_results.AggregationState(10, throughput_window_seconds=8)
        first = aggregate_results.aggregate_snapshots([], 100.0)
        first.update({
            "activity_requests_success_total": 10,
            "activity_path_operations_success_total": 40,
        })
        state.update(first)
        second = aggregate_results.aggregate_snapshots([], 102.0, previous=first)
        second.update({
            "activity_requests_success_total": 14,
            "activity_path_operations_success_total": 64,
        })
        state.update(second)
        self.assertEqual(second["activity_requests_per_second"], 2.0)
        self.assertEqual(second["activity_path_operations_per_second"], 12.0)
        self.assertEqual(
            state.history()[-1]["activity_path_operations_per_second"], 12.0
        )

    def test_dashboard_leads_with_logical_paths_and_keeps_request_rate(self):
        html = aggregate_results.build_dashboard_html()
        self.assertIn("Logical Path Throughput", html)
        self.assertIn("path-op/s", html)
        self.assertIn("req/s", html)
        self.assertIn("activity_path_operations_per_second", html)


if __name__ == "__main__":
    unittest.main()
