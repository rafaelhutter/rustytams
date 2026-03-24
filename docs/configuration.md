# Configuration

Each service accepts CLI arguments.

## tams-server

```
--port                 Port to listen on (default: 5800)
--data-dir             Data directory for persistence (default: data)
--s3-endpoint          S3 endpoint URL (default: http://localhost:9000)
--s3-bucket            S3 bucket name for media objects (default: tams-media)
--s3-access-key        S3 access key (default: rustfsadmin)
--s3-secret-key        S3 secret key (default: rustfsadmin123)
--s3-region            S3 region (default: us-east-1)
--auth-url             Base URL of auth server (default: http://localhost:5802)
```

API documentation is served automatically:
- `/docs` — interactive Swagger UI (no authentication required)
- `/api-spec` — raw OpenAPI 3.1 YAML specification

The Swagger UI assets are downloaded once via `make swagger-ui` and the OpenAPI spec is resolved from the BBC TAMS spec during `make build`. Both are embedded into the binary at compile time.

## rustfs (S3 object store)

RustFS is built from source as a git submodule. Configuration is via environment variables:

```
RUSTFS_ACCESS_KEY      S3 access key (default in Makefile: rustfsadmin)
RUSTFS_SECRET_KEY      S3 secret key (default in Makefile: rustfsadmin123)
RUSTFS_ADDRESS         Bind address (default: :9000)
RUSTFS_CONSOLE_ENABLE  Enable web console (default in Makefile: false)
```

Any S3-compatible store can be used instead by pointing `--s3-endpoint` at it.

## tams-auth-server

```
--port                 Port to listen on (default: 5802)
```

## Default Credentials

All development credentials are documented in [`.env.example`](../.env.example) at the project root. Copy it to `.env` to customize:

```bash
cp .env.example .env
```

Defaults:
- **Username:** `test`
- **Password:** `password`
- **API Key:** `test-api-key`
- **S3 Access Key:** `rustfsadmin`
- **S3 Secret Key:** `rustfsadmin123`
