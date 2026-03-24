# RustyTAMS API Integration Tests — Error Cases
#
# Tests authentication, validation, and error response format.
#
# Run: hurl --test tests/errors.hurl

# --- Teardown stale data ---

DELETE http://localhost:5800/flows/hurl-err-3
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

DELETE http://localhost:5800/flows/hurl-err-4
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

DELETE http://localhost:5800/flows/hurl-err-5/segments?timerange=_
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

DELETE http://localhost:5800/flows/hurl-err-5
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

# --- 401: No authentication ---

GET http://localhost:5800/service
HTTP 401

GET http://localhost:5800/sources
HTTP 401

GET http://localhost:5800/flows
HTTP 401

# --- 404: Nonexistent resources ---

GET http://localhost:5800/flows/nonexistent-flow
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 404
[Asserts]
jsonpath "$.type" isString
jsonpath "$.summary" isString

GET http://localhost:5800/sources/nonexistent-source
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 404
[Asserts]
jsonpath "$.type" isString
jsonpath "$.summary" isString

GET http://localhost:5800/objects/nonexistent-object
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 404
[Asserts]
jsonpath "$.type" isString
jsonpath "$.summary" isString

GET http://localhost:5800/service/webhooks/nonexistent-webhook
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 404
[Asserts]
jsonpath "$.type" isString
jsonpath "$.summary" isString

DELETE http://localhost:5800/service/webhooks/nonexistent-webhook
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP 404
[Asserts]
jsonpath "$.type" isString
jsonpath "$.summary" isString

# --- 400: Invalid JSON body ---

PUT http://localhost:5800/flows/hurl-err-1
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
```
not valid json at all
```
HTTP 400
[Asserts]
jsonpath "$.type" isString

# --- 400: Body id != path id ---

PUT http://localhost:5800/flows/hurl-err-2
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{
  "id": "different-id",
  "source_id": "err-src",
  "format": "urn:x-nmos:format:video",
  "codec": "video/h264",
  "essence_parameters": {"frame_width": 1920, "frame_height": 1080}
}
HTTP 400

# --- 400: Flow missing container, storage allocation fails ---

# First create a flow without container
PUT http://localhost:5800/flows/hurl-err-3
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{
  "id": "hurl-err-3",
  "source_id": "hurl-err-src-3",
  "format": "urn:x-nmos:format:video",
  "codec": "video/h264",
  "essence_parameters": {"frame_width": 1920, "frame_height": 1080}
}
HTTP 201

# Try to allocate storage — should fail without container
POST http://localhost:5800/flows/hurl-err-3/storage
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{"limit": 1}
HTTP 400

# Cleanup
DELETE http://localhost:5800/flows/hurl-err-3
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

# --- 403: Write to read-only flow ---

# Create a flow, set read_only, then try to post segments
PUT http://localhost:5800/flows/hurl-err-4
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{
  "id": "hurl-err-4",
  "source_id": "hurl-err-src-4",
  "format": "urn:x-nmos:format:video",
  "codec": "video/h264",
  "container": "video/mp2t",
  "essence_parameters": {"frame_width": 1920, "frame_height": 1080}
}
HTTP 201

PUT http://localhost:5800/flows/hurl-err-4/read_only
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
true
HTTP 204

POST http://localhost:5800/flows/hurl-err-4/storage
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{"limit": 1}
HTTP 403

# Cleanup — unset read_only first so we can delete
PUT http://localhost:5800/flows/hurl-err-4/read_only
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
false
HTTP 204

DELETE http://localhost:5800/flows/hurl-err-4
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

# --- 404: Storage allocation on nonexistent flow ---

POST http://localhost:5800/flows/nonexistent/storage
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{"limit": 1}
HTTP 404

# --- 200 with failed_segments (partial failure) ---

# Create a flow and segment, then post a batch with one overlapping
PUT http://localhost:5800/flows/hurl-err-5
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{
  "id": "hurl-err-5",
  "source_id": "hurl-err-src-5",
  "format": "urn:x-nmos:format:video",
  "codec": "video/h264",
  "container": "video/mp2t",
  "essence_parameters": {"frame_width": 1920, "frame_height": 1080}
}
HTTP 201

POST http://localhost:5800/flows/hurl-err-5/segments
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
{"object_id": "err5-obj-1", "timerange": "[0:0_10:0)"}
HTTP 201

# Batch post with one valid + one overlapping — returns 200 with failed_segments
POST http://localhost:5800/flows/hurl-err-5/segments
Authorization: Basic dGVzdDpwYXNzd29yZA==
Content-Type: application/json
[{"object_id": "err5-obj-2", "timerange": "[10:0_20:0)"}, {"object_id": "err5-obj-3", "timerange": "[5:0_15:0)"}]
HTTP 200
[Asserts]
jsonpath "$.failed_segments" exists

# Cleanup
DELETE http://localhost:5800/flows/hurl-err-5/segments?timerange=_
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

DELETE http://localhost:5800/flows/hurl-err-5
Authorization: Basic dGVzdDpwYXNzd29yZA==
HTTP *

# --- API key auth works ---

GET http://localhost:5800/service?access_token=test-api-key
HTTP 200
