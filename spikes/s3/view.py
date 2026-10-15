#!/usr/bin/env python3
"""Throwaway spike S3: one live pane view at 100 noisy windows. Not used by crates/.

usage: view.py <tmux> <sock> <candidate> <server-pid> <echo-pane> <noisy-pane> <hist-pane>
               <run-dir> <out-prefix> <base-s> <view-s>

Phases:
  1. baseline: the main connection only (S0.5: no-output,ignore-size, list-windows once a second);
  2. echo: open a view of the echo pane, send 200 `send-keys -l Z`, time each echo;
  3. noisy view: view a noisy pane for <view-s> seconds; CPU over the baseline, per-pane output counts;
  4. capture cost (S3.6): capture-pane -e -p and capture-pane -e -p -S -2000, 50 times each.
Writes <out-prefix>-result.json and <out-prefix>-latency.txt.
"""

import collections
import json
import os
import random
import re
import resource
import subprocess
import sys
import threading
import time

KEYS = 200
TICK = 0.05  # candidate d's timer

tmux, sock, cand, server_pid, echo_pane, noisy_pane, hist_pane, run_dir, prefix = sys.argv[1:10]
base_s, view_s = float(sys.argv[10]), float(sys.argv[11])
now = time.monotonic
result = {"candidate": cand, "tmux": tmux}


def ps_time(pid):
    """Cumulative CPU seconds of pid (ps, 0.01 s resolution), or None once gone."""
    try:
        s = subprocess.run(["ps", "-o", "time=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
    except OSError:
        return None
    if not s:
        return None
    parts = s.replace("-", ":").split(":")
    secs = 0.0
    for p in parts:
        secs = secs * 60 + float(p)
    return secs


def self_cpu():
    r = resource.getrusage(resource.RUSAGE_SELF)
    return r.ru_utime + r.ru_stime


class Conn:
    """One control-mode client. Counts %output per pane; hands command replies to callbacks."""

    def __init__(self, name, flags):
        args = [tmux, "-L", sock, "-f", "/dev/null", "-C", "attach-session", "-f", flags, "-t", "=duohive"]
        self.p = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        self.name = name
        self.lock = threading.Lock()
        self.pending = collections.deque([("attach", None)])
        self.out_lines = collections.Counter()  # pane -> %output lines
        self.out_bytes = collections.Counter()
        self.watch = None  # pane whose 'Z' bytes are counted
        self.z = 0
        self.z_cond = threading.Condition()
        self.z_times = []  # (z count, time first seen)
        self.errors = []
        self.th = threading.Thread(target=self.read, daemon=True)
        self.th.start()

    def send(self, cmd, cb=None):
        with self.lock:
            self.pending.append((cmd, cb))
            self.p.stdin.write((cmd + "\n").encode())
            self.p.stdin.flush()

    def call_many(self, cmds, timeout=10):
        """Send several commands on one line (one server loop pass); wait for the last reply."""
        ev = threading.Event()
        with self.lock:
            for _ in cmds[:-1]:
                self.pending.append(("many", None))
            self.pending.append(("many-last", lambda lines, err, t: ev.set()))
            self.p.stdin.write((" ; ".join(cmds) + "\n").encode())
            self.p.stdin.flush()
        return ev.wait(timeout)

    def call(self, cmd, timeout=10):
        """Send and wait for the reply: (lines, seconds, error)."""
        ev = threading.Event()
        box = {}

        def cb(lines, err, t_end):
            box.update(lines=lines, err=err, t=t_end)
            ev.set()

        t0 = now()
        self.send(cmd, cb)
        if not ev.wait(timeout):
            return None, None, "timeout"
        return box["lines"], box["t"] - t0, box["err"]

    def read(self):
        block = None
        for raw in self.p.stdout:
            t = now()
            line = raw.rstrip(b"\n").decode("utf-8", "replace")
            if block is not None:
                if line.startswith("%end ") or line.startswith("%error "):
                    with self.lock:
                        cmd, cb = self.pending.popleft() if self.pending else (None, None)
                    err = line.startswith("%error ")
                    if err:
                        self.errors.append((cmd, block))
                    if cb:
                        cb(block, err, t)
                    block = None
                else:
                    block.append(line)
                continue
            if line.startswith("%begin "):
                block = []
            elif line.startswith("%output ") or line.startswith("%extended-output "):
                parts = line.split(" ", 2)
                pane = parts[1]
                data = parts[2] if len(parts) > 2 else ""
                if line.startswith("%extended-output "):
                    data = data.split(" : ", 1)[1] if " : " in data else ""
                self.out_lines[pane] += 1
                self.out_bytes[pane] += len(data)
                if pane == self.watch:
                    self.bump(data.count("Z"), t)

    def bump(self, n, t):
        if n <= 0:
            return
        with self.z_cond:
            self.z += n
            self.z_times.append((self.z, t))
            self.z_cond.notify_all()

    def set_z(self, n, t):
        with self.z_cond:
            if n > self.z:
                self.z = n
                self.z_times.append((n, t))
                self.z_cond.notify_all()

    def close(self):
        try:
            self.p.stdin.close()
        except OSError:
            pass
        self.p.terminate()


# ---- the main connection (TD 4.1 source 2) -------------------------------------------------
main = Conn("main", "no-output,ignore-size")
main.call("refresh-client -C 160x48")
stop_poll = threading.Event()


def poll():
    nxt = now()
    while not stop_poll.is_set():
        main.send("list-windows -t =duohive -F '#{window_id} #{window_activity} #{pane_dead} #{pane_dead_status}'")
        nxt += 1.0
        time.sleep(max(0.0, nxt - now()))


threading.Thread(target=poll, daemon=True).start()


def snapshot(extra_pids):
    pids = {"server": server_pid, "main-client": main.p.pid}
    pids.update(extra_pids)
    return now(), self_cpu(), {k: ps_time(v) for k, v in pids.items()}


def cpu_pct(a, b):
    """% of one core per role between two snapshots."""
    (t0, s0, p0), (t1, s1, p1) = a, b
    wall = t1 - t0
    res = {"wall_s": round(wall, 2), "reader": round(100 * (s1 - s0) / wall, 3)}
    for k in p1:
        if p0.get(k) is not None and p1.get(k) is not None:
            res[k] = round(100 * (p1[k] - p0[k]) / wall, 3)
    return res


# ---- phase 1: baseline -----------------------------------------------------------------------
time.sleep(3)
main_before = sum(main.out_lines.values())
a = snapshot({})
time.sleep(base_s)
b = snapshot({})
result["baseline_cpu"] = cpu_pct(a, b)
print("baseline", result["baseline_cpu"], flush=True)


# ---- the candidates --------------------------------------------------------------------------
class View:
    """One open view. conn: the view's control client. Keys go to pane ids only."""

    def __init__(self):
        self.conn = Conn("view", "no-output,ignore-size")
        self.conn.call("refresh-client -C 160x48")
        self.extra = {"view-client": self.conn.p.pid}
        self.frames = 0
        self.frame_changes = 0
        self.fifo_bytes = 0
        self.pane = None
        self.stop = threading.Event()

    @staticmethod
    def server_children():
        return subprocess.run(["pgrep", "-P", str(server_pid)], capture_output=True, text=True).stdout.split()

    def first_frame(self, pane):
        lines, t1, err = self.conn.call(f"capture-pane -e -p -t {pane}")
        cur, t2, _ = self.conn.call(f"display -p -t {pane} '#{{cursor_x}} #{{cursor_y}}'")
        return {"capture_s": t1, "cursor_s": t2, "bytes": sum(len(x) + 1 for x in lines or []), "cursor": cur}

    def open(self, pane, others):
        self.pane = pane
        self.conn.watch = pane
        ff = self.first_frame(pane)
        if cand == "a":
            self.conn.call(f"refresh-client -A '{pane}:on'")
            self.conn.call(f"refresh-client -A '{pane}:continue'")
        elif cand == "b":
            # Clearing no-output resets every pane's on/off state, so it comes first, and the
            # whole line runs in one server loop pass: no pane output can slip in between.
            cmds = ["refresh-client -f '!no-output'"] + [f"refresh-client -A '{o}:off'" for o in others]
            self.conn.call_many(cmds + [f"refresh-client -A '{pane}:on'"])
        elif cand == "c":
            fifo = os.path.join(run_dir, f"view-{pane[1:]}.fifo")
            os.mkfifo(fifo, 0o600)
            self.fifo_th = threading.Thread(target=self.read_fifo, args=(fifo,), daemon=True)
            self.fifo_th.start()
            before = set(self.server_children())
            self.conn.call(f"pipe-pane -O -t {pane} 'exec cat > {fifo}'")
            time.sleep(0.3)
            for i, p in enumerate(sorted(set(self.server_children()) - before)):
                self.extra[f"pipe-{i}"] = int(p)
        elif cand == "d":
            self.last = None
            self.tick_th = threading.Thread(target=self.ticker, daemon=True)
            self.tick_th.start()
        return ff

    def close(self):
        self.stop.set()
        if cand == "a":
            self.conn.call(f"refresh-client -A '{self.pane}:off'")
        elif cand == "b":
            self.conn.call("refresh-client -f no-output")
            self.conn.call(f"refresh-client -A '{self.pane}:off'")
        elif cand == "c":
            self.conn.call(f"pipe-pane -t {self.pane}")
        time.sleep(0.3)
        self.stop = threading.Event()

    def read_fifo(self, fifo):
        fd = os.open(fifo, os.O_RDONLY)
        while True:
            data = os.read(fd, 65536)
            if not data:
                break
            self.fifo_bytes += len(data)
            self.conn.bump(data.count(b"Z"), now())
        os.close(fd)

    def ticker(self):
        busy = threading.Event()
        stop = self.stop

        def got(lines, err, t):
            self.frames += 1
            frame = "\n".join(lines)
            if frame != self.last:
                self.last = frame
                self.frame_changes += 1
                self.conn.set_z(frame.count("Z"), t)
            busy.clear()

        nxt = now()
        while not stop.is_set():
            if not busy.is_set():
                busy.set()
                self.conn.send(f"capture-pane -e -p -t {self.pane}", got)
            nxt += TICK
            time.sleep(max(0.0, nxt - now()))


panes = subprocess.run([tmux, "-L", sock, "-f", "/dev/null", "list-panes", "-s", "-t", "=duohive", "-F", "#{pane_id}"],
                       capture_output=True, text=True).stdout.split()
view = View()

# ---- phase 2: echo ---------------------------------------------------------------------------
result["first_frame_echo"] = view.open(echo_pane, [p for p in panes if p != echo_pane])
time.sleep(0.5)
c = view.conn
lat = []
missed = 0
for i in range(KEYS):
    with c.z_cond:
        target = c.z + 1
    t0 = now()
    c.send(f"send-keys -t {echo_pane} -l Z")
    with c.z_cond:
        c.z_cond.wait_for(lambda: c.z >= target, timeout=2.0)
        seen = [t for (n, t) in c.z_times if n >= target]
    if not seen:
        missed += 1
        if cand == "a" and missed == 3:
            break  # not honoured; no point sending 200
        continue
    lat.append(seen[0] - t0)
    time.sleep(random.uniform(0.02, 0.06))
lines, _, _ = c.call(f"capture-pane -p -t {echo_pane}")
result["echo"] = {"keys_sent": i + 1, "echoes_seen": len(lat), "missed": missed,
                  "z_on_screen": sum(x.count("Z") for x in lines or [])}
if lat:
    s = sorted(lat)
    q = lambda f: round(1000 * s[min(len(s) - 1, int(f * len(s)))], 2)
    result["echo"].update(p50_ms=q(0.50), p95_ms=q(0.95), max_ms=round(1000 * s[-1], 2), min_ms=round(1000 * s[0], 2))
with open(prefix + "-latency.txt", "w") as f:
    f.write("\n".join(f"{x * 1000:.3f}" for x in lat) + "\n")
print("echo", result["echo"], flush=True)
view.close()

# ---- phase 3: noisy view ---------------------------------------------------------------------
result["echo_conn_output_lines"] = dict(c.out_lines)
if cand != "a" or lat:
    c.out_lines.clear()
    main_mark = dict(main.out_lines)
    result["first_frame_noisy"] = view.open(noisy_pane, [p for p in panes if p != noisy_pane])
    time.sleep(1.0)
    frames0, changes0 = view.frames, view.frame_changes
    a = snapshot(view.extra)
    time.sleep(view_s)
    b = snapshot(view.extra)
    result["view_cpu"] = cpu_pct(a, b)
    result["view_frames"] = {"captures": view.frames - frames0, "changed": view.frame_changes - changes0}
    result["view_conn_output_lines"] = dict(c.out_lines)
    result["view_conn_other_panes"] = sum(n for p, n in c.out_lines.items() if p != noisy_pane)
    result["fifo_bytes"] = view.fifo_bytes
    view.close()
    print("view", result["view_cpu"], flush=True)
else:
    result["view_cpu"] = None

result["main_output_lines"] = sum(main.out_lines.values()) - main_before
result["main_output_by_pane"] = dict(main.out_lines)

# ---- phase 4: capture cost (S3.6) ------------------------------------------------------------
cost = {}
for name, cmd in (("visible", f"capture-pane -e -p -t {hist_pane}"),
                  ("history2000", f"capture-pane -e -p -S -2000 -t {hist_pane}")):
    times, sizes = [], []
    s0 = ps_time(server_pid)
    t0 = now()
    for _ in range(50):
        lines, dt, err = c.call(cmd)
        times.append(dt)
        sizes.append(sum(len(x.encode()) + 1 for x in lines or []))
    wall = now() - t0
    s1 = ps_time(server_pid)
    st = sorted(times)
    cost[name] = {"p50_ms": round(1000 * st[25], 2), "p95_ms": round(1000 * st[47], 2), "max_ms": round(1000 * st[-1], 2),
                  "bytes": sizes[-1], "lines": len(lines or []),
                  "server_cpu_ms_per_call": round(1000 * (s1 - s0) / 50, 2) if s0 is not None and s1 is not None else None}
result["capture_cost"] = cost
result["errors"] = {"main": main.errors[:5], "view": c.errors[:5]}
stop_poll.set()
with open(prefix + "-result.json", "w") as f:
    json.dump(result, f, indent=1)
print("capture", cost, flush=True)
view.conn.close()
main.close()
