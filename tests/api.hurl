# RustyTAMS API Integration Tests — Lifecycle Happy Path
#
# Covers the full TAMS dependency tree: service → flow → source →
# properties → storage → S3 upload → segments → objects → webhooks → delete.
#
# Idempotent: Phase 1 tears down stale data from crashed runs,
# Phase 3 tears down and asserts correct delete behaviour.
#
# Run: hurl --test tests/api.hurl

# ============================================================
# Phase 1: Teardown stale data from previous runs
# ============================================================

DELETE http://localhost:5800/flows/hurl-flow-1/segments?timerange=_
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

DELETE http://localhost:5800/flows/hurl-flow-1
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

DELETE http://localhost:5800/flows/hurl-flow-2
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

# ============================================================
# Phase 2: Test all operations
# ============================================================

# --- Root ---

GET http://localhost:5800/
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

# --- Service ---

GET http://localhost:5800/service
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
jsonpath "$.type" startsWith "urn:x-tams:service:"
jsonpath "$.api_version" == "8.0"
jsonpath "$.min_object_timeout" isString
jsonpath "$.name" isString

POST http://localhost:5800/service
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{"name": "HurlTest", "description": "Integration test run"}
HTTP 200
[Asserts]
jsonpath "$.name" == "HurlTest"

GET http://localhost:5800/service/storage-backends
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
jsonpath "$[0].id" isString
jsonpath "$[0].store_type" isString
jsonpath "$[0].provider" == "s3"
jsonpath "$[0].store_product" isString
jsonpath "$[0].label" == "s3"

# --- Create flow (unlocks source + flow + segment + object endpoints) ---

PUT http://localhost:5800/flows/hurl-flow-1
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{
  "id": "hurl-flow-1",
  "source_id": "hurl-src-1",
  "format": "urn:x-nmos:format:video",
  "codec": "video/h264",
  "container": "video/mp2t",
  "essence_parameters": {
    "frame_width": 1920,
    "frame_height": 1080
  }
}
HTTP 201
[Asserts]
jsonpath "$.id" == "hurl-flow-1"
jsonpath "$.source_id" == "hurl-src-1"
jsonpath "$.format" == "urn:x-nmos:format:video"
jsonpath "$.codec" == "video/h264"
jsonpath "$.container" == "video/mp2t"
jsonpath "$.essence_parameters" exists
jsonpath "$.created" isString
jsonpath "$.metadata_updated" isString

# --- Sources ---

GET http://localhost:5800/sources
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
header "X-Paging-Limit" exists
jsonpath "$[0].id" isString
jsonpath "$[0].format" isString

GET http://localhost:5800/sources/hurl-src-1
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
jsonpath "$.id" == "hurl-src-1"
jsonpath "$.format" == "urn:x-nmos:format:video"

# Source label
PUT http://localhost:5800/sources/hurl-src-1/label
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
"Test Source"
HTTP 204

GET http://localhost:5800/sources/hurl-src-1/label
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
body == "\"Test Source\""

DELETE http://localhost:5800/sources/hurl-src-1/label
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 204

# Source description
PUT http://localhost:5800/sources/hurl-src-1/description
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
"A test source for Hurl integration tests"
HTTP 204

GET http://localhost:5800/sources/hurl-src-1/description
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
body contains "test source"

DELETE http://localhost:5800/sources/hurl-src-1/description
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 204

# Source tags
PUT http://localhost:5800/sources/hurl-src-1/tags/test-tag
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
"test-value"
HTTP 204

GET http://localhost:5800/sources/hurl-src-1/tags
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

GET http://localhost:5800/sources/hurl-src-1/tags/test-tag
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/sources/hurl-src-1/tags/test-tag
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

DELETE http://localhost:5800/sources/hurl-src-1/tags/test-tag
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 204

# --- Flow properties ---

# Flow label
PUT http://localhost:5800/flows/hurl-flow-1/label
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
"Test Flow"
HTTP 204

GET http://localhost:5800/flows/hurl-flow-1/label
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
body == "\"Test Flow\""

DELETE http://localhost:5800/flows/hurl-flow-1/label
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 204

# Flow description
PUT http://localhost:5800/flows/hurl-flow-1/description
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
"A test flow"
HTTP 204

GET http://localhost:5800/flows/hurl-flow-1/description
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

DELETE http://localhost:5800/flows/hurl-flow-1/description
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 204

# Flow tags
PUT http://localhost:5800/flows/hurl-flow-1/tags/env
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
"test"
HTTP 204

GET http://localhost:5800/flows/hurl-flow-1/tags
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

GET http://localhost:5800/flows/hurl-flow-1/tags/env
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/flows/hurl-flow-1/tags/env
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

DELETE http://localhost:5800/flows/hurl-flow-1/tags/env
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 204

# Flow read_only
PUT http://localhost:5800/flows/hurl-flow-1/read_only
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
false
HTTP 204

GET http://localhost:5800/flows/hurl-flow-1/read_only
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

# Flow bit rates
PUT http://localhost:5800/flows/hurl-flow-1/max_bit_rate
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
10000000
HTTP 204

GET http://localhost:5800/flows/hurl-flow-1/max_bit_rate
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

DELETE http://localhost:5800/flows/hurl-flow-1/max_bit_rate
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 204

PUT http://localhost:5800/flows/hurl-flow-1/avg_bit_rate
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
5000000
HTTP 204

GET http://localhost:5800/flows/hurl-flow-1/avg_bit_rate
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

DELETE http://localhost:5800/flows/hurl-flow-1/avg_bit_rate
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 204

# Flow collection — set, read (assert required fields), then delete
PUT http://localhost:5800/flows/hurl-flow-1/flow_collection
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
[{"id": "hurl-flow-1", "role": "primary"}]
HTTP 204

GET http://localhost:5800/flows/hurl-flow-1/flow_collection
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
jsonpath "$[0].id" isString
jsonpath "$[0].role" isString

DELETE http://localhost:5800/flows/hurl-flow-1/flow_collection
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 204

# Flow list + detail
GET http://localhost:5800/flows
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
header "X-Paging-Limit" exists
jsonpath "$[0].id" isString
jsonpath "$[0].source_id" isString
jsonpath "$[0].format" isString
jsonpath "$[0].codec" isString
jsonpath "$[0].essence_parameters" exists

GET http://localhost:5800/flows/hurl-flow-1
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
jsonpath "$.id" == "hurl-flow-1"
jsonpath "$.source_id" == "hurl-src-1"
jsonpath "$.format" isString
jsonpath "$.codec" isString
jsonpath "$.essence_parameters" exists

# --- Storage allocation ---

POST http://localhost:5800/flows/hurl-flow-1/storage
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{"limit": 1}
HTTP 201
[Captures]
put_url: jsonpath "$.media_objects[0].put_url.url"
object_id: jsonpath "$.media_objects[0].object_id"
[Asserts]
jsonpath "$.media_objects[0].put_url.url" contains "X-Amz-Signature"
jsonpath "$.media_objects[0].object_id" isString

# --- S3 upload via presigned URL ---

PUT {{put_url}}
Content-Type: video/mp2t
```
test media bytes for hurl integration test
```
HTTP 200

# --- Segments ---

POST http://localhost:5800/flows/hurl-flow-1/segments
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{"object_id": "{{object_id}}", "timerange": "[0:0_10:0)"}
HTTP 201

GET http://localhost:5800/flows/hurl-flow-1/segments
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
header "X-Paging-Limit" exists
header "X-Paging-Timerange" exists
header "X-Paging-Count" exists
jsonpath "$[0].object_id" == {{object_id}}
jsonpath "$[0].timerange" == "[0:0_10:0)"
jsonpath "$[0].get_urls[0].url" contains "X-Amz-Signature"
jsonpath "$[0].get_urls[0].label" == "s3"

# --- Objects ---

GET http://localhost:5800/objects/{{object_id}}
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
jsonpath "$.id" == {{object_id}}
jsonpath "$.referenced_by_flows" contains "hurl-flow-1"
jsonpath "$.timerange" isString

POST http://localhost:5800/objects/{{object_id}}/instances
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{"url": "https://cdn.example.com/hurl-test", "label": "cdn"}
HTTP 201

DELETE http://localhost:5800/objects/{{object_id}}/instances?label=cdn
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 204

# --- Webhooks ---

POST http://localhost:5800/service/webhooks
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{
  "url": "http://example.com/hurl-hook",
  "events": ["flows/created"]
}
HTTP 201
[Captures]
webhook_id: jsonpath "$.id"
[Asserts]
jsonpath "$.id" isString
jsonpath "$.url" == "http://example.com/hurl-hook"
jsonpath "$.events" exists
jsonpath "$.status" isString

GET http://localhost:5800/service/webhooks
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
header "X-Paging-Limit" exists
jsonpath "$[0].id" isString
jsonpath "$[0].url" isString
jsonpath "$[0].events" exists
jsonpath "$[0].status" isString

GET http://localhost:5800/service/webhooks/{{webhook_id}}
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
jsonpath "$.id" == {{webhook_id}}
jsonpath "$.url" isString
jsonpath "$.events" exists
jsonpath "$.status" isString

HEAD http://localhost:5800/service/webhooks/{{webhook_id}}
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

PUT http://localhost:5800/service/webhooks/{{webhook_id}}
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{
  "url": "http://example.com/hurl-hook-updated",
  "events": ["flows/created", "flows/updated"],
  "status": "created"
}
HTTP 201
[Asserts]
jsonpath "$.id" isString
jsonpath "$.url" isString
jsonpath "$.events" exists
jsonpath "$.status" isString

# Create a second webhook for pagination
POST http://localhost:5800/service/webhooks
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{"url": "http://example.com/hurl-hook-2", "events": ["flows/updated"]}
HTTP 201
[Captures]
webhook_id2: jsonpath "$.id"

# Webhooks pagination — 2 webhooks, limit=1 forces next page
GET http://localhost:5800/service/webhooks?limit=1
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
header "X-Paging-Limit" == "1"
header "X-Paging-NextKey" exists
header "Link" exists

DELETE http://localhost:5800/service/webhooks/{{webhook_id2}}
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 204

DELETE http://localhost:5800/service/webhooks/{{webhook_id}}
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 204

# --- Pagination (create a second flow so limit=1 produces a next page) ---

PUT http://localhost:5800/flows/hurl-flow-2
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{
  "id": "hurl-flow-2",
  "source_id": "hurl-src-2",
  "format": "urn:x-nmos:format:audio",
  "codec": "audio/aac",
  "container": "video/mp2t",
  "essence_parameters": {}
}
HTTP 201

GET http://localhost:5800/flows?limit=1
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
header "X-Paging-Limit" == "1"
header "X-Paging-NextKey" exists
header "Link" exists

GET http://localhost:5800/sources?limit=1
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
header "X-Paging-Limit" == "1"
header "X-Paging-NextKey" exists
header "Link" exists

# Add a second segment for pagination
POST http://localhost:5800/flows/hurl-flow-2/storage
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{"limit": 1}
HTTP 201
[Captures]
put_url2: jsonpath "$.media_objects[0].put_url.url"
object_id2: jsonpath "$.media_objects[0].object_id"

PUT {{put_url2}}
Content-Type: video/mp2t
```
test2
```
HTTP 200

POST http://localhost:5800/flows/hurl-flow-1/segments
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{"object_id": "{{object_id2}}", "timerange": "[10:0_20:0)"}
HTTP 201

# Also register a segment on flow-2 so its DELETE returns 202
POST http://localhost:5800/flows/hurl-flow-2/segments
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{"object_id": "{{object_id2}}", "timerange": "[0:0_10:0)"}
HTTP 201

# Segments pagination — 2 segments, limit=1 forces next page
GET http://localhost:5800/flows/hurl-flow-1/segments?limit=1
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
header "X-Paging-Limit" == "1"
header "X-Paging-Timerange" exists
header "X-Paging-Count" exists
header "X-Paging-NextKey" exists
header "Link" exists

# Segments with reverse_order
GET http://localhost:5800/flows/hurl-flow-1/segments?reverse_order=true
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
header "X-Paging-Reverse-Order" exists

# Objects pagination — object_id is referenced by both flow-1 and flow-2 segments
# (flow-2 segment registered above), so limit=1 forces a next page
GET http://localhost:5800/objects/{{object_id2}}?limit=1
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
header "X-Paging-Limit" == "1"
header "X-Paging-NextKey" exists
header "Link" exists

# --- Delete requests ---

GET http://localhost:5800/flow-delete-requests
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

# --- HEAD requests (all GET endpoints) ---

HEAD http://localhost:5800/
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/service
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/service/storage-backends
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/sources
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/sources/hurl-src-1
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/sources/hurl-src-1/tags
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/sources/hurl-src-1/description
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

HEAD http://localhost:5800/sources/hurl-src-1/label
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

HEAD http://localhost:5800/flows
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/flows/hurl-flow-1
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/flows/hurl-flow-1/tags
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/flows/hurl-flow-1/description
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

HEAD http://localhost:5800/flows/hurl-flow-1/label
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

HEAD http://localhost:5800/flows/hurl-flow-1/read_only
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/flows/hurl-flow-1/flow_collection
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

HEAD http://localhost:5800/flows/hurl-flow-1/max_bit_rate
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

HEAD http://localhost:5800/flows/hurl-flow-1/avg_bit_rate
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

HEAD http://localhost:5800/flows/hurl-flow-1/segments
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/objects/{{object_id}}
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/service/webhooks
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

HEAD http://localhost:5800/flow-delete-requests
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

# ============================================================
# Phase 3: Teardown (tested — DELETE assertions are coverage)
# ============================================================

# Delete segments from flow-1
DELETE http://localhost:5800/flows/hurl-flow-1/segments?timerange=_
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 204

# Delete flow-1 (segments deleted above so immediate 204)
DELETE http://localhost:5800/flows/hurl-flow-1
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 204

# Delete flow-2 (has segments from pagination test = 202 async with delete request)
DELETE http://localhost:5800/flows/hurl-flow-2
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 202
[Captures]
delete_request_id: jsonpath "$.id"
[Asserts]
jsonpath "$.id" isString
jsonpath "$.flow_id" == "hurl-flow-2"
jsonpath "$.timerange_to_delete" isString
jsonpath "$.delete_flow" == true
jsonpath "$.status" isString

# Get specific delete request by ID
GET http://localhost:5800/flow-delete-requests/{{delete_request_id}}
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
jsonpath "$.id" == {{delete_request_id}}
jsonpath "$.flow_id" isString
jsonpath "$.timerange_to_delete" isString
jsonpath "$.delete_flow" exists
jsonpath "$.status" isString

HEAD http://localhost:5800/flow-delete-requests/{{delete_request_id}}
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200

# Check delete requests list
GET http://localhost:5800/flow-delete-requests
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 200
[Asserts]
jsonpath "$[0].id" isString
jsonpath "$[0].flow_id" isString
jsonpath "$[0].timerange_to_delete" isString
jsonpath "$[0].delete_flow" exists
jsonpath "$[0].status" isString

# Restore service name
POST http://localhost:5800/service
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{"name": "RustyTAMS"}
HTTP 200
