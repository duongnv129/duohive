#!/usr/bin/env python3
"""Throwaway S0 control-mode reader. Not imported by crates/.

usage: reader.py <tmux> <socket> <mode> <seconds> <log> <pidfile>
mode: no-output | pause | poll

Attaches one control client, silences every pane, subscribes (or polls),
timestamps every line it reads ("<epoch> <line>"; lines it sends are "<epoch> > <cmd>",
events it causes are "<epoch> # <note>"). At t=60 it opens a late noisy window,
at t=70 pauses it (pause mode), at t=90 kills it. At the end it kills the server
and keeps reading until EOF, so %exit is logged.
"""
import os
import resource
import subprocess
import sys
import threading
import time

tmux, sock, mode, seconds, log_path, pid_path = sys.argv[1:7]
seconds = int(seconds)
NOISY = "while :; do date; sleep 0.1; done"

log = open(log_path, "w", buffering=1)
lock = threading.Lock()


def emit(text):
    with lock:
        log.write(f"{time.time():.3f} {text}\n")


def rusage(label):
    r = resource.getrusage(resource.RUSAGE_SELF)
    emit(f"# rusage-{label} {r.ru_utime:.6f} {r.ru_stime:.6f}")


def external(*args):
    out = subprocess.run([tmux, "-L", sock, *args], capture_output=True, text=True)
    return out.stdout.strip()


client = subprocess.Popen(
    [tmux, "-L", sock, "-C", "attach", "-t", "=duohive"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    bufsize=0,
)
with open(pid_path, "a") as f:
    f.write(f"reader {os.getpid()}\nclient {client.pid}\n")


def send(cmd):
    emit(f"> {cmd}")
    client.stdin.write((cmd + "\n").encode())
    client.stdin.flush()


def read_loop():
    for raw in iter(client.stdout.readline, b""):
        emit(raw.rstrip(b"\n").decode("utf-8", "replace"))
    emit("# EOF")


reader = threading.Thread(target=read_loop, daemon=True)
reader.start()

t0 = time.time()
emit(f"# start mode={mode}")
if mode == "pause":
    send("refresh-client -f pause-after=1,ignore-size")
    for pane in external("list-panes", "-s", "-t", "=duohive", "-F", "#{pane_id}").split():
        send(f"refresh-client -A '{pane}:pause'")
else:  # no-output, and poll (silenced the same way, over the same connection)
    send("refresh-client -f no-output,ignore-size")

if mode != "poll":
    send("refresh-client -B 'act:@*:#{window_activity}'")
    send("refresh-client -B 'dead:%*:#{pane_dead} #{pane_dead_status}'")
emit("# subscribed")
rusage("start")

late_window = late_pane = None
done = set()
next_poll = time.time()
while time.time() - t0 < seconds:
    now = time.time()
    t = now - t0
    if mode == "poll" and now >= next_poll:
        send("list-windows -F '#{window_id} #{window_activity} #{pane_dead} #{pane_dead_status}'")
        next_poll += 1.0
    if t >= 60 and "late" not in done and seconds > 90:
        late_window, late_pane = external(
            "new-window", "-d", "-P", "-F", "#{window_id} #{pane_id}", "-n", "late", NOISY
        ).split()
        emit(f"# late-created {late_window} {late_pane}")
        done.add("late")
    if t >= 70 and "late" in done and "late-pause" not in done:
        if mode == "pause":
            send(f"refresh-client -A '{late_pane}:pause'")
        emit(f"# late-pause-point {late_pane}")
        done.add("late-pause")
    if t >= 90 and "late-pause" in done and "late-kill" not in done:
        external("kill-window", "-t", late_window)
        emit(f"# late-killed {late_window}")
        done.add("late-kill")
    time.sleep(0.05)

rusage("end")
emit("# kill-server")
external("kill-server")
reader.join(timeout=5)
client.wait(timeout=5)
emit("# done")
