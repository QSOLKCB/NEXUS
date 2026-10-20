#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Bind the proof model to v1.0.0 and compare freshly executed Lean/Rust models.

Run with `python3 -I scripts/check_formal.py`; requires git, cargo and lake.
"""
import collections
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
BASELINE = "24c7b5bb8b158d84e08b10ccc8fdd16b6889b7e4"
EXPECTED_ROWS = {
    "vertex": 12, "edge": 24, "triangle": 8, "field": 54, "atlas": 576,
    "partner": 64, "plan": 24, "control": 144, "seed": 144,
    "state": 108, "restored": 108, "increment": 64, "record": 126,
}


def run(args, cwd=ROOT, timeout=300):
    return subprocess.run(args, cwd=cwd, check=True, capture_output=True,
                          timeout=timeout).stdout


def source_binding():
    lock = json.loads((ROOT / "formal/source-lock.json").read_text())
    if lock["release_commit"] != BASELINE or lock["release_tag"] != "v1.0.0":
        raise ValueError("formalisation baseline was changed")
    resolved = run(["git", "rev-parse", "v1.0.0^{commit}"]).decode().strip()
    if resolved != BASELINE:
        raise ValueError("v1.0.0 does not resolve to the pinned release commit")
    paths = run(["git", "ls-tree", "-r", "--name-only", BASELINE,
                 "src", "site", "Cargo.toml", "Cargo.lock"]).decode().splitlines()
    if set(paths) != set(lock["sha256"]):
        raise ValueError("source lock does not cover exactly the released runtime files")
    current = {str(p.relative_to(ROOT)) for folder in ["src", "site"]
               for p in (ROOT / folder).rglob("*") if p.is_file()}
    current.update(["Cargo.toml", "Cargo.lock"])
    if current != set(paths):
        raise ValueError("runtime file inventory changed; update the model and source binding explicitly")
    for name in paths:
        expected = lock["sha256"][name]
        released = run(["git", "show", f"{BASELINE}:{name}"])
        for label, data in [("release", released), ("worktree", (ROOT / name).read_bytes())]:
            if hashlib.sha256(data).hexdigest() != expected:
                raise ValueError(f"{label} source identity mismatch: {name}")
    print(f"Source binding passed: {len(paths)} runtime files at {BASELINE}")


def proof_inventory():
    inventory = json.loads((ROOT / "formal/theorems.json").read_text())
    found = []
    for path in sorted((ROOT / "formal/Nexus").glob("*.lean")):
        source = path.read_text()
        namespace = re.search(r"^namespace (Nexus\.[A-Za-z]+)$", source, re.M).group(1)
        found.extend(namespace + "." + name for name in re.findall(r"^theorem (\w+)", source, re.M))
    if sorted(found) != inventory:
        raise ValueError("named theorem inventory changed")
    print(f"Proof inventory passed: {len(found)} named theorems")


def parse_rows(raw):
    rows = []
    for line in raw.decode("ascii").splitlines():
        tag, *values = line.split(",")
        if tag not in EXPECTED_ROWS or not values:
            raise ValueError(f"invalid parity row: {line[:100]}")
        rows.append((tag, tuple(int(v) for v in values)))
    counts = collections.Counter(tag for tag, _ in rows)
    if counts != EXPECTED_ROWS:
        raise ValueError(f"incomplete parity output: {dict(counts)}")
    return rows


def compare_models(lean, rust):
    a, b = parse_rows(lean), parse_rows(rust)
    for row, (left, right) in enumerate(zip(a, b), 1):
        if left != right:
            raise ValueError(f"Rust/Lean mismatch at row {row}: {left} != {right}")
    initial = {values[:2]: values[3:] for tag, values in a if tag == "state" and values[2] == 0}
    for tag, values in a:
        if tag == "restored" and values[3:] != initial[values[:2]]:
            raise ValueError(f"inverse failed to restore seed {values[:3]}")
    print(f"Rust/Lean parity passed: {len(a)} rows; 108 full states and 108 inverse restorations")


def negative_checks(lean, rust):
    """Exercise the gate with incomplete, mismatched and false-restoration data."""
    def rejects(label, left, right):
        try:
            compare_models(left, right)
        except ValueError:
            return
        raise ValueError(f"gate regression: accepted {label}")

    rejects("empty output", b"", rust)
    rejects("truncated output", b"\n".join(lean.splitlines()[:-1]) + b"\n", rust)
    rows = lean.splitlines()
    idx = next(i for i, row in enumerate(rows) if row.startswith(b"restored,"))
    values = rows[idx].split(b",")
    values[-1] = str((int(values[-1]) + 1) % 4).encode("ascii")
    rows[idx] = b",".join(values)
    corrupted = b"\n".join(rows) + b"\n"
    rejects("changed register", corrupted, rust)
    rejects("matching but unrestored states", corrupted, corrupted)
    print("Four negative gate checks passed")


def main():
    source_binding()
    proof_inventory()
    if sys.argv[1:] == ["--source-only"]:
        return
    if sys.argv[1:]:
        raise ValueError("usage: check_formal.py [--source-only]")
    print(run(["lake", "build", "Nexus", "parity"], ROOT / "formal").decode(), end="")
    print(run(["lake", "env", "lean", "-DwarningAsError=true", "Audit.lean"], ROOT / "formal").decode(), end="")
    lean = run([str(ROOT / "formal/.lake/build/bin/parity")])
    rust = run(["cargo", "run", "--quiet", "--locked", "--example", "lean_reference"])
    compare_models(lean, rust)
    negative_checks(lean, rust)


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as exc:
        sys.stderr.write(exc.stdout.decode(errors="replace"))
        sys.stderr.write(exc.stderr.decode(errors="replace"))
        raise SystemExit(exc.returncode)
    except (ValueError, OSError, subprocess.TimeoutExpired) as exc:
        raise SystemExit(str(exc))
