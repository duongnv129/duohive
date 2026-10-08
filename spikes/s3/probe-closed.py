#!/usr/bin/env python3
"""Throwaway spike S3: candidate b's edge cases, on the viewer connection.

usage: probe-closed.py <tmux> (starts and kills its own -L server)
1. A window added while viewing: does %window-add arrive, does its pane stream until it is
   turned off, and does `refresh-client -A '%N:off'` stop it?
2. The viewed window killed: which notification arrives, and does send-keys to its pane id
   fail with %error (so keys never reach a newer window)?
"""
import os
import subprocess
import sys
import threading
import time

tmux = sys.argv[1]
sock = f"duohive-s3-closed-{os.getpid()}"
T = [tmux, "-L", sock, "-f", "/dev/null"]
log = []


def reader(p):
    for raw in p.stdout:
        log.append((time.monotonic(), raw.decode("utf-8", "replace").rstrip("\n")))


subprocess.run(T + ["new-session", "-d", "-s", "duohive", "-n", "keeper", "-x", "80", "-y", "24", "tail -f /dev/null"], check=True)
try:
    subprocess.run(T + ["set-option", "-g", "remain-on-exit", "on"], check=True)
    subprocess.run(T + ["new-window", "-d", "-n", "viewed", "while :; do date; sleep 0.1; done"], check=True)
    ids = dict(reversed(l.split()) for l in subprocess.run(
        T + ["list-panes", "-s", "-t", "=duohive", "-F", "#{pane_id} #{window_name}"], capture_output=True, text=True).stdout.split("\n") if l)
    wid = subprocess.run(T + ["display", "-p", "-t", "=duohive:viewed", "#{window_id}"], capture_output=True, text=True).stdout.strip()
    pane = ids["viewed"]
    p = subprocess.Popen(T + ["-C", "attach-session", "-f", "no-output,ignore-size", "-t", "=duohive"],
                         stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    threading.Thread(target=reader, args=(p,), daemon=True).start()

    def send(line):
        p.stdin.write((line + "\n").encode())
        p.stdin.flush()

    send(" ; ".join(["refresh-client -f '!no-output'", f"refresh-client -A '{ids['keeper']}:off'",
                     f"refresh-client -A '{pane}:on'", f"capture-pane -e -N -p -t {pane}"]))
    time.sleep(1)
    subprocess.run(T + ["new-window", "-d", "-n", "late", "while :; do date; sleep 0.1; done"], check=True)
    time.sleep(1)
    late = subprocess.run(T + ["display", "-p", "-t", "=duohive:late", "#{pane_id}"], capture_output=True, text=True).stdout.strip()
    t_off = time.monotonic()
    send(f"refresh-client -A '{late}:off'")
    time.sleep(1.5)
    adds = [l for _, l in log if l.startswith("%window-add")]
    late_before = sum(1 for t, l in log if l.startswith(f"%output {late} ") and t < t_off)
    late_after = sum(1 for t, l in log if l.startswith(f"%output {late} ") and t > t_off + 0.05)
    print(f"{tmux}: window-add lines {adds}; late pane {late}: {late_before} output lines before off, {late_after} after")

    t_kill = time.monotonic()
    subprocess.run(T + ["kill-window", "-t", wid], check=True)
    time.sleep(0.5)
    send(f"send-keys -t {pane} -l Z")
    time.sleep(0.5)
    after = [l for t, l in log if t > t_kill and not l.startswith("%output")]
    print(f"{tmux}: after kill-window {wid}: {after}")
    p.stdin.close()
    p.terminate()
finally:
    subprocess.run(T + ["kill-server"], capture_output=True)
    for d in (f"/private/tmp/tmux-{os.getuid()}", f"/tmp/tmux-{os.getuid()}"):
        try:
            os.unlink(os.path.join(d, sock))
        except OSError:
            pass
