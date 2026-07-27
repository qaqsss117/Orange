from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
SCHEMA_PATH = Path("contracts/business-api/business-api.schema.v1.json")
WIRE_FIXTURE_PATH = Path("contracts/business-api/fixtures/wire-success.v1.json")
PUBLIC_FIXTURE_PATH = Path("contracts/business-api/fixtures/public-success.v1.json")
FAILURE_FIXTURE_PATH = Path("contracts/business-api/fixtures/failures.v1.json")
FIELD_MAPPING_PATH = Path("contracts/business-api/field-mapping.v1.json")
TYPESCRIPT_PATH = Path("src/businessApi.ts")

REQUIRED_OPERATIONS = (
    "config",
    "login",
    "register",
    "account",
    "subscription",
    "plans",
    "orders",
    "payment",
    "invite",
    "tickets",
    "update",
)

EXPECTED_STATUSES = {
    "AccountStatus": ("active", "disabled"),
    "SubscriptionStatus": ("none", "trial", "active", "expired", "exhausted"),
    "OrderStatus": ("pending", "paid", "cancelled", "closed", "refunded"),
    "PaymentStatus": ("unavailable", "pending", "ready", "expired"),
    "TicketStatus": ("open", "answered", "closed"),
}

EXPECTED_MAPPINGS = {
    ("config", "apiBaseUrl", None, "validate_allowlist_and_discard"),
    ("config", "paymentBaseUrl", None, "validate_allowlist_and_discard"),
    ("config", "supportUrl", None, "validate_allowlist_and_discard"),
    ("config", "bannerUrl", None, "validate_allowlist_and_discard"),
    ("login", "credentials.accessToken", None, "rust_secure_store"),
    ("login", "credentials.refreshToken", None, "rust_secure_store"),
    ("register", "credentials.accessToken", None, "rust_secure_store"),
    ("register", "credentials.refreshToken", None, "rust_secure_store"),
    ("subscription", "subscriptionCredential", None, "data_plane_only"),
    (
        "payment",
        "paymentUrl",
        "targetHost",
        "validate_and_project_host",
    ),
    ("account", "user.email", "user.email", "explicit_user_content"),
    (
        "orders",
        "order.amount.minorUnits",
        "order.amount.minorUnits",
        "integer_minor_units",
    ),
    (
        "orders",
        "order.createdAtUnixMs",
        "order.createdAtUnixMs",
        "unix_milliseconds",
    ),
}

EXPECTED_FAILURES: dict[str, tuple[dict[str, object], str]] = {
    "empty-2xx": (
        {
            "kind": "http",
            "statusCode": 204,
            "contentType": None,
            "bodyClass": "empty",
        },
        "empty_success",
    ),
    "http-4xx": (
        {
            "kind": "http",
            "statusCode": 400,
            "contentType": "application/json",
            "bodyClass": "api_error",
        },
        "request_rejected",
    ),
    "http-5xx": (
        {
            "kind": "http",
            "statusCode": 503,
            "contentType": "application/json",
            "bodyClass": "api_error",
        },
        "service_unavailable",
    ),
    "non-json": (
        {
            "kind": "http",
            "statusCode": 200,
            "contentType": "text/plain",
            "bodyClass": "non_json",
        },
        "invalid_response",
    ),
    "timeout": ({"kind": "transport", "code": "timeout"}, "timeout"),
    "schema-drift": (
        {
            "kind": "http",
            "statusCode": 200,
            "contentType": "application/json",
            "bodyClass": "schema_drift",
        },
        "schema_drift",
    ),
}

REDACTED_WIRE_VALUES = {
    "responses.config.apiBaseUrl": "<redacted:api-base-url>",
    "responses.config.paymentBaseUrl": "<redacted:payment-base-url>",
    "responses.config.supportUrl": "<redacted:support-url>",
    "responses.config.bannerUrl": "<redacted:banner-url>",
    "responses.login.credentials.accessToken": "<redacted:access-token>",
    "responses.login.credentials.refreshToken": "<redacted:refresh-token>",
    "responses.register.credentials.accessToken": "<redacted:access-token>",
    "responses.register.credentials.refreshToken": "<redacted:refresh-token>",
    "responses.subscription.subscriptionCredential": (
        "<redacted:subscription-credential>"
    ),
    "responses.payment.paymentUrl": "<redacted:payment-url>",
    "responses.orders.order.orderId": "<redacted:order-id>",
    "responses.payment.orderId": "<redacted:order-id>",
    "responses.invite.inviteCode": "<redacted:invite-code>",
}

FORBIDDEN_PUBLIC_KEYS = {
    "accessToken",
    "refreshToken",
    "credentials",
    "subscriptionCredential",
    "paymentUrl",
    "password",
}

FORBIDDEN_TYPESCRIPT_FIELDS = (
    "accessToken",
    "refreshToken",
    "subscriptionCredential",
    "paymentUrl",
    "password",
)


def load_json_object(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path.as_posix()} must contain a JSON object")
    return value


def schema_violations(schema: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if schema.get("schemaVersion") != 1:
        errors.append("business API schemaVersion must be 1")
    if schema.get("environment") != "development":
        errors.append("business API schema must remain development-only")
    if schema.get("releaseAllowed") is not False:
        errors.append("business API clean-room schema must not be release-enabled")
    if schema.get("unknownFieldPolicy") != {
        "objects": "reject",
        "statusValues": "map_to_unknown",
    }:
        errors.append("business API unknown-field policy drifted")
    if schema.get("units") != {
        "timestamp": "unix_milliseconds",
        "money": "integer_minor_units",
        "traffic": "bytes",
        "integerMaximum": 9_007_199_254_740_991,
    }:
        errors.append("business API unit policy drifted")

    operations = schema.get("x-orange-operations")
    if not isinstance(operations, list):
        errors.append("business API operation registry must be an array")
    else:
        operation_names = [
            operation.get("name") if isinstance(operation, dict) else None
            for operation in operations
        ]
        if operation_names != list(REQUIRED_OPERATIONS):
            errors.append("business API operation registry is incomplete or reordered")
        for operation in operations:
            if not isinstance(operation, dict) or set(operation) != {
                "name",
                "request",
                "wireResponse",
                "publicResponse",
            }:
                errors.append("business API operation entry is not structurally closed")
                continue
            for field in ("wireResponse", "publicResponse"):
                reference = operation.get(field)
                if not isinstance(reference, str) or not reference.startswith("#/$defs/"):
                    errors.append(f"business API operation has invalid {field} reference")

    definitions = schema.get("$defs")
    if not isinstance(definitions, dict):
        return errors + ["business API schema lacks $defs"]

    for name, definition in definitions.items():
        if not isinstance(definition, dict):
            errors.append(f"business API definition {name} must be an object")
            continue
        if definition.get("type") == "object":
            properties = definition.get("properties")
            required = definition.get("required")
            if definition.get("additionalProperties") is not False:
                errors.append(f"business API object {name} is not closed")
            if not isinstance(properties, dict) or not isinstance(required, list):
                errors.append(f"business API object {name} lacks explicit properties/required")
            elif set(properties) != set(required):
                errors.append(f"business API object {name} has implicit optional fields")

    for name, known_values in EXPECTED_STATUSES.items():
        definition = definitions.get(name)
        if not isinstance(definition, dict):
            errors.append(f"business API status definition {name} is missing")
            continue
        if definition.get("x-knownValues") != list(known_values):
            errors.append(f"business API status registry {name} drifted")
        if definition.get("x-unknownValueMapping") != "unknown":
            errors.append(f"business API status {name} must map unknown values")

    safe_integer = definitions.get("SafeInteger")
    if not isinstance(safe_integer, dict) or safe_integer.get("minimum") != 0 or safe_integer.get(
        "maximum"
    ) != 9_007_199_254_740_991:
        errors.append("business API safe-integer boundary drifted")
    currency = definitions.get("CurrencyCode")
    if not isinstance(currency, dict) or currency.get("pattern") != "^[A-Z]{3}$":
        errors.append("business API currency format drifted")
    return errors


def field_mapping_violations(document: dict[str, Any]) -> list[str]:
    if document.get("schemaVersion") != 1:
        return ["business API field mapping schemaVersion must be 1"]
    mappings = document.get("mappings")
    if not isinstance(mappings, list):
        return ["business API field mappings must be an array"]
    actual: set[tuple[object, object, object, object]] = set()
    errors: list[str] = []
    for mapping in mappings:
        if not isinstance(mapping, dict) or set(mapping) != {
            "operation",
            "wireField",
            "publicField",
            "policy",
        }:
            errors.append("business API field mapping entry is not structurally closed")
            continue
        operation = mapping.get("operation")
        wire_field = mapping.get("wireField")
        public_field = mapping.get("publicField")
        policy = mapping.get("policy")
        if (
            not isinstance(operation, str)
            or not isinstance(wire_field, str)
            or (public_field is not None and not isinstance(public_field, str))
            or not isinstance(policy, str)
        ):
            errors.append("business API field mapping values must be bounded strings or null")
            continue
        actual.add((operation, wire_field, public_field, policy))
    if actual != EXPECTED_MAPPINGS:
        errors.append("business API sensitive/public field mapping drifted")
    return errors


def failure_fixture_violations(document: dict[str, Any]) -> list[str]:
    if document.get("schemaVersion") != 1:
        return ["business API failure fixture schemaVersion must be 1"]
    cases = document.get("cases")
    if not isinstance(cases, list):
        return ["business API failure fixture cases must be an array"]
    actual: dict[str, tuple[object, object]] = {}
    errors: list[str] = []
    for case in cases:
        if not isinstance(case, dict) or set(case) != {"name", "source", "expected"}:
            errors.append("business API failure case is not structurally closed")
            continue
        name = case.get("name")
        if not isinstance(name, str) or name in actual:
            errors.append("business API failure case name is invalid or duplicated")
            continue
        actual[name] = (case.get("source"), case.get("expected"))
    if actual != EXPECTED_FAILURES:
        errors.append("business API failure matrix drifted")
    return errors


def dotted_value(document: dict[str, Any], path: str) -> object:
    value: object = document
    for part in path.split("."):
        if not isinstance(value, dict) or part not in value:
            return None
        value = value[part]
    return value


def walk_json(value: object, path: str = "") -> list[tuple[str, str | None, object]]:
    entries: list[tuple[str, str | None, object]] = []
    if isinstance(value, dict):
        for key, child in value.items():
            child_path = f"{path}.{key}" if path else key
            entries.append((child_path, key, child))
            entries.extend(walk_json(child, child_path))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            child_path = f"{path}[{index}]"
            entries.append((child_path, None, child))
            entries.extend(walk_json(child, child_path))
    return entries


def fixture_violations(
    wire: dict[str, Any], public: dict[str, Any]
) -> list[str]:
    errors: list[str] = []
    expected_top_keys = {"schemaVersion", "environment", "responses"}
    for name, fixture in (("wire", wire), ("public", public)):
        if set(fixture) != expected_top_keys:
            errors.append(f"business API {name} fixture top-level object is not closed")
        if fixture.get("schemaVersion") != 1 or fixture.get("environment") != "development":
            errors.append(f"business API {name} fixture metadata drifted")
        responses = fixture.get("responses")
        if not isinstance(responses, dict) or tuple(responses) != REQUIRED_OPERATIONS:
            errors.append(f"business API {name} fixture operation coverage drifted")

    for path, expected in REDACTED_WIRE_VALUES.items():
        if dotted_value(wire, path) != expected:
            errors.append(f"business API wire fixture does not redact {path}")

    for name, fixture in (("wire", wire), ("public", public)):
        for path, key, value in walk_json(fixture):
            if isinstance(value, str) and re.search(r"https?://", value, re.IGNORECASE):
                errors.append(f"business API {name} fixture contains URL at {path}")
            if isinstance(value, str) and "@" in value and not value.lower().endswith(
                ".invalid"
            ):
                errors.append(f"business API {name} fixture contains non-reserved email at {path}")
            if name == "public" and key in FORBIDDEN_PUBLIC_KEYS:
                errors.append(f"business API public fixture exposes sensitive field {path}")
    return errors


def typescript_violations(source: str) -> list[str]:
    errors: list[str] = []
    if re.search(r"\bany\b", source):
        errors.append("business API TypeScript contract uses any")
    if re.search(
        r"export\s+type\s+\w+\s*=[^;]{0,240}\b(?:Record|Map)\s*<",
        source,
        re.MULTILINE,
    ):
        errors.append("business API TypeScript DTO exports an unbounded map")
    if re.search(r"\[\s*\w+\s*:\s*(?:string|number)\s*\]\s*:", source):
        errors.append("business API TypeScript contract declares an index signature")
    for field in FORBIDDEN_TYPESCRIPT_FIELDS:
        if re.search(re.escape(field), source, re.IGNORECASE):
            errors.append(f"business API TypeScript contract names sensitive field {field}")
    if re.search(r"\b(?:fetch|XMLHttpRequest|WebSocket)\b|\baxios\b", source):
        errors.append("business API TypeScript contract contains direct networking")
    for guard in (
        "actualKeys.length !== keys.length",
        "actualKeys.every((key) => keys.includes(key))",
        "Number.isSafeInteger(value)",
        'return knownValues.includes(value as T) ? (value as T) : "unknown";',
    ):
        if guard not in source:
            errors.append(f"business API TypeScript parser lacks strict guard: {guard}")
    return errors


def audit(root: Path) -> dict[str, Any]:
    schema = load_json_object(root / SCHEMA_PATH)
    wire = load_json_object(root / WIRE_FIXTURE_PATH)
    public = load_json_object(root / PUBLIC_FIXTURE_PATH)
    failures = load_json_object(root / FAILURE_FIXTURE_PATH)
    mappings = load_json_object(root / FIELD_MAPPING_PATH)
    typescript = (root / TYPESCRIPT_PATH).read_text(encoding="utf-8")

    errors = schema_violations(schema)
    errors.extend(field_mapping_violations(mappings))
    errors.extend(failure_fixture_violations(failures))
    errors.extend(fixture_violations(wire, public))
    errors.extend(typescript_violations(typescript))
    return {
        "schema_version": 1,
        "passed": not errors,
        "environment": schema.get("environment"),
        "release_allowed": schema.get("releaseAllowed"),
        "operation_count": len(schema.get("x-orange-operations", [])),
        "failure_case_count": len(failures.get("cases", [])),
        "field_mapping_count": len(mappings.get("mappings", [])),
        "schema": SCHEMA_PATH.as_posix(),
        "wire_fixture": WIRE_FIXTURE_PATH.as_posix(),
        "public_fixture": PUBLIC_FIXTURE_PATH.as_posix(),
        "typescript_contract": TYPESCRIPT_PATH.as_posix(),
        "errors": sorted(set(errors)),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit the Orange business API contract")
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/business-api-contract.json",
    )
    arguments = parser.parse_args()
    report = audit(ROOT)
    report_path = arguments.report if arguments.report.is_absolute() else ROOT / arguments.report
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (json.JSONDecodeError, OSError, ValueError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
