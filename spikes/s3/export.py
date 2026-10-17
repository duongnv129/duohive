#!/usr/bin/env python3
"""Throwaway spike S3.4: copy the recordings (out/rec2/) into fixtures/ for Task 2.4.

Byte-for-byte, except the user's OpenCode model and provider names, replaced by strings of
the same length so every cursor move in the stream stays right. Nothing else is changed.
The names to replace come from DUOHIVE_EXPORT_SWAPS ("old=new;old=new"), never from this
file, so the real ones stay out of the repository.
usage: DUOHIVE_EXPORT_SWAPS='...' export.py [src] [dst]
"""
import os
import shutil
import sys

here = os.path.dirname(os.path.abspath(__file__))
src = sys.argv[1] if len(sys.argv) > 1 else os.path.join(here, "out", "rec2")
dst = sys.argv[2] if len(sys.argv) > 2 else os.path.join(here, "fixtures")
SWAPS = [
    tuple(pair.split("=", 1)[i].encode() for i in (0, 1))
    for pair in os.environ.get("DUOHIVE_EXPORT_SWAPS", "").split(";")
    if "=" in pair
]
for a, b in SWAPS:
    assert len(a) == len(b)
for root, _dirs, files in os.walk(src):
    rel = os.path.relpath(root, src)
    if rel == "." :
        continue
    os.makedirs(os.path.join(dst, rel), exist_ok=True)
    for f in files:
        data = open(os.path.join(root, f), "rb").read()
        for a, b in SWAPS:
            data = data.replace(a, b)
        open(os.path.join(dst, rel, f), "wb").write(data)
print("exported", src, "->", dst)
