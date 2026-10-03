#!/usr/bin/env python3
"""Throwaway: can one pane stream while the client has no-output? usage: selected.py <tmux-binary>"""
import subprocess, sys, time, os
tmux = sys.argv[1]; sock = f"duohive-s0-sel-{os.getpid()}"
t = lambda *a: subprocess.run([tmux, "-L", sock, *a], capture_output=True, text=True).stdout.strip()
try:
    t("new-session", "-d", "-s", "duohive", "while :; do date; sleep 0.1; done")
    t("new-window", "-d", "while :; do date; sleep 0.1; done")
    c = subprocess.Popen([tmux, "-L", sock, "-C", "attach", "-t", "=duohive"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    def send(x): c.stdin.write(x + "\n"); c.stdin.flush(); time.sleep(1.0); print("> " + x)
    send("refresh-client -f no-output,ignore-size")
    send("refresh-client -A '%0:on'")
    send("refresh-client -A '%0:continue'")
    send("refresh-client -f '!no-output'")
    send("refresh-client -A '%1:pause'")
    send("display-message -p '#{client_flags}'")
    t("kill-server")
    seen = []
    import collections
    for line in c.stdout:
        if line.startswith("%output") or line.startswith("%extended-output"):
            seen.append(line.split()[1])
        else:
            if seen: print("   output:", dict(collections.Counter(seen))); seen = []
            if not line.startswith(("%end",)): print("   " + line.rstrip())
finally:
    t("kill-server"); os.system(f"rm -f /private/tmp/tmux-$(id -u)/{sock}")
