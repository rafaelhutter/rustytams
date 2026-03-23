"""Tests for the TAMS Web UI Flask server."""

import os
from unittest.mock import patch

import pytest

from app import app, DIST_DIR, main


@pytest.fixture
def client():
    """Flask test client."""
    app.config["TESTING"] = True
    with app.test_client() as c:
        yield c


@pytest.fixture(autouse=True)
def require_dist():
    """Skip tests if frontend hasn't been built."""
    if not os.path.isdir(DIST_DIR):
        pytest.skip("frontend/dist/ not found — run 'make build-web' first")


def test_index_returns_html(client):
    resp = client.get("/")
    assert resp.status_code == 200
    assert b"<!doctype html>" in resp.data.lower() or b"<!DOCTYPE html>" in resp.data


def test_index_content_type(client):
    resp = client.get("/")
    assert "text/html" in resp.content_type


def test_static_css_served(client):
    """Verify that at least one CSS asset is served from dist/assets/."""
    assets = os.listdir(os.path.join(DIST_DIR, "assets"))
    css_files = [f for f in assets if f.endswith(".css")]
    assert len(css_files) > 0, "No CSS files in dist/assets/"
    resp = client.get(f"/assets/{css_files[0]}")
    assert resp.status_code == 200
    assert "text/css" in resp.content_type


def test_static_js_served(client):
    """Verify that at least one JS asset is served from dist/assets/."""
    assets = os.listdir(os.path.join(DIST_DIR, "assets"))
    js_files = [f for f in assets if f.endswith(".js")]
    assert len(js_files) > 0, "No JS files in dist/assets/"
    resp = client.get(f"/assets/{js_files[0]}")
    assert resp.status_code == 200
    assert "javascript" in resp.content_type


def test_spa_catch_all_returns_index(client):
    """Unknown paths should return index.html for SPA routing."""
    resp = client.get("/sources")
    assert resp.status_code == 200
    assert b"<title>RustyTAMS</title>" in resp.data


def test_spa_deep_path_returns_index(client):
    resp = client.get("/flows/some-uuid-here")
    assert resp.status_code == 200
    assert b"<title>RustyTAMS</title>" in resp.data


def test_path_traversal_blocked(client):
    """Paths like ../../etc/passwd must not leak file existence."""
    resp = client.get("/../../etc/passwd")
    # Should either 404 or return index.html, never the actual file
    assert resp.status_code in (200, 404)
    if resp.status_code == 200:
        assert b"<title>RustyTAMS</title>" in resp.data


class TestMainMissingDist:
    """Tests for main() that do NOT require the dist directory."""

    @pytest.fixture(autouse=True)
    def require_dist(self):
        """Override the module-level autouse fixture so these tests always run."""

    def test_main_exits_when_dist_missing(self):
        """main() should raise SystemExit(1) when DIST_DIR does not exist."""
        with (
            patch("app.os.path.isdir", return_value=False),
            patch("sys.argv", ["app.py"]),
            pytest.raises(SystemExit) as exc_info,
        ):
            main()
        assert exc_info.value.code == 1
