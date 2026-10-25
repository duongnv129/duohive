#!/usr/bin/env python3
"""Turns `duohive bench --gate --json` output into a Markdown report for the job summary.

Usage: bench-report.py <gate.json>. Prints Markdown. A missing or unreadable file (the gate
ended without a verdict, for example exit 3 for load) is reported as such, never as a pass.
"""
import json
import sys

LABELS = [
    ("idle_cpu", "Idle CPU", "%"),
    ("memory", "Memory (end of window)", "MB"),
    ("hook", "Hook to screen (max)", "ms"),
    ("tilde", "Screen-read `~` (p95)", "ms"),
    ("first_frame", "First frame on reopen", "ms"),
]


def load(path):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def tier_table(tier):
    worst = tier["worst"]["targets"]
    rows = ["| Target | Worst of 3 runs | Limit | |", "|---|---|---|---|"]
    for key, label, unit in LABELS:
        t = worst[key]
        mark = "pass" if t["pass"] else "**MISSED**"
        rows.append(f"| {label} | {t['value']:.2f} {unit} | < {t['target']:g} {unit} | {mark} |")
    return "\n".join(rows)


def main(path):
    gate = load(path)
    print("## Benchmark report (hosted macOS arm64 runner)\n")
    if not gate:
        print("The gate ended without a verdict (no result file). See the job log: exit 3 means every")
        print("attempt was discarded for machine load.")
        return
    verdict = "PASS" if gate.get("all_pass") else "FAIL"
    print(f"**{verdict}** · version `{gate.get('version')}` · commit `{str(gate.get('git_sha'))[:7]}` · "
          f"platform `{gate.get('platform')}` · binary {gate.get('binary_bytes')} B (limit 10000000 B)\n")
    print("> A shared hosted runner is not the plugged-in M-series machine of TD §10: the load check is "
          "off here, so treat this as a report. The release gate is run on the release machine.\n")
    for tier in gate.get("tiers", []):
        print(f"### {tier['agents']} agents\n")
        print(tier_table(tier) + "\n")
        info = tier.get("without_view")
        if info:
            print(f"Without the live view (information only): idle CPU {info['idle_cpu_pct']:.2f} %, "
                  f"memory {info['rss_end_mb']:.1f} MB.\n")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "bench-gate.json")
