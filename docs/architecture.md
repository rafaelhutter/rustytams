# Architecture

RustyTAMS runs as four services:

```
Browser --> tams-web:5803          (Svelte SPA + Flask static server)
         |
Client --> tams-server:5800        (metadata: sources, flows, segments, webhooks)
         |
Client --> rustfs:9000             (S3-compatible media object store)

tams-server --> tams-auth-server:5802  (credential validation)
```

- **tams-server** (port 5800) -- TAMS API. Handles all metadata operations (sources, flows, segments, storage allocation, webhooks). Generates S3 presigned URLs so clients can upload/download media directly from the object store. Serves interactive API docs at `/docs` (Swagger UI) and the OpenAPI spec at `/api-spec`.
- **rustfs** (port 9000) -- S3-compatible object store ([RustFS](https://github.com/rustfs/rustfs), included as a git submodule). Clients talk to it directly using presigned URLs obtained from the TAMS API. Any S3-compatible store (AWS S3, MinIO, etc.) can be used instead.
- **tams-auth-server** (port 5802) -- Auth service. Token issuance and credential validation (Basic auth, API keys, Bearer tokens).
- **tams-web** (port 5803) -- Web UI. Svelte 5 SPA served by a Python/Flask static server. Talks to tams-server for metadata; media upload/download uses presigned S3 URLs from the API.

Each component is independently swappable. Replace the object store by pointing `--s3-endpoint` at a different S3-compatible backend. Replace the auth server with an OAuth2 provider by implementing `/auth/check` and `/auth/token`.

## Crate Structure

```
rustytams/
  tams-types/          Shared types (timestamp, timerange, source, flow, segment, etc.)
  tams-auth/           Auth library (token store, credential validation)
  tams-store/          Metadata store library (filesystem-backed, S3 presigned URLs)
  tams-server/         TAMS API binary
  tams-auth-server/    Auth service binary
  tams-web/            Web UI (Svelte 5 + Flask)
  rustfs/              S3-compatible object store (git submodule)
```

## TAMS Spec Flow

```
1. Client -> tams-server:   POST /flows/{id}/storage     (allocate storage)
2. tams-server -> Client:   Returns put_urls              (S3 presigned PUT URLs)
3. Client -> rustfs:        PUT /bucket/key?X-Amz-...     (direct upload via presigned URL)
4. Client -> tams-server:   POST /flows/{id}/segments     (register metadata)
5. Client -> tams-server:   GET /flows/{id}/segments      (get metadata + S3 presigned GET URLs)
6. Client -> rustfs:        GET /bucket/key?X-Amz-...     (direct download via presigned URL)
```

The TAMS server never touches media bytes — it only generates presigned URLs.
