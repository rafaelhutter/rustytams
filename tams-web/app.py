"""TAMS Web UI server.

Serves the built Svelte SPA from frontend/dist/.
Catch-all route returns index.html for SPA deep linking.
"""

import argparse
import os

from flask import Flask, send_from_directory, make_response
from werkzeug.exceptions import NotFound

DIST_DIR = os.path.join(os.path.dirname(__file__), "frontend", "dist")

app = Flask(__name__, static_folder=None)


def serve_index():
    """Serve index.html with no-cache headers to prevent stale JS bundles."""
    resp = make_response(send_from_directory(DIST_DIR, "index.html"))
    resp.headers["Cache-Control"] = "no-cache, no-store, must-revalidate"
    return resp


@app.route("/")
def index():
    return serve_index()


@app.route("/<path:path>")
def catch_all(path):
    """Serve static files if they exist, otherwise return index.html for SPA routing."""
    try:
        return send_from_directory(DIST_DIR, path)
    except NotFound:
        return serve_index()


def main():
    parser = argparse.ArgumentParser(description="TAMS Web UI server")
    parser.add_argument("--port", type=int, default=5803, help="Port to listen on")
    parser.add_argument("--host", default="127.0.0.1", help="Host to bind to")
    args = parser.parse_args()

    if not os.path.isdir(DIST_DIR):
        print(f"Error: {DIST_DIR} not found. Run 'make build-web' first.")
        raise SystemExit(1)

    print(f"Serving TAMS Web UI on http://{args.host}:{args.port}")
    app.run(host=args.host, port=args.port)


if __name__ == "__main__":
    main()
