#!/usr/bin/env python3
"""Throwaway: which close line a control client gets for each way a window goes away.
usage: close.py <tmux-binary>"""
import subprocess, sys, time
tmux = sys.argv[1]
sock = f"duohive-s0-close-{__import__('os').getpid()}"
t = lambda *a: subprocess.run([tmux, "-L", sock, *a], capture_output=True, text=True).stdout.strip()
try:
    t("new-session", "-d", "-s", "duohive", "-n", "keeper", "tail -f /dev/null")
    t("new-session", "-d", "-s", "other", "-n", "o", "tail -f /dev/null")
    c = subprocess.Popen([tmux, "-L", sock, "-C", "attach", "-t", "=duohive"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    c.stdin.write("refresh-client -f no-output,ignore-size\n"); c.stdin.flush()
    time.sleep(0.3)
    a = t("new-window", "-d", "-P", "-F", "#{window_id}", "-t", "=duohive", "sleep 1")      # process exits, remain-on-exit off
    b = t("new-window", "-d", "-P", "-F", "#{window_id}", "-t", "=duohive", "tail -f /dev/null")
    d = t("new-window", "-d", "-P", "-F", "#{window_id}", "-t", "=duohive", "tail -f /dev/null")
    e = t("new-window", "-d", "-P", "-F", "#{window_id}", "-t", "=other", "tail -f /dev/null")
    time.sleep(1.5)
    t("kill-window", "-t", b)                                     # external kill-window
    c.stdin.write(f"kill-window -t {d}\n"); c.stdin.flush()       # kill-window from the control client
    t("kill-window", "-t", e)                                     # window in another session
    time.sleep(0.5)
    print(f"# {t('-V') or ''} exits={a} external-kill={b} control-kill={d} other-session={e}")
    t("kill-server")
    for line in c.stdout:
        if not line.startswith(("%begin", "%end", "%output")):
            print(line.rstrip())
finally:
    t("kill-server")
    __import__("os").system(f"rm -f /private/tmp/tmux-$(id -u)/{sock}")  # 3.2a leaves its socket file
