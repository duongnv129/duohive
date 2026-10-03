#!/usr/bin/env python3
"""Throwaway S0 analysis: summarise every out/<tag>-* run. usage: analyze.py [out-dir]"""
import collections
import glob
import math
import os
import re
import statistics
import sys

out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "out")
SUB = re.compile(r"^%subscription-changed (\S+) \$\d+ (@\d+) (\d+) (\S+) : ?(.*)$")
POLL = re.compile(r"^(@\d+) (\d+) (\d) ?(\d*)$")


def pct(xs, p):
    xs = sorted(xs)
    if not xs:
        return float("nan")
    k = (len(xs) - 1) * p
    lo, hi = math.floor(k), math.ceil(k)
    return xs[lo] + (xs[hi] - xs[lo]) * (k - lo)


def cputime(s):
    m, sec = s.split(":")
    return int(m) * 60 + float(sec)


def load_windows(tag):
    wins = {}
    for line in open(f"{out}/{tag}-windows.txt"):
        idx, wid, pane, name = line.split()
        wins[wid] = {"idx": int(idx), "pane": pane, "name": name, "kind": name.split("-")[0]}
    return wins


def analyze(tag):
    wins = load_windows(tag)
    by_idx = {w["idx"]: wid for wid, w in wins.items()}
    lines = []
    for raw in open(f"{out}/{tag}-notifications.log", errors="replace"):
        ts, _, rest = raw.rstrip("\n").partition(" ")
        lines.append((float(ts), rest))
    marks = {rest[2:].split()[0]: (ts, rest) for ts, rest in lines if rest.startswith("# ")}
    res = {"tag": tag}
    if "subscribed" in marks and "kill-server" not in marks:
        return None
    if "subscribed" in marks:
        t_sub = marks["subscribed"][0]
        t_end = marks["kill-server"][0]
        res["duration"] = t_end - t_sub
        # activity reports: subscription lines, or poll replies
        reports = collections.defaultdict(list)
        in_block = False
        for ts, rest in lines:
            m = SUB.match(rest)
            if m and m.group(1) == "act":
                reports[m.group(2)].append((ts, int(m.group(5))))
            if rest.startswith("%begin"):
                in_block = True
            elif rest.startswith("%end") or rest.startswith("%error"):
                in_block = False
            elif in_block:
                p = POLL.match(rest)
                if p:
                    reports[p.group(1)].append((ts, int(p.group(2))))
        # delays: burst start (ground truth, ms) -> first report carrying a value >= floor(start)
        delays, missing = [], 0
        for line in open(f"{out}/{tag}-bursts.log"):
            idx, start = line.split()
            start = float(start)
            if start < t_sub + 1.5 or start > t_end - 3:
                continue
            wid = by_idx[int(idx)]
            hit = [ts for ts, v in reports[wid] if ts >= start and v >= math.floor(start)]
            if hit:
                delays.append(hit[0] - start)
            else:
                missing += 1
        res["delays"] = delays
        res["missing"] = missing
        # report rates (subscribe: changes only; poll: one line per window per poll)
        lo, hi = t_sub + 2, t_end
        rate = collections.defaultdict(list)
        min_gap = {}
        for wid, w in wins.items():
            rs = [ts for ts, _ in reports[wid] if lo <= ts <= hi]
            rate[w["kind"]].append(len(rs) / (hi - lo))
            gaps = [b - a for a, b in zip(rs, rs[1:])]
            if gaps:
                min_gap[w["kind"]] = min(min_gap.get(w["kind"], 99), min(gaps))
        res["rate"] = {k: (statistics.mean(v), max(v)) for k, v in rate.items()}
        res["min_gap"] = min_gap
        # output leaks for silenced panes (late window handled separately)
        late = marks.get("late-created", (None, "# late-created - -"))[1].split()
        late_pane = late[3] if len(late) > 3 else None
        leaks = collections.Counter()
        late_out = {"before_pause": 0, "after_pause": 0}
        t_lp = marks.get("late-pause-point", (1e20,))[0]
        for ts, rest in lines:
            if rest.startswith("%output ") or rest.startswith("%extended-output "):
                pane = rest.split()[1]
                if pane == late_pane:
                    late_out["before_pause" if ts < t_lp + 0.5 else "after_pause"] += 1
                elif ts > t_sub + 0.5:
                    leaks[pane] += 1
        res["leaks"] = sum(leaks.values())
        res["late"] = late_out
        res["pause"] = sum(1 for _, r in lines if r.startswith("%pause"))
        res["continue"] = sum(1 for _, r in lines if r.startswith("%continue"))
        res["late_pause_lines"] = [r for _, r in lines if late_pane and r in (f"%pause {late_pane}", f"%continue {late_pane}")]
        dead_wid = next(wid for wid, w in wins.items() if w["name"] == "dead")
        t_start = marks["start"][0]
        res["dead"] = [
            (round(ts - t_start, 2), r) for ts, r in lines if r.startswith("%subscription-changed dead") and f" {dead_wid} " in r
        ] or [(round(ts - t_start, 2), r) for ts, r in lines if r.startswith(dead_wid + " ") and r.endswith(" 1 3")][:1]
        res["window_close"] = [r for _, r in lines if r.startswith("%window-close") or r.startswith("%unlinked-window-close")]
        res["exit"] = [r for _, r in lines if r.startswith("%exit")]
        res["errors"] = [r for _, r in lines if r.startswith("%error")]
        res["sample_window_line"] = next((r for _, r in lines if r.startswith("%subscription-changed act") and " @1 " in r), None)
        res["sample_pane_line"] = next((r for _, r in lines if r.startswith("%subscription-changed dead") and " %1 " in r), None)
        if "rusage-start" in marks:
            a = marks["rusage-start"][1].split()
            b = marks["rusage-end"][1].split()
            used = float(b[2]) + float(b[3]) - float(a[2]) - float(a[3])
            res["reader_rusage_pct"] = 100 * used / (marks["rusage-end"][0] - marks["rusage-start"][0])
        res["late_line"] = marks.get("late-created", (None, None))[1]
    # cpu
    samples = collections.defaultdict(list)
    for line in list(open(f"{out}/{tag}-cpu.csv"))[1:]:
        t, role, _, pcpu, rss, ct = line.strip().split(",")
        samples[role].append((float(t), float(pcpu), int(rss), cputime(ct)))
    first = min(s[0][0] for s in samples.values())
    last = max(s[-1][0] for s in samples.values())
    lo, hi = first + 5, last - 1
    cpu = {}
    for role, ss in samples.items():
        win = [s for s in ss if lo <= s[0] <= hi]
        if len(win) < 2:
            continue
        cpu[role] = {
            "cpu_pct": 100 * (win[-1][3] - win[0][3]) / (win[-1][0] - win[0][0]),
            "rss_mib_max": max(s[2] for s in win) / 1024,
            "rss_mib_mean": statistics.mean(s[2] for s in win) / 1024,
        }
    res["cpu"] = cpu
    return res


def fmt(res):
    o = [f"== {res['tag']}"]
    if "delays" in res:
        d = res["delays"]
        o.append(
            f"delay n={len(d)} missing={res['missing']} median={pct(d, .5):.3f}s p95={pct(d, .95):.3f}s max={max(d) if d else float('nan'):.3f}s"
        )
        o.append("rate/window/s (mean,max): " + ", ".join(f"{k}={v[0]:.3f},{v[1]:.3f}" for k, v in sorted(res["rate"].items())))
        o.append("min gap between reports per kind (s): " + ", ".join(f"{k}={v:.3f}" for k, v in sorted(res["min_gap"].items())))
        o.append(f"%output leaks from silenced panes after silencing: {res['leaks']}")
        o.append(f"late window {res['late_line']}: %output lines before pause point {res['late']['before_pause']}, after {res['late']['after_pause']}; {res['late_pause_lines']}")
        o.append(f"%pause={res['pause']} %continue={res['continue']} errors={res['errors'][:3]}")
        o.append(f"dead: {res['dead']}")
        o.append(f"window-close: {res['window_close']} exit: {res['exit']}")
        o.append(f"sample window line: {res['sample_window_line']}")
        o.append(f"sample pane line: {res['sample_pane_line']}")
    if "reader_rusage_pct" in res:
        o.append(f"reader cpu (getrusage, subscribed..end): {res['reader_rusage_pct']:.3f}%")
    for role, c in sorted(res["cpu"].items()):
        o.append(f"cpu {role}: {c['cpu_pct']:.2f}% rss max {c['rss_mib_max']:.1f} MiB mean {c['rss_mib_mean']:.1f} MiB")
    return "\n".join(o)


tags = sorted({os.path.basename(p)[: -len("-cpu.csv")] for p in glob.glob(f"{out}/*-cpu.csv")})
for tag in tags:
    r = analyze(tag)
    if r:
        print(fmt(r))
    print()
