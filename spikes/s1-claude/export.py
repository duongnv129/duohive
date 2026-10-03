#!/usr/bin/env python3
"""Copy the spike's raw captures into the committed fixtures, redacted.

Home paths become /HOME, and the user name and email are replaced. Hook payloads keep
their structure byte for byte otherwise. The transcript is cut down to its `user` and
`assistant` entries (the rest carries the user's global context: memory, skills, CLAUDE.md).
"""
import json
import os
import pathlib
import re
import shutil

RAW = pathlib.Path("/tmp/duohive-spike/raw")
OUT = pathlib.Path(__file__).resolve().parents[2] / "crates/duohive-cli/tests/fixtures/claude/2.1.288"
HOME = os.path.expanduser("~")
USER = os.environ["USER"]


def redact(text: str) -> str:
    text = text.replace(HOME, "/HOME")
    # Thinking-block signatures are opaque blobs that embed account and org ids.
    text = re.sub(r'"signature":"[^"]*"', '"signature":"REDACTED"', text)
    text = re.sub(r"[\w.+-]+@[\w-]+\.[\w.]+", "user@example.com", text)
    text = text.replace(USER, "user")
    return text


def main() -> None:
    if OUT.exists():
        shutil.rmtree(OUT)
    for sub in ("hooks", "screens", "replies"):
        (OUT / sub).mkdir(parents=True)
    for f in sorted((RAW / "hooks").glob("*.json")):
        (OUT / "hooks" / f.name).write_text(redact(f.read_text()))
    (OUT / "hooks" / "index.log").write_text(redact((RAW / "hooks" / "index.log").read_text()))
    for f in sorted((RAW / "screens").glob("*.txt")):
        if f.stat().st_size:
            (OUT / "screens" / f.name).write_text(redact(f.read_text()))
    # Replies: the Stop payload's last_assistant_message, one file per turn.
    for f in sorted((RAW / "hooks").glob("Stop-*.json")):
        msg = json.loads(f.read_text())["last_assistant_message"]
        (OUT / "replies" / f"{f.stem}.last_assistant_message.txt").write_text(redact(msg) + "\n")
    # The transcript of session a1 (first SessionStart), user/assistant entries only.
    transcript = json.loads((RAW / "hooks" / "SessionStart-1.json").read_text())["transcript_path"]
    keep = [line for line in open(transcript) if json.loads(line).get("type") in ("user", "assistant")]
    (OUT / "replies" / "transcript-a1.jsonl").write_text(redact("".join(keep)))


if __name__ == "__main__":
    main()
