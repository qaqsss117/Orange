from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_supply_chain.py"
SPEC = importlib.util.spec_from_file_location("check_supply_chain", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class SupplyChainTests(unittest.TestCase):
    def test_denylist_matches_only_dependency_boundaries(self) -> None:
        names = ["safe-clashing-name", "vendor/mihomo-core", "react"]
        patterns = [r"(^|[-_./])clash($|[-_./])", r"(^|[-_./])mihomo($|[-_./])"]
        self.assertEqual(CHECKER.denied_dependencies(names, patterns), ["vendor/mihomo-core"])

    def test_pnpm_package_names_are_parsed(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        path = Path(temporary.name) / "pnpm-lock.yaml"
        path.write_text(
            "lockfileVersion: '9.0'\npackages:\n\n  '@scope/pkg@1.2.3':\n"
            "    resolution: {}\n\n  react@19.0.0:\n    resolution: {}\n\nsnapshots:\n",
            encoding="utf-8",
        )
        self.assertEqual(CHECKER.pnpm_package_names(path), ["@scope/pkg", "react"])

    def test_configured_urls_report_source_file(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "config.toml").write_text('registry = "https://example.invalid/index"\n', encoding="utf-8")
        self.assertEqual(
            CHECKER.configured_urls(root, ["*.toml"]),
            [("config.toml", "https://example.invalid/index")],
        )

    def test_sbom_component_names_are_parsed(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        path = Path(temporary.name) / "sbom.json"
        path.write_text(
            json.dumps({"bomFormat": "CycloneDX", "components": [{"name": "react"}]}),
            encoding="utf-8",
        )
        self.assertEqual(CHECKER.sbom_package_names(path), ["react"])

    def test_hashed_pypi_requirements_are_parsed(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        path = Path(temporary.name) / "requirements.txt"
        digest = "a" * 64
        path.write_text(f"tomli==2.2.1 --hash=sha256:{digest}\n", encoding="utf-8")
        self.assertEqual(CHECKER.pypi_requirements(path), [("tomli", "2.2.1", digest)])

    def test_unhashed_pypi_requirement_is_rejected(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        path = Path(temporary.name) / "requirements.txt"
        path.write_text("tomli==2.2.1\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "invalid hashed Python requirement"):
            CHECKER.pypi_requirements(path)

    def test_every_required_ecosystem_must_be_covered(self) -> None:
        policy = {
            "required_dependency_ecosystems": ["cargo", "npm", "go"],
            "dependency_lockfiles": {"cargo": "Cargo.lock"},
            "dependency_systems_without_packages": {"go": "not introduced"},
        }
        self.assertEqual(
            CHECKER.validate_ecosystem_coverage(policy),
            ["dependency ecosystems lack lockfile or empty reason: npm"],
        )


if __name__ == "__main__":
    unittest.main()
