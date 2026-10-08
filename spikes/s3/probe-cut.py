#!/usr/bin/env python3
"""Throwaway spike S3: is candidate b's first frame an exact cut of its stream?

usage: probe-cut.py <tmux> (starts and kills its own -L server)
A pane prints numbered lines, about 1,500 a second. The viewer sends the open line of S3.8 (clear
no-output, others off, the pane on, display cursor, capture-pane -e -N -p) 20 times, closing in
between. For each: the last number on the capture must be the last number in the %output data
before the capture's %begin, and the first %output after its %end must continue from it.
"""
import os
import re
import subprocess
import sys
import time

tmux = sys.argv[1]
sock = f"duohive-s3-cut-{os.getpid()}"
T = [tmux, "-L", sock, "-f", "/dev/null"]
subprocess.run(T + ["new-session", "-d", "-s", "duohive", "-n", "keeper", "-x", "80", "-y", "24", "tail -f /dev/null"], check=True)
try:
    subprocess.run(T + ["new-window", "-d", "-n", "fast", "perl -e '$|=1; $i=0; while (1) { print \"L\", ++$i, \"\\n\"; select(undef, undef, undef, 0.0005) }'"], check=True)
    subprocess.run(T + ["new-window", "-d", "-n", "noisy", "while :; do date; sleep 0.1; done"], check=True)
    panes = subprocess.run(T + ["list-panes", "-s", "-t", "=duohive", "-F", "#{pane_id} #{window_name}"],
                           capture_output=True, text=True).stdout.split("\n")
    pane = [p.split()[0] for p in panes if p.endswith(" fast")][0]
    others = [p.split()[0] for p in panes if p and not p.endswith(" fast")]
    p = subprocess.Popen(T + ["-C", "attach-session", "-f", "no-output,ignore-size", "-t", "=duohive"],
                         stdin=subprocess.PIPE, stdout=subprocess.PIPE)

    def unescape(s):
        return re.sub(r"\\([0-7]{3})", lambda m: chr(int(m.group(1), 8)), s)

    ok = bad = 0
    for trial in range(20):
        cmds = ["refresh-client -f '!no-output'"] + [f"refresh-client -A '{o}:off'" for o in others]
        cmds += [f"refresh-client -A '{pane}:on'",
                 f"display -p -t {pane} '#{{cursor_x}} #{{cursor_y}}'",
                 f"capture-pane -e -N -p -t {pane}"]
        p.stdin.write((" ; ".join(cmds) + "\n").encode())
        p.stdin.flush()
        blocks = 0
        before, after, cap, block = "", "", None, None
        deadline = time.time() + 5
        while time.time() < deadline:
            line = p.stdout.readline().decode("utf-8", "replace").rstrip("\n")
            if block is not None:
                if line.startswith("%end") or line.startswith("%error"):
                    blocks += 1
                    if len(block) == 24:  # the capture-pane -N reply: one line per row
                        cap = block
                    block = None
                else:
                    block.append(line)
                continue
            if line.startswith("%begin"):
                block = []
            elif line.startswith(f"%output {pane} "):
                data = unescape(line.split(" ", 2)[2])
                if cap is None:
                    before += data
                else:
                    after += data
                    if len(after) > 200:
                        break
        nums_cap = re.findall(r"L(\d+)", "\n".join(cap or []))
        last_cap = int(nums_cap[-1]) if nums_cap else None
        first_after = re.match(r"\r?\n?L?(\d+)", after)
        nums_before = re.findall(r"L(\d+)", before)
        # The line in progress at the cut may be split: the capture shows its digits so far.
        cont = after.lstrip("\r\n")
        good = last_cap is not None and (not nums_before or int(nums_before[-1]) <= last_cap) and (
            cont.startswith(f"L{last_cap + 1}") or re.match(r"^\d*\r?\n", cont) is not None)
        print(f"trial {trial}: capture ends L{last_cap}, before-stream ends "
              f"L{nums_before[-1] if nums_before else '-'}, after-stream starts {after[:14]!r} -> {'exact' if good else 'NOT exact'}")
        ok += good
        bad += not good
        p.stdin.write(b"refresh-client -f no-output\n")
        p.stdin.flush()
        time.sleep(0.2)
        # Drain whatever came after the close (its reply, and output queued before it).
        os.set_blocking(p.stdout.fileno(), False)
        try:
            while p.stdout.read(65536):
                pass
        except (BlockingIOError, TypeError):
            pass
        os.set_blocking(p.stdout.fileno(), True)
    print(f"{tmux}: exact {ok}/20")
    p.stdin.close()
    p.terminate()
finally:
    subprocess.run(T + ["kill-server"], capture_output=True)
    for d in (f"/private/tmp/tmux-{os.getuid()}", f"/tmp/tmux-{os.getuid()}"):
        try:
            os.unlink(os.path.join(d, sock))
        except OSError:
            pass
