#!/usr/bin/env python3
"""Generate REPORT.md and summary CSVs from raw evidence. No manual numbers."""
import csv
import json
import re
import statistics
import sys
from pathlib import Path


def metrics(path: Path) -> dict:
    out = {}
    with path.open(encoding="utf-8") as handle:
        for row in csv.reader(handle):
            if len(row) == 2 and row[0] != "metric":
                out[row[0]] = row[1]
    return out


def exit_status(path: Path) -> str:
    try:
        return path.read_text().strip()
    except FileNotFoundError:
        return "missing"


def performance(results: Path) -> list[dict]:
    raw = results / "04-performance" / "raw"
    groups: dict[str, list[float]] = {}
    exec_groups: dict[tuple[str, str], list[float]] = {}
    for path in sorted(raw.glob("*.csv")):
        name = re.sub(r"-\d+$", "", path.stem)
        if name.startswith("bench-exec"):
            with path.open(encoding="utf-8") as handle:
                for row in csv.DictReader(handle):
                    key = (name, row["mode"])
                    exec_groups.setdefault(key, []).append(float(row["elapsed_ns"]))
        else:
            m = metrics(path)
            if "elapsed_ns" in m:
                groups.setdefault(name, []).append(float(m["elapsed_ns"]))
    rows = []
    for name, values in sorted(groups.items()):
        rows.append(summary_row(name, "", values))
    for (name, mode), values in sorted(exec_groups.items()):
        rows.append(summary_row(name, mode, values))
    return rows


def summary_row(name: str, mode: str, values: list[float]) -> dict:
    return {
        "command": name,
        "mode": mode,
        "runs": len(values),
        "median_elapsed_ns": f"{statistics.median(values):.0f}",
        "min_elapsed_ns": f"{min(values):.0f}",
        "max_elapsed_ns": f"{max(values):.0f}",
        "stdev_elapsed_ns": f"{statistics.pstdev(values):.0f}" if len(values) > 1 else "0",
    }


def speedup(rows: list[dict]) -> list[dict]:
    base = next((r for r in rows if r["command"] == "bench-exec-W1" and r["mode"] == "parallel-batch"), None)
    out = []
    if not base:
        return out
    for r in rows:
        m = re.match(r"bench-exec-W(\d+)$", r["command"])
        if m and r["mode"] == "parallel-batch":
            w = int(m.group(1))
            s = float(base["median_elapsed_ns"]) / float(r["median_elapsed_ns"])
            out.append({"requested_workers": w, "speedup": f"{s:.3f}", "efficiency": f"{s / w:.3f}"})
    return out


def memory(results: Path) -> list[dict]:
    rows = []
    for path in sorted((results / "06-memory" / "raw").glob("*.time.txt")):
        text = path.read_text(encoding="utf-8", errors="replace")
        m = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", text)
        rows.append({"command": path.name.replace(".time.txt", ""),
                     "max_rss_kbytes": m.group(1) if m else "not measured"})
    return rows


def table(rows: list[dict]) -> str:
    if not rows:
        return "_no rows_\n"
    keys = list(rows[0].keys())
    lines = ["| " + " | ".join(keys) + " |", "|" + "---|" * len(keys)]
    lines += ["| " + " | ".join(str(r[k]) for k in keys) + " |" for r in rows]
    return "\n".join(lines) + "\n"


def write_csv(path: Path, rows: list[dict]) -> None:
    if not rows:
        path.write_text("")
        return
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0].keys()))
        writer.writeheader()
        writer.writerows(rows)


def main() -> None:
    results = Path(sys.argv[1])
    prov = json.loads((results / "00-provenance" / "VERSION_PROVENANCE.json").read_text())
    version = prov.get("package_version") or "unknown"
    out = [f"# NEXUS {version} independent multicore verification\n"]
    out.append("## Provenance\n")
    out.append(table([{"field": k, "value": v} for k, v in prov.items() if k != "sources"]))

    out.append("\n## Build steps\n")
    out.append(table([{"step": p.stem, "exit_status": exit_status(p)}
                      for p in sorted((results / "01-build" / "primary").glob("*.exit-status"))]))

    out.append("\n## Correctness\n")
    rows = []
    for path in sorted((results / "02-correctness").glob("*.csv")):
        m = metrics(path)
        rows.append({
            "run": path.stem,
            "exit_status": exit_status(path.with_suffix(".exit-status")),
            "nexus_error": m.get("nexus_error", "n/a"),
            "antipodal_error": m.get("antipodal_error", "n/a"),
            "gap_error": m.get("gap_error", "n/a"),
            "eversion_sign_ok": m.get("eversion_sign_ok", "n/a"),
            "restoration_mismatches": m.get("restoration_mismatches", "n/a"),
            "passed": m.get("passed", "n/a"),
        })
    out.append(table(rows))

    out.append("\n## Parallel audits\n")
    base = metrics(results / "02-correctness" / "audit-default.csv") if (results / "02-correctness" / "audit-default.csv").exists() else {}
    strip = lambda m: {k: v for k, v in m.items() if k != "elapsed_ns" and "workers" not in k}
    rows = []
    for path in sorted((results / "03-parallel").glob("audit-W*.csv")):
        m = metrics(path)
        rows.append({"run": path.stem, "requested_workers": m.get("requested_workers"),
                     "effective_workers": m.get("effective_workers"),
                     "identical_to_sequential": str(strip(m) == strip(base))})
    out.append(table(rows))
    obs = results / "03-parallel" / "thread-observation" / "summary.txt"
    if obs.exists():
        out.append("\n```\n" + obs.read_text() + "```\n")

    out.append("\n## Determinism\n")
    rows = []
    for path in sorted((results / "05-determinism").glob("*.csv")):
        m = metrics(path)
        rows.append({"run": path.stem, "root_sha256": m.get("root_sha256", "n/a")})
    out.append(table(rows))

    out.append("\n## Performance (generated from raw CSV)\n")
    perf = performance(results)
    write_csv(results / "analysis" / "performance_summary.csv", perf)
    out.append(table(perf))
    effective = int(prov.get("effective_cpus") or 1)
    out.append("\n### Speedup\n")
    if effective < 2:
        out.append("INCONCLUSIVE: fewer than two effective CPUs.\n")
    else:
        out.append(table(speedup(perf)))

    out.append("\n## Memory (maximum resident set size of the NEXUS child)\n")
    mem = memory(results)
    write_csv(results / "analysis" / "memory_summary.csv", mem)
    out.append(table(mem))

    out.append("\n## Interpretation boundary\n")
    out.append("Trits and lanes are labels over continuous geometry. Lanes do not create threads. "
               "Ledgers establish byte identity. The lattice is a classical reversible state machine. "
               "No finite-field geometry, quantum computing, physical dimensionality, conservation law, "
               "or topological invariant is inferred from this test.\n")
    print("".join(out))


if __name__ == "__main__":
    main()
