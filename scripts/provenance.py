#!/usr/bin/env python3
"""Assemble VERSION_PROVENANCE.json from raw logs. Never typed by hand."""
import json
import re
import sys
from pathlib import Path


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace").strip()
    except FileNotFoundError:
        return ""


def field(text: str, key: str) -> str:
    match = re.search(rf"^{re.escape(key)}:\s*(.+)$", text, re.MULTILINE)
    return match.group(1).strip() if match else ""


def main() -> None:
    results = Path(sys.argv[1])
    raw = results / "00-provenance" / "raw"
    metadata = {}
    try:
        meta = json.loads(read(raw / "cargo-metadata.json"))
        package = next(p for p in meta["packages"] if p["name"] == "nexus")
        metadata = {
            "package_name": package["name"],
            "package_version": package["version"],
            "declared_rust_version": package.get("rust_version"),
        }
    except (json.JSONDecodeError, StopIteration, KeyError):
        metadata = {"package_name": None, "package_version": None, "declared_rust_version": None}
    rustc = read(raw / "rustc.txt")
    cargo = read(raw / "cargo.txt")
    binary = read(results / "01-build" / "primary" / "binary.sha256").split()
    env = read(raw / "environment.txt")
    doc = {
        **metadata,
        "active_rustc_release": field(rustc, "release"),
        "active_rustc_commit": field(rustc, "commit-hash"),
        "active_rustc_host": field(rustc, "host"),
        "active_cargo_release": field(cargo, "release"),
        "rustup_active_toolchain": read(raw / "rustup-active-toolchain.txt"),
        "msrv_toolchain_result": "INCONCLUSIVE" if not (results / "01-build" / "msrv").exists() else "see 01-build/msrv",
        "control_commit": read(results / "00-provenance" / "control-commit.txt"),
        "tested_source_commit": read(results / "00-provenance" / "tested-source-commit.txt"),
        "binary_sha256": binary[0] if binary else None,
        "host_advertised_cpus": (re.search(r"^--- processors ---\n(\d+)", env, re.MULTILINE) or [None, None])[1],
        "effective_cpus": read(raw / "effective-cpus.txt"),
        "uname": (re.search(r"^--- uname ---\n(.+)$", env, re.MULTILINE) or [None, None])[1],
        "sources": {
            "rustc": "00-provenance/raw/rustc.txt",
            "cargo": "00-provenance/raw/cargo.txt",
            "cargo_metadata": "00-provenance/raw/cargo-metadata.json",
            "environment": "00-provenance/raw/environment.txt",
        },
    }
    print(json.dumps(doc, indent=2))


if __name__ == "__main__":
    main()
