#!/usr/bin/env python3
"""Throwaway spike S3: summarise out/*-result.json into out/summary.txt (and stdout).

Extra CPU of a view = (view phase - baseline phase) of the same run, server + reader + the
view's tmux client + any pipe process. ps has 0.01 s resolution.
"""

import glob
import json
import os
import sys

out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "out")
lines = []
p = lines.append
runs = []
for f in sorted(glob.glob(os.path.join(out, "*-result.json"))):
    tag = os.path.basename(f)[: -len("-result.json")]
    runs.append((tag, json.load(open(f))))

p("tag | echo p50 / p95 / max ms | echoes | base server % | base reader % | view server % | view reader % | "
  "view client+pipe % | extra (server+reader+client+pipe) % | view conn other-pane lines (echo / noisy) | "
  "main %output lines | first frame ms | captures/changed")
for tag, r in runs:
    e = r.get("echo", {})
    b = r.get("baseline_cpu") or {}
    v = r.get("view_cpu") or {}
    extra_procs = sum(val for k, val in v.items() if k == "view-client" or k.startswith("pipe-"))
    if v:
        extra = (v.get("server", 0) - b.get("server", 0)) + (v.get("reader", 0) - b.get("reader", 0)) + extra_procs
        extra_s = f"{extra:+.2f}"
    else:
        extra_s = "n/a"
    echo_other = sum(n for pane, n in (r.get("echo_conn_output_lines") or {}).items())
    view_other = r.get("view_conn_other_panes", "n/a")
    ff = r.get("first_frame_echo") or {}
    ff_ms = round(1000 * ((ff.get("capture_s") or 0) + (ff.get("cursor_s") or 0)), 2)
    vf = r.get("view_frames") or {}
    lat = (f"{e.get('p50_ms')} / {e.get('p95_ms')} / {e.get('max_ms')}" if "p50_ms" in e else "none")
    p(f"{tag} | {lat} | {e.get('echoes_seen')}/{e.get('keys_sent')} | {b.get('server')} | {b.get('reader')} | "
      f"{v.get('server', 'n/a')} | {v.get('reader', 'n/a')} | {round(extra_procs, 3) if v else 'n/a'} | {extra_s} | "
      f"echo-phase lines {echo_other} (echo pane only: {set((r.get('echo_conn_output_lines') or {}).keys())}) / "
      f"{view_other} | {r.get('main_output_lines')} | {ff_ms} | {vf.get('captures', '-')}/{vf.get('changed', '-')}")

p("")
p("capture cost (S3.6): tag | visible p50/p95 ms, bytes, server ms/call | -S -2000 p50/p95 ms, bytes, lines, server ms/call")
for tag, r in runs:
    c = r.get("capture_cost", {})
    a, h = c.get("visible", {}), c.get("history2000", {})
    p(f"{tag} | {a.get('p50_ms')}/{a.get('p95_ms')}, {a.get('bytes')}, {a.get('server_cpu_ms_per_call')} | "
      f"{h.get('p50_ms')}/{h.get('p95_ms')}, {h.get('bytes')}, {h.get('lines')}, {h.get('server_cpu_ms_per_call')}")

# The other checks' outputs, as the report cites them (home paths shortened).
for name, keep in (("resize.txt", None), ("trust.txt", None), ("cut-3.2a.txt", "exact "), ("cut-3.7c.txt", "exact "), ("closed.txt", None),
                   ("fidelity-fixtures-eN.txt", ("==", "differing", "cursor", "alternate", "row ")),
                   ("fidelity-fixtures-e.txt", ("==", "cells differing"))):
    f = os.path.join(out, name)
    if not os.path.exists(f):
        continue
    p("")
    p(f"== out/{name}")
    for line in open(f).read().splitlines():
        if keep is None or any(k in line for k in ((keep,) if isinstance(keep, str) else keep)):
            p(line.replace(os.path.expanduser("~"), "/HOME"))

text = "\n".join(lines) + "\n"
open(os.path.join(out, "summary.txt"), "w").write(text)
print(text)
