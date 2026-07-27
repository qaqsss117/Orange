from __future__ import annotations

import copy
import importlib.util
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_business_api_contract.py"
SPEC = importlib.util.spec_from_file_location("check_business_api_contract", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
ROOT = Path(__file__).resolve().parents[3]


class BusinessApiContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.schema = CHECKER.load_json_object(ROOT / CHECKER.SCHEMA_PATH)
        cls.wire = CHECKER.load_json_object(ROOT / CHECKER.WIRE_FIXTURE_PATH)
        cls.public = CHECKER.load_json_object(ROOT / CHECKER.PUBLIC_FIXTURE_PATH)
        cls.failures = CHECKER.load_json_object(ROOT / CHECKER.FAILURE_FIXTURE_PATH)
        cls.mappings = CHECKER.load_json_object(ROOT / CHECKER.FIELD_MAPPING_PATH)
        cls.typescript = (ROOT / CHECKER.TYPESCRIPT_PATH).read_text(encoding="utf-8")

    def test_repository_contract_passes_every_audit(self) -> None:
        self.assertEqual(CHECKER.schema_violations(self.schema), [])
        self.assertEqual(CHECKER.field_mapping_violations(self.mappings), [])
        self.assertEqual(CHECKER.failure_fixture_violations(self.failures), [])
        self.assertEqual(CHECKER.fixture_violations(self.wire, self.public), [])
        self.assertEqual(CHECKER.typescript_violations(self.typescript), [])
        self.assertTrue(CHECKER.audit(ROOT)["passed"])

    def test_schema_rejects_missing_operations_and_open_objects(self) -> None:
        schema = copy.deepcopy(self.schema)
        schema["x-orange-operations"].pop()
        schema["$defs"]["Money"]["additionalProperties"] = True
        errors = CHECKER.schema_violations(schema)
        self.assertTrue(any("operation registry" in error for error in errors))
        self.assertTrue(any("Money is not closed" in error for error in errors))

    def test_failure_matrix_is_exact(self) -> None:
        failures = copy.deepcopy(self.failures)
        failures["cases"][0]["expected"] = "invalid_response"
        self.assertTrue(
            any(
                "failure matrix drifted" in error
                for error in CHECKER.failure_fixture_violations(failures)
            )
        )

    def test_fixture_rejects_raw_secrets_urls_and_real_emails(self) -> None:
        wire = copy.deepcopy(self.wire)
        public = copy.deepcopy(self.public)
        wire["responses"]["login"]["credentials"]["accessToken"] = "raw-secret"
        public["responses"]["account"]["user"]["email"] = "member@example.com"
        public["responses"]["config"]["notice"] = "https://example.invalid/notice"
        public["responses"]["payment"]["paymentUrl"] = "hidden"
        errors = CHECKER.fixture_violations(wire, public)
        self.assertTrue(any("does not redact" in error for error in errors))
        self.assertTrue(any("non-reserved email" in error for error in errors))
        self.assertTrue(any("contains URL" in error for error in errors))
        self.assertTrue(any("sensitive field" in error for error in errors))

    def test_typescript_rejects_sensitive_and_unbounded_public_dtos(self) -> None:
        unsafe = (
            self.typescript
            + "\nexport type UnsafeBag = Record<string, any>;"
            + "\nexport interface UnsafeSecret { accessToken: string; }"
            + "\nfetch('https://api.orange.invalid');\n"
        )
        errors = CHECKER.typescript_violations(unsafe)
        self.assertTrue(any("uses any" in error for error in errors))
        self.assertTrue(any("unbounded map" in error for error in errors))
        self.assertTrue(any("accessToken" in error for error in errors))
        self.assertTrue(any("direct networking" in error for error in errors))

    def test_sensitive_field_mapping_is_exact(self) -> None:
        mappings = copy.deepcopy(self.mappings)
        mappings["mappings"][0]["publicField"] = "unsafe"
        self.assertTrue(
            any(
                "field mapping drifted" in error
                for error in CHECKER.field_mapping_violations(mappings)
            )
        )

        mappings["mappings"][0]["publicField"] = {"unbounded": True}
        self.assertTrue(
            any(
                "bounded strings or null" in error
                for error in CHECKER.field_mapping_violations(mappings)
            )
        )


if __name__ == "__main__":
    unittest.main()
