# Integration Tests

The integration tests use Python scripts from the [BBC TAMS repository](https://github.com/bbc/tams) (included as a git submodule) to ingest HLS content and outgest it as a transport stream file.

## Running

```bash
# First time: init the submodule
git submodule update --init

# Run the full integration test
make integration-test
```

## What it does

1. Generate sample HLS content from Big Buck Bunny (requires ffmpeg, cached after first run)
2. Create a Python venv and install dependencies
3. Start all services
4. Ingest 5 HLS segments via presigned URLs
5. Outgest them to a .ts file
6. Stop all services
7. Verify the output file

## Make Targets

```
make venv               Create Python venv with integration test deps
make sample-content     Generate HLS sample content (requires ffmpeg)
make integration-test   Run full integration test (start -> ingest -> outgest -> stop)
make generate-media     Generate test media segments with ffmpeg (no downloads)
make web-fixtures       Populate running TAMS with web UI test data
```
