#!/usr/bin/env python3
"""Assemble VERSION_PROVENANCE.json from raw logs. Never typed by hand."""
import json
import os
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


def cpu_capacity(root: Path = Path("/sys/fs/cgroup"), proc: Path = Path("/proc/self/cgroup"), affinity=None, evidence=None) -> float:
    """Available CPU capacity, capped by affinity and visible ancestor quotas."""
    if affinity is None:
        affinity = len(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else (os.cpu_count() or 1)
    capacity = float(affinity)
    if evidence is not None:
        evidence.update(affinity_cpus=affinity, cgroup_membership=read(proc), quota_files={})
    def observed(path):
        value = read(path)
        if evidence is not None and value:
            evidence["quota_files"][str(path)] = value
        return value
    roots = [(root, "cpu.max"), (root / "cpu", "cpu.cfs_quota_us"),
             (root / "cpu,cpuacct", "cpu.cfs_quota_us")]
    memberships = read(proc).splitlines()
    for base, filename in roots:
        directories = {base}
        for line in memberships:
            parts = line.split(":", 2)
            if len(parts) != 3:
                continue
            _, controllers, relative = parts
            if (filename == "cpu.max" and controllers == "") or (filename != "cpu.max" and "cpu" in controllers.split(",")):
                path = (base / relative.lstrip("/")).resolve()
                # Only traverse visible ancestors within this cgroup mount.
                if path.is_relative_to(base.resolve()):
                    while path != base.resolve():
                        directories.add(path)
                        path = path.parent
        for directory in directories:
            words = observed(directory / filename).split()
            if not words:
                continue
            if filename != "cpu.max":
                words += observed(directory / "cpu.cfs_period_us").split()
            if len(words) != 2 or words[0] == "max":
                continue
            try:
                quota, period = map(int, words)
            except ValueError:
                continue
            if quota > 0 and period > 0:
                capacity = min(capacity, quota / period)
    return capacity


def main() -> None:
    if sys.argv[1:] == ["--cpu-capacity-evidence"]:
        evidence = {}
        evidence["effective_cpus"] = cpu_capacity(evidence=evidence)
        print(json.dumps(evidence, indent=2))
        return
    if sys.argv[1:] == ["--cpu-capacity"]:
        print(cpu_capacity())
        return
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
        "host_advertised_cpus": (re.search(r"^--- processors ---\n\d+\n(\d+)", env, re.MULTILINE) or [None, None])[1],
        "effective_cpus": read(raw / "effective-cpus.txt"),
        "uname": (re.search(r"^--- uname ---\n(.+)$", env, re.MULTILINE) or [None, None])[1],
        "sources": {
            "rustc": "00-provenance/raw/rustc.txt",
            "cargo": "00-provenance/raw/cargo.txt",
            "cargo_metadata": "00-provenance/raw/cargo-metadata.json",
            "environment": "00-provenance/raw/environment.txt",
            "cpu_capacity": "00-provenance/raw/cpu-capacity.json",
        },
    }
    print(json.dumps(doc, indent=2))


if __name__ == "__main__":
    main()
