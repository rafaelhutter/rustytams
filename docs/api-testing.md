# API Integration Testing

RustyTAMS uses [Hurl](https://hurl.dev) for API integration testing against the live server. Tests verify every endpoint against the BBC TAMS OpenAPI specification.

## Prerequisites

```bash
brew install hurl
```

## Running Tests

Start the server, then run:

```bash
make run-all
make hurl-test
```

Example output:

```
hurl --test tests/api.hurl tests/errors.hurl
Success tests/errors.hurl (27 request(s) in 17 ms)
Success tests/api.hurl (102 request(s) in 75 ms)
--------------------------------------------------------------------------------
Executed files:    2
Executed requests: 129 (1675.3/s)
Succeeded files:   2 (100.0%)
Failed files:      0 (0.0%)
Duration:          77 ms (0h:0m:0s:77ms)
```

## Coverage Verification

Verify that tests cover all 80 OpenAPI operations and assert on all required response fields:

```bash
make hurl-coverage
```

Example output:

```
=== Operation Coverage: 80/80 ===

=== Assertion Coverage (required response fields) ===
  All 17 endpoints fully asserted!
```

The coverage script (`scripts/check-hurl-coverage.py`) reads the OpenAPI spec and parses the Hurl test files to verify:

1. **Operation coverage** — every `operationId` in the spec has a corresponding HTTP request in the tests
2. **Assertion coverage** — every `required` field in each response schema is asserted on with a `jsonpath` or `header` check

If a new endpoint is added to the spec without a corresponding test, `make hurl-coverage` fails.

## Test Structure

### `tests/api.hurl` — Lifecycle Happy Path (102 requests)

Walks the full TAMS dependency tree in three phases:

**Phase 1: Teardown stale data** — deletes leftovers from a previous crashed run (`HTTP *` accepts any status). Makes the test idempotent.

**Phase 2: Test all operations** — creates resources, exercises every endpoint, asserts responses:

```
Service:     GET /, GET /service, POST /service, GET /storage-backends
Flow:        PUT (create), GET list + detail, all properties (label,
             description, tags, read_only, bit rates, flow_collection)
Source:      GET list + detail, all properties (label, description, tags)
Storage:     POST allocate → captures presigned S3 PUT URL
S3 upload:   PUT to presigned URL
Segments:    POST register, GET list (with pagination)
Objects:     GET detail (with pagination), POST + DELETE instances
Webhooks:    POST create, GET list + detail, PUT update, DELETE
Pagination:  limit=1 on flows, sources, segments, webhooks, objects
HEAD:        HEAD for every GET endpoint
Delete reqs: DELETE flow (202 async) → GET /flow-delete-requests/{id}
```

**Phase 3: Teardown** — deletes all test resources, asserting correct delete response codes. The teardown IS part of the test coverage — it covers DELETE operations.

### `tests/errors.hurl` — Negative Cases (27 requests)

```
401:  GET without authentication (3 endpoints)
404:  nonexistent flow, source, object, webhook (5 endpoints)
400:  invalid JSON body, body id != path id, missing container,
      overlapping segment timerange (batch with failed_segments)
403:  write to read-only flow
200:  API key auth via access_token query parameter
```

## Idempotency

Tests use fixed resource IDs (`hurl-flow-1`, `hurl-src-1`, etc.). The Phase 1 teardown handles stale data from crashed runs. Running `make hurl-test` multiple times in a row always succeeds.

## What the Tests Verify Against the Spec

For each endpoint, assertions are derived from the BBC TAMS OpenAPI spec response schemas:

- **Status codes** — match the documented response codes (200, 201, 204, 400, 403, 404)
- **Required fields** — every field in the schema's `required` array is checked with `jsonpath ... exists` or `jsonpath ... isString`
- **Pagination headers** — `X-Paging-Limit`, `X-Paging-NextKey`, `X-Paging-Timerange`, `X-Paging-Count`, `X-Paging-Reverse-Order`, `Link`
- **S3 presigned URLs** — `X-Amz-Signature` present in storage allocation and segment GET responses
- **Error format** — error responses contain `type` and `summary` fields per the spec's error schema

## Schemathesis (Property-Based Testing)

In addition to Hurl tests, `make spec-check` runs [schemathesis](https://github.com/schemathesis/schemathesis) which fuzzes the API with schema-valid payloads:

```bash
make spec-check
```

This tests broader input space (random UUIDs, edge-case timeranges, etc.) but has known limitations:

- Crashes on the `{request-id}` path parameter (werkzeug bug with hyphens in param names)
- Reports false positives for undocumented 401s and semantic validations the schema can't express
- HEAD deserialization failures (HEAD has no body per [RFC 9110 Section 9.3.2](https://www.rfc-editor.org/rfc/rfc9110#section-9.3.2))

The Hurl tests are the primary integration test suite. Schemathesis is supplementary.
