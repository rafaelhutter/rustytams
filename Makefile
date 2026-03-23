.PHONY: help build test lint format format-check check clean \
       run-auth run-server run-web run-all \
       run-rustfs stop-rustfs build-rustfs \
       stop-auth stop-server stop-web stop-all \
       status venv integration-test generate-media web-fixtures clean-data \
       web-venv install-web build-web test-web test-web-js test-web-flask \
       lint-web format-web check-web spec-check

PIDS_DIR := .pids
VENV := venv
PYTHON := $(VENV)/bin/python
RUSTFS_BIN := rustfs/target/release/rustfs
RUSTFS_DATA := rustfs-data
RUSTFS_PORT := 9000
RUSTFS_ACCESS_KEY := rustfsadmin
RUSTFS_SECRET_KEY := rustfsadmin123
TAMS_EXAMPLES := tams/examples
WEB_VENV := tams-web/venv
WEB_PYTHON := $(WEB_VENV)/bin/python
WEB_FRONTEND := tams-web/frontend

# Fixed flow-id for integration tests (deterministic, no log parsing needed)
INTEGRATION_FLOW_ID := 00000000-0000-0000-0000-000000000001

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

# --- Build / Test / Lint ---

build: ## Build all crates
	cargo build --workspace

test: test-web ## Run all unit tests
	cargo test --workspace

lint: ## Run clippy with warnings as errors
	cargo clippy --workspace --tests -- -D warnings

format: ## Format all code
	cargo fmt --all

format-check: ## Check formatting (no changes)
	cargo fmt --all -- --check

check: format-check lint test ## Full CI check (format + lint + test)

clean: ## Remove build artifacts
	cargo clean

clean-data: ## Remove service data directories
	rm -rf data media-data rustfs-data

# --- Service Management ---

define check_running
	@mkdir -p $(PIDS_DIR)
	@if [ -f $(PIDS_DIR)/$(1).pid ] && kill -0 $$(cat $(PIDS_DIR)/$(1).pid) 2>/dev/null; then \
		echo "$(1) is already running (PID $$(cat $(PIDS_DIR)/$(1).pid))"; \
		exit 1; \
	fi
endef

define start_service
	$(call check_running,$(1))
	cargo build -p $(2)
	cargo run -p $(2) -- $(3) > $(PIDS_DIR)/$(1).log 2>&1 & echo $$! > $(PIDS_DIR)/$(1).pid
	@echo "$(1) started (PID $$(cat $(PIDS_DIR)/$(1).pid)), log: $(PIDS_DIR)/$(1).log"
endef

define stop_service
	@if [ -f $(PIDS_DIR)/$(1).pid ] && kill -0 $$(cat $(PIDS_DIR)/$(1).pid) 2>/dev/null; then \
		kill $$(cat $(PIDS_DIR)/$(1).pid); \
		rm -f $(PIDS_DIR)/$(1).pid; \
		echo "$(1) stopped"; \
	else \
		echo "$(1) is not running"; \
		rm -f $(PIDS_DIR)/$(1).pid; \
	fi
endef

define check_status
	@if [ -f $(PIDS_DIR)/$(1).pid ] && kill -0 $$(cat $(PIDS_DIR)/$(1).pid) 2>/dev/null; then \
		echo "  $(1): running (PID $$(cat $(PIDS_DIR)/$(1).pid))"; \
	else \
		echo "  $(1): stopped"; \
		rm -f $(PIDS_DIR)/$(1).pid; \
	fi
endef

build-rustfs: ## Build rustfs from source (release mode)
	cd rustfs && cargo build --release

$(RUSTFS_BIN):
	$(MAKE) build-rustfs

run-rustfs: $(RUSTFS_BIN) ## Start rustfs S3 object store (port 9000)
	$(call check_running,rustfs)
	@mkdir -p $(RUSTFS_DATA)
	RUSTFS_ACCESS_KEY=$(RUSTFS_ACCESS_KEY) \
	RUSTFS_SECRET_KEY=$(RUSTFS_SECRET_KEY) \
	RUSTFS_ADDRESS=:$(RUSTFS_PORT) \
	RUSTFS_CONSOLE_ENABLE=false \
	$(RUSTFS_BIN) server $(RUSTFS_DATA) > $(PIDS_DIR)/rustfs.log 2>&1 & echo $$! > $(PIDS_DIR)/rustfs.pid
	@echo "rustfs started (PID $$(cat $(PIDS_DIR)/rustfs.pid)), log: $(PIDS_DIR)/rustfs.log"
	@for i in 1 2 3 4 5; do \
		AWS_ACCESS_KEY_ID=$(RUSTFS_ACCESS_KEY) AWS_SECRET_ACCESS_KEY=$(RUSTFS_SECRET_KEY) \
		aws --endpoint-url http://localhost:$(RUSTFS_PORT) s3 mb s3://tams-media 2>/dev/null && break; \
		sleep 1; \
	done

stop-rustfs: ## Stop rustfs
	$(call stop_service,rustfs)

run-auth: ## Start auth server (port 5802)
	$(call start_service,auth-server,tams-auth-server,)

run-server: ## Start TAMS API server (port 5800)
	$(call start_service,tams-server,tams-server,)

run-web: web-venv build-web ## Start web UI (port 5803)
	$(call check_running,web-server)
	$(WEB_PYTHON) tams-web/app.py > $(PIDS_DIR)/web-server.log 2>&1 & echo $$! > $(PIDS_DIR)/web-server.pid
	@echo "web-server started (PID $$(cat $(PIDS_DIR)/web-server.pid)), log: $(PIDS_DIR)/web-server.log"

run-all: ## Start all services
	@$(MAKE) run-rustfs 2>/dev/null || true
	@$(MAKE) run-auth 2>/dev/null || true
	@$(MAKE) run-server 2>/dev/null || true
	@$(MAKE) run-web 2>/dev/null || true
	@sleep 2
	@$(MAKE) status

stop-auth: ## Stop auth server
	$(call stop_service,auth-server)

stop-server: ## Stop TAMS API server
	$(call stop_service,tams-server)

stop-web: ## Stop web UI
	$(call stop_service,web-server)

stop-all: ## Stop all services
	@$(MAKE) stop-web
	@$(MAKE) stop-server
	@$(MAKE) stop-auth
	@$(MAKE) stop-rustfs

status: ## Show service status
	@echo "Services:"
	$(call check_status,auth-server)
	$(call check_status,tams-server)
	$(call check_status,web-server)
	$(call check_status,rustfs)

# --- Web UI ---

$(WEB_VENV)/bin/activate: tams-web/requirements.txt
	python3 -m venv $(WEB_VENV)
	$(WEB_VENV)/bin/pip install --upgrade pip
	$(WEB_VENV)/bin/pip install -r tams-web/requirements.txt
	touch $(WEB_VENV)/bin/activate

web-venv: $(WEB_VENV)/bin/activate ## Create web UI Python venv

install-web: ## Install web UI npm dependencies
	cd $(WEB_FRONTEND) && npm install

build-web: install-web ## Build web UI frontend
	cd $(WEB_FRONTEND) && npm run build

test-web: test-web-js test-web-flask ## Run all web UI tests

test-web-js: ## Run frontend JS tests (vitest)
	cd $(WEB_FRONTEND) && npx vitest run

test-web-flask: web-venv ## Run Flask backend tests (pytest)
	cd tams-web && $(abspath $(WEB_VENV))/bin/python -m pytest test_app.py -v

lint-web: ## Lint web UI (ruff for Python, vite build for Svelte warnings)
	ruff check tams-web/app.py
	cd $(WEB_FRONTEND) && npm run build 2>&1 | grep -E 'error|warning' || true

format-web: ## Format web UI Python code (ruff)
	ruff format tams-web/app.py

check-web: lint-web test-web build-web ## Full web UI check (lint + test + build)

# --- Python / Integration ---

$(TAMS_EXAMPLES)/sample_content/hls_output.m3u8:
	cd $(TAMS_EXAMPLES) && bash hls_sample_content.sh

sample-content: $(TAMS_EXAMPLES)/sample_content/hls_output.m3u8 ## Generate HLS sample content (requires ffmpeg)

$(VENV)/bin/activate: $(TAMS_EXAMPLES)/requirements.txt requirements-dev.txt
	python3 -m venv $(VENV)
	$(VENV)/bin/pip install --upgrade pip
	$(VENV)/bin/pip install -r $(TAMS_EXAMPLES)/requirements.txt
	$(VENV)/bin/pip install -r requirements-dev.txt
	touch $(VENV)/bin/activate

venv: $(VENV)/bin/activate ## Create Python venv with integration test + dev deps

$(TAMS_EXAMPLES)/sample_content/fixtures/video_seg_000.ts:
	cd $(TAMS_EXAMPLES) && bash generate_test_media.sh

generate-media: $(TAMS_EXAMPLES)/sample_content/fixtures/video_seg_000.ts ## Generate test media segments with ffmpeg (no downloads)

web-fixtures: venv generate-media ## Populate running TAMS with web UI test data (40 sources, 120 flows, media segments)
	@echo "=== Web Fixtures ==="
	$(PYTHON) $(TAMS_EXAMPLES)/web_fixtures.py \
		--tams-url http://localhost:5800 \
		--username test --password password \
		--media-dir $(TAMS_EXAMPLES)/sample_content/fixtures
	@echo "=== Web fixtures loaded ==="

integration-test: venv sample-content ## Run full integration test (start -> ingest -> outgest -> stop)
	@echo "=== Integration Test ==="
	@$(MAKE) stop-all 2>/dev/null || true
	@$(MAKE) clean-data
	@$(MAKE) run-all
	@echo "--- Ingesting 5 HLS segments ---"
	$(PYTHON) $(TAMS_EXAMPLES)/ingest_hls.py \
		--tams-url http://localhost:5800 \
		--username test --password password \
		--hls-filename $(TAMS_EXAMPLES)/sample_content/hls_output.m3u8 \
		--hls-segment-count 5 \
		--flow-id $(INTEGRATION_FLOW_ID)
	@echo "--- Outgesting to file ---"
	$(PYTHON) $(TAMS_EXAMPLES)/outgest_file.py \
		--tams-url http://localhost:5800 \
		--username test --password password \
		--flow-id $(INTEGRATION_FLOW_ID) \
		--output /tmp/integration_test_output.ts
	@$(MAKE) stop-all
	@if [ -s /tmp/integration_test_output.ts ]; then \
		echo "=== PASS: Integration test succeeded (output: /tmp/integration_test_output.ts) ==="; \
	else \
		echo "=== FAIL: Output file missing or empty ==="; \
		exit 1; \
	fi

TAMS_SPEC := tams/api/TimeAddressableMediaStore.yaml
RESOLVED_SPEC := .resolved-spec.yaml

$(RESOLVED_SPEC): $(TAMS_SPEC) scripts/resolve-spec.py
	$(PYTHON) scripts/resolve-spec.py $(TAMS_SPEC) $(RESOLVED_SPEC)

spec-check: venv $(RESOLVED_SPEC) ## Validate API responses against TAMS OpenAPI spec (requires running server)
	@echo "=== Spec Compliance Check (schemathesis) ==="
	$(VENV)/bin/st run $(RESOLVED_SPEC) \
		--url http://localhost:5800 \
		--max-examples 10 \
		|| true
	@echo ""
	@echo "=== Known failure categories ==="
	@echo "  ~121 undocumented 401 (spec does not list 401; coverage phase tests unauthenticated)"
	@echo "  ~73  rejected schema-compliant (semantic validation: id mismatch, missing container, etc)"
	@echo "  ~9   HEAD JSON deser (HEAD has no body per HTTP spec)"
	@echo "  ~4   accepted schema-violating (server ignores unknown query params)"
	@echo "Auth: schemathesis.toml provides access_token + basic_auth per spec security schemes"
