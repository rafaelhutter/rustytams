#!/usr/bin/env python3
"""Resolve all $ref in the TAMS OpenAPI spec to produce a self-contained YAML.

Schemathesis cannot resolve external $ref in examples, so this script
inlines them before running spec-check.
"""

import json
import sys
from pathlib import Path

import yaml


def resolve_refs(obj, base_dir: Path):
    """Recursively resolve $ref to local files."""
    if isinstance(obj, dict):
        if "$ref" in obj and isinstance(obj["$ref"], str):
            ref_path = base_dir / obj["$ref"]
            if ref_path.exists():
                with open(ref_path) as f:
                    if ref_path.suffix == ".json":
                        resolved = json.load(f)
                    else:
                        resolved = yaml.safe_load(f)
                return resolve_refs(resolved, ref_path.parent)
            return obj
        return {k: resolve_refs(v, base_dir) for k, v in obj.items()}
    elif isinstance(obj, list):
        return [resolve_refs(item, base_dir) for item in obj]
    return obj


def main():
    spec_path = (
        Path(sys.argv[1])
        if len(sys.argv) > 1
        else Path("tams/api/TimeAddressableMediaStore.yaml")
    )
    output_path = (
        Path(sys.argv[2]) if len(sys.argv) > 2 else Path(".resolved-spec.yaml")
    )

    with open(spec_path) as f:
        spec = yaml.safe_load(f)

    resolved = resolve_refs(spec, spec_path.parent)

    with open(output_path, "w") as f:
        yaml.dump(
            resolved, f, default_flow_style=False, allow_unicode=True, sort_keys=False
        )

    print(f"Resolved {spec_path} -> {output_path}")


if __name__ == "__main__":
    main()
