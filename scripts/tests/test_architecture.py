"""Negative boundary fixtures: a green gate must actually reject architectural drift."""
import copy
from datetime import date
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    "architecture", Path(__file__).resolve().parents[1] / "check-architecture.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
TODAY = date(2026, 9, 27)


class ArchitectureGateTests(unittest.TestCase):
    def setUp(self):
        root = Path("/fixture").resolve()
        self.metadata = {"workspace_root": str(root), "workspace_members": ["core", "native"],
                         "packages": [
                             {"id": name, "name": name,
                              "manifest_path": str(root / name / "Cargo.toml"), "dependencies": []}
                             for name in ("core", "native")]}
        self.policy = {"schema_version": 1, "modules": {
            name: {"path": name, "layer": name, "owner": "team", "responsibility": "boundary",
                   "dependencies": {"normal": [], "dev": [], "build": []}}
            for name in ("core", "native")}, "exceptions": []}

    def edge(self, source="core", target="native", kind=None, **extra):
        package = next(p for p in self.metadata["packages"] if p["name"] == source)
        package["dependencies"].append({"name": target, "kind": kind,
                                        "path": str(Path("/fixture") / target), **extra})

    def errors(self):
        return CHECK.violations(self.metadata, self.policy, TODAY)

    def test_valid_baseline(self):
        self.assertEqual(self.errors(), [])

    def test_unknown_module(self):
        del self.policy["modules"]["native"]
        self.assertIn("Unreviewed modules", " ".join(self.errors()))

    def test_renamed_optional_target_dependency_cannot_bypass(self):
        self.edge(rename="innocent_alias", optional=True, target="native")
        self.metadata["packages"][0]["dependencies"][0]["target"] = 'cfg(windows)'
        self.assertIn("Unreviewed dependency", " ".join(self.errors()))

    def test_dev_allowance_does_not_authorize_production(self):
        self.policy["modules"]["core"]["dependencies"]["dev"] = ["native"]
        self.edge(kind="dev")
        self.assertEqual(self.errors(), [])
        self.edge()
        self.assertIn("Unreviewed dependency", " ".join(self.errors()))

    def test_build_dependencies_checked(self):
        self.edge(kind="build")
        self.assertIn("--build-->", " ".join(self.errors()))

    def test_cycles_checked_even_if_both_edges_allowed(self):
        self.edge()
        self.edge("native", "core")
        self.policy["modules"]["core"]["dependencies"]["normal"] = ["native"]
        self.policy["modules"]["native"]["dependencies"]["normal"] = ["core"]
        self.assertIn("Production dependency cycle", " ".join(self.errors()))

    def test_external_dependency_requires_review(self):
        self.edge(target="unreviewed_renderer", path=None)
        self.assertIn("Unreviewed dependency", " ".join(self.errors()))

    def test_workspace_replacement_rejected(self):
        self.edge(path=None)
        self.policy["modules"]["core"]["dependencies"]["normal"] = ["native"]
        self.assertIn("another source", " ".join(self.errors()))

    def test_unregistered_path_dependency_rejected_even_if_allowed(self):
        self.edge(target="hidden")
        self.policy["modules"]["core"]["dependencies"]["normal"] = ["hidden"]
        self.assertIn("outside reviewed workspace", " ".join(self.errors()))

    def test_exception_expiry_and_cleanup(self):
        self.edge()
        exception = {"id": "debt-1", "from": "core", "to": "native", "kind": "normal",
                     "owner": "team", "reason": "migration", "remove_when": "port injection",
                     "review_by": "2026-09-27"}
        self.policy["exceptions"] = [exception]
        self.assertEqual(self.errors(), [])
        exception["review_by"] = "2026-09-26"
        self.assertIn("Expired exception", " ".join(self.errors()))
        self.metadata["packages"][0]["dependencies"] = []
        self.assertIn("Remove resolved exception", " ".join(self.errors()))

    def test_exception_cannot_be_laundered_into_permanent_allowance(self):
        self.edge()
        self.policy["modules"]["core"]["dependencies"]["normal"] = ["native"]
        self.policy["exceptions"] = [{"id": "debt-1", "from": "core", "to": "native",
                                     "kind": "normal", "owner": "team", "reason": "migration",
                                     "remove_when": "ports", "review_by": "2026-10-01"}]
        self.assertIn("also permanently allowed", " ".join(self.errors()))

    def test_stale_allowances_must_be_removed(self):
        self.policy["modules"]["core"]["dependencies"]["normal"] = ["native"]
        self.assertIn("Remove unused allowance", " ".join(self.errors()))

    def test_module_moves_require_review(self):
        self.policy["modules"]["core"]["path"] = "elsewhere"
        self.assertIn("Module moved without review", " ".join(self.errors()))

    def test_checker_does_not_mutate_inputs(self):
        before = copy.deepcopy((self.metadata, self.policy))
        self.errors()
        self.assertEqual(before, (self.metadata, self.policy))


if __name__ == "__main__":
    unittest.main()
