#!/usr/bin/env python3
"""Audit Hurl integration tests against the TAMS OpenAPI spec.

Checks two things:
1. Operation coverage: which spec operations have corresponding Hurl requests
2. Assertion coverage: which required response fields are asserted on

Run: python scripts/check-hurl-coverage.py
"""

import re
import sys
from pathlib import Path

import yaml

SPEC_PATH = Path("tams-server/swagger-ui/api-spec.yaml")
HURL_FILES = list(Path("tests").glob("*.hurl"))

# Map from test resource IDs to spec path params
NORMALISE = [
    (r"hurl-flow-\d+|hurl-err-\d+", "{flowId}"),
    (r"hurl-src-\d+|hurl-err-src-\d+", "{sourceId}"),
    (r"hurl-wh-\d+|nonexistent-webhook", "{webhookId}"),
    (r"nonexistent-object", "{objectId}"),
    (r"nonexistent-source", "{sourceId}"),
    (r"nonexistent-flow|nonexistent(?=/)", "{flowId}"),
    (r"/tags/[a-z-]+", "/tags/{name}"),
    (r"\{\{object_id\d*\}\}", "{objectId}"),
    (r"\{\{webhook_id\d*\}\}", "{webhookId}"),
    (r"\{\{delete_request_id\}\}", "{request-id}"),
]


def normalise_path(raw_path):
    path = re.sub(r"\?.*", "", raw_path)
    for pattern, replacement in NORMALISE:
        path = re.sub(pattern, replacement, path)
    return path


def parse_hurl_files():
    """Extract (method, path) and jsonpath assertions from Hurl files."""
    ops = set()
    # Map (method, normalised_path) -> set of asserted jsonpath fields
    assertions = {}

    for hurl_file in HURL_FILES:
        current_op = None
        with open(hurl_file) as f:
            for line in f:
                line = line.strip()
                m = re.match(
                    r"^(GET|POST|PUT|DELETE|HEAD)\s+http://localhost:5800(/\S*)", line
                )
                if m:
                    method, raw_path = m.group(1), m.group(2)
                    norm = normalise_path(raw_path)
                    ops.add((method, norm))
                    current_op = (method, norm)
                    assertions.setdefault(current_op, set())

                # Parse jsonpath assertions
                jp = re.match(r'jsonpath "\$\.(\w+)"', line)
                if jp and current_op:
                    assertions[current_op].add(jp.group(1))

                # Parse jsonpath on array items
                jp2 = re.match(r'jsonpath "\$\[0\]\.(\w+)"', line)
                if jp2 and current_op:
                    assertions[current_op].add(jp2.group(1))

                # Parse header assertions
                hdr = re.match(r'header "([^"]+)"', line)
                if hdr and current_op:
                    assertions[current_op].add(f"header:{hdr.group(1)}")

    return ops, assertions


def collect_required(schema):
    """Recursively collect required fields from a schema."""
    fields = set()
    for f in schema.get("required", []):
        fields.add(f)
    for key in ("oneOf", "allOf", "anyOf"):
        for branch in schema.get(key, []):
            fields |= collect_required(branch)
    if schema.get("type") == "array" and "items" in schema:
        fields |= collect_required(schema["items"])
    return fields


def collect_required_headers(resp):
    return set(resp.get("headers", {}).keys())


def main():
    with open(SPEC_PATH) as f:
        spec = yaml.safe_load(f)

    hurl_ops, hurl_assertions = parse_hurl_files()

    # --- Operation coverage ---
    all_ops = []
    for path, methods in spec["paths"].items():
        for method in ("head", "get", "post", "put", "delete"):
            if method in methods:
                op_id = methods[method].get("operationId", f"{method.upper()} {path}")
                all_ops.append((method.upper(), path, op_id))

    covered = [(m, p, o) for m, p, o in all_ops if (m, p) in hurl_ops]
    missing = [(m, p, o) for m, p, o in all_ops if (m, p) not in hurl_ops]

    print(f"=== Operation Coverage: {len(covered)}/{len(all_ops)} ===")
    if missing:
        print(f"Missing ({len(missing)}):")
        for m, p, o in sorted(missing):
            print(f"  {m:6} {p:55} {o}")
    print()

    # --- Assertion coverage ---
    print("=== Assertion Coverage (required response fields) ===")
    gaps = []
    ok_count = 0

    for path, methods in sorted(spec["paths"].items()):
        for method in ("get", "post", "put"):
            if method not in methods:
                continue
            for code in ("200", "201"):
                resp = methods[method].get("responses", {}).get(code, {})
                schema = (
                    resp.get("content", {})
                    .get("application/json", {})
                    .get("schema", {})
                )
                required_fields = collect_required(schema)
                required_headers = collect_required_headers(resp)

                if not required_fields and not required_headers:
                    continue

                norm_path = path
                op_key = (method.upper(), norm_path)
                asserted = hurl_assertions.get(op_key, set())

                missing_fields = required_fields - asserted
                missing_headers = {
                    h for h in required_headers if f"header:{h}" not in asserted
                }

                if missing_fields or missing_headers:
                    gaps.append(
                        (method.upper(), path, code, missing_fields, missing_headers)
                    )
                else:
                    ok_count += 1

    if gaps:
        print(f"  {ok_count} endpoints fully asserted")
        print(f"  {len(gaps)} endpoints with missing assertions:")
        print()
        for method, path, code, fields, headers in gaps:
            print(f"  {method:6} {path} [{code}]")
            if fields:
                print(f"    missing field assertions: {sorted(fields)}")
            if headers:
                print(f"    missing header assertions: {sorted(headers)}")
    else:
        print(f"  All {ok_count} endpoints fully asserted!")

    # Exit with error if there are gaps
    if gaps:
        sys.exit(1)


if __name__ == "__main__":
    main()
