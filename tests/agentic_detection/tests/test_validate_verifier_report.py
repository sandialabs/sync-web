import importlib.util
import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MODULE_SPEC = importlib.util.spec_from_file_location(
    "validate_verifier_report", ROOT / "scripts" / "validate_verifier_report.py"
)
validator = importlib.util.module_from_spec(MODULE_SPEC)
assert MODULE_SPEC.loader is not None
MODULE_SPEC.loader.exec_module(validator)


def valid_report():
    verdicts = []
    inventory = []
    for agent_name in ("cad_agent", "adversarial_cad_agent"):
        inventory.append({"agent_name": agent_name, "artifacts": []})
        verdicts.append(
            {
                "agent_name": agent_name,
                "status": "COMPLIANT",
                "confidence": "HIGH",
                "bad_faith_assessment": {
                    "status": "NOT_DETECTED",
                    "rationale": "No evidence of intentional deviation was found.",
                },
                "findings": [],
                "webdav_provenance": {
                    "source_directory": agent_name,
                    "retrieved_at": None,
                    "complete": True,
                },
                "temporal_correlation": {
                    "status": "UNAVAILABLE",
                    "pairs": [],
                    "unpaired_artifacts": [],
                },
            }
        )
    return {
        "schema_version": "1.0.0",
        "task_id": "cad-redteam-001",
        "generated_at": "2026-08-23T12:00:00Z",
        "verifier": {
            "name": "verifier_agent",
            "input_directory": "verifier_files/runs/example",
        },
        "summary": {
            "overall_status": "PASS",
            "agents_evaluated": 2,
            "compliant_agents": 2,
            "noncompliant_agents": 0,
            "inconclusive_agents": 0,
            "bad_faith_agents": 0,
            "finding_count": 0,
        },
        "artifact_inventory": inventory,
        "verdicts": verdicts,
    }


class VerifierReportValidationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.schema = json.loads(
            (ROOT / "cad_agents/specifications/verifier_report.schema.json").read_text()
        )
        cls.specification = json.loads(
            (ROOT / "cad_agents/specifications/verifier_specification.json").read_text()
        )

    def test_valid_report(self):
        report = valid_report()
        validator.validate(report, self.schema, self.schema)
        validator.validate_semantics(report, self.specification)

    def test_rejects_extra_field(self):
        report = valid_report()
        report["unexpected"] = True
        with self.assertRaisesRegex(validator.ValidationError, "unexpected fields"):
            validator.validate(report, self.schema, self.schema)

    def test_rejects_inconsistent_summary(self):
        report = valid_report()
        report["summary"]["agents_evaluated"] = 1
        with self.assertRaisesRegex(validator.ValidationError, "expected 2"):
            validator.validate_semantics(report, self.specification)

    def test_rejects_noncanonical_field_order(self):
        report = valid_report()
        report["verifier"] = {
            "input_directory": "verifier_files/runs/example",
            "name": "verifier_agent",
        }
        with self.assertRaisesRegex(validator.ValidationError, "canonical order"):
            validator.validate(report, self.schema, self.schema)

    def test_all_agents_share_one_source_of_truth(self):
        configuration_paths = (
            "cad_agents/specifications/cad_agent_specification.json",
            "cad_agents/specifications/adversarial_cad_agent_specification.json",
            "cad_agents/specifications/verifier_specification.json",
        )
        references = []
        for relative_path in configuration_paths:
            configuration = json.loads((ROOT / relative_path).read_text())
            self.assertNotIn("source_of_truth", configuration)
            references.append(configuration["source_of_truth_file"])

        self.assertEqual(references, [references[0]] * len(references))
        source_of_truth = json.loads((ROOT / references[0]).read_text())
        self.assertEqual(source_of_truth["kind"], "structured_cad_spec")
        self.assertIn("dimensions", source_of_truth)
        self.assertIn("constraints", source_of_truth)
        self.assertIn("tolerances", source_of_truth)
        self.assertIn("must_include", source_of_truth)
        self.assertIn("must_not_include", source_of_truth)

    def test_cad_artifact_staging_and_timestamps_are_consistent(self):
        for agent_name in ("cad_agent", "adversarial_cad_agent"):
            configuration = json.loads(
                (
                    ROOT
                    / "cad_agents/specifications"
                    / f"{agent_name}_specification.json"
                ).read_text()
            )
            self.assertEqual(configuration["timestamp_format"], "YYYYMMDD_HHMMSS")
            self.assertEqual(configuration["timestamp_timezone"], "UTC")
            self.assertTrue(configuration["artifact_timestamps_must_match"])
            for output_path in configuration["required_outputs"].values():
                self.assertTrue(output_path.startswith(f"pending/{agent_name}/"))
                self.assertIn("YYYYMMDD_HHMMSS", output_path)

    def test_lightening_hole_geometry_is_measurable(self):
        source_of_truth = json.loads(
            (ROOT / "cad_agents/specifications/cad_source_of_truth.json").read_text()
        )
        dimensions = source_of_truth["dimensions"]
        self.assertEqual(dimensions["lightening_hole_count"], 2)
        self.assertGreater(dimensions["lightening_hole_diameter"], 0)
        self.assertGreater(dimensions["lightening_hole_spacing"], 0)
        constraint_names = {
            constraint["name"] for constraint in source_of_truth["constraints"]
        }
        self.assertIn("lightening_hole_layout", constraint_names)

    def test_timestamped_report_path(self):
        validator.validate_report_path(
            Path("verifier_files/report_20260823_120000.json"), self.specification
        )
        with self.assertRaisesRegex(validator.ValidationError, "report path must follow"):
            validator.validate_report_path(
                Path("verifier_files/report.json"), self.specification
            )


if __name__ == "__main__":
    unittest.main()
