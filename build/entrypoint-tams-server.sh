#!/bin/sh
# Translates environment variables (set via Kubernetes secrets/configmaps)
# to CLI arguments expected by tams-server (clap-based).
exec /usr/local/bin/tams-server \
  --data-dir /data \
  --s3-endpoint "https://${B2_ENDPOINT}" \
  --s3-bucket "${B2_BUCKET}" \
  --s3-access-key "${B2_ACCESS_KEY_ID}" \
  --s3-secret-key "${B2_SECRET_ACCESS_KEY}" \
  --s3-region "${B2_REGION}" \
  --auth-url "${AUTH_URL:-http://tams-auth:5802}" \
  "$@"
