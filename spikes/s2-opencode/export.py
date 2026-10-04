#!/usr/bin/env python3
"""Copy the spike's raw captures into the committed fixtures, redacted.

Home paths become /HOME, and the user name and emails are replaced. Hook payloads keep
their structure byte for byte otherwise. The plugin's full event log is kept as
hooks/events.jsonl without the high-volume `message.part.delta` and `plugin.added`
entries. Replies: the `duohive_last_reply` the plugin attached at each `session.idle`,
one file per turn, plus a1's turns derived offline from the event log by the same rule
(a1 ran the first plugin version, which did not attach replies), and the
`opencode export` of a1's session.
"""
import json
import os
import pathlib
import re
import shutil

RAW = pathlib.Path("/tmp/duohive-spike/raw-oc")
# Review-fix runs (a5 to a7): the plugin's config-hook composer and the early paste.
RAW2 = pathlib.Path("/tmp/duohive-spike/raw-oc2")
EXPORT_A1 = pathlib.Path("/tmp/duohive-spike/s2/export-a1.json")
OUT = pathlib.Path(__file__).resolve().parents[2] / "crates/duohive-cli/tests/fixtures/opencode/1.18.34"
HOME = os.path.expanduser("~")
USER = os.environ["USER"]
DROP = {"message.part.delta", "plugin.added"}


def redact(text: str) -> str:
    text = text.replace(HOME, "/HOME")
    # The provider's display name from the user's config carries an organisation name;
    # keep the line width so the screens stay aligned.
    text = re.sub(r"Example \(\w+\)", lambda m: "(Example)".ljust(len(m.group(0))), text)
    text = re.sub(r"[\w.+-]+@[\w-]+\.[\w.]+", "user@example.com", text)
    text = text.replace(USER, "user")
    return text


def replies_from_events(lines):
    """The reply rule on the raw event stream: at each session.idle, the text parts
    (non-synthetic, ended) of the last assistant message with text since the last
    user message of that session."""
    parts, last, out = {}, {}, []
    for line in lines:
        e = json.loads(line)
        p = e.get("properties", {})
        t = e.get("type")
        if t == "message.updated" and p.get("info", {}).get("role") == "user":
            last.pop(p["info"]["sessionID"], None)
        elif t == "message.part.updated":
            part = p["part"]
            if part["type"] == "text" and part.get("time", {}).get("end") and not part.get("synthetic"):
                parts.setdefault(part["messageID"], {})[part["id"]] = part["text"]
                last[part["sessionID"]] = part["messageID"]
        elif t == "session.idle":
            mid = last.get(p["sessionID"])
            out.append((p["sessionID"], "".join(parts[mid].values()) if mid else None))
    return out


def main() -> None:
    if OUT.exists():
        shutil.rmtree(OUT)
    for sub in ("hooks", "screens", "replies"):
        (OUT / sub).mkdir(parents=True)
    for f in sorted((RAW / "hooks").glob("*.json")):
        (OUT / "hooks" / f.name).write_text(redact(f.read_text()))
    (OUT / "hooks" / "index.log").write_text(redact((RAW / "hooks" / "index.log").read_text()))
    lines = (RAW / "events.jsonl").read_text().splitlines()
    kept = [l for l in lines if json.loads(l).get("type") not in DROP]
    (OUT / "hooks" / "events.jsonl").write_text(redact("\n".join(kept) + "\n"))
    for f in sorted((RAW / "screens").glob("*.txt")):
        if f.stat().st_size and not f.name.startswith(("startup-with-task-", "startup-a4-probe")):
            (OUT / "screens" / f.name).write_text(redact(f.read_text()))
    # The 0.5 s start-up frames: the last blank one, the prompt pre-filled, then submitted.
    for name in ("startup-with-task-4.txt", "startup-with-task-5.txt", "startup-with-task-6.txt", "startup-with-task-5s.txt"):
        src = RAW / "screens" / name
        (OUT / "screens" / name).write_text(redact(src.read_text()))
    for f in sorted((RAW / "hooks").glob("session.idle.reply-*.json")):
        reply = json.loads(f.read_text())["duohive_last_reply"]
        text = "(null)\n" if reply is None else redact(reply) + "\n"
        (OUT / "replies" / f"{f.stem}.duohive_last_reply.txt").write_text(text)
    a1_session = json.loads((RAW / "hooks" / "session.created-1.json").read_text())["properties"]["sessionID"]
    n = 0
    for sid, reply in replies_from_events(lines):
        if sid != a1_session:
            continue
        n += 1
        text = "(null)\n" if reply is None else redact(reply) + "\n"
        (OUT / "replies" / f"a1-turn-{n}.from-events.txt").write_text(text)
    (OUT / "replies" / "export-a1.json").write_text(redact(EXPORT_A1.read_text()))
    # Review-fix runs, prefixed "review-" so the first runs' numbering is unchanged.
    for f in sorted((RAW2 / "hooks").glob("*.json")):
        (OUT / "hooks" / f"review-{f.name}").write_text(redact(f.read_text()))
    (OUT / "hooks" / "review-index.log").write_text(redact((RAW2 / "hooks" / "index.log").read_text()))
    lines2 = (RAW2 / "events.jsonl").read_text().splitlines()
    kept2 = [l for l in lines2 if json.loads(l).get("type") not in DROP]
    (OUT / "hooks" / "review-events.jsonl").write_text(redact("\n".join(kept2) + "\n"))
    for f in sorted((RAW2 / "screens").glob("*.txt")):
        (OUT / "screens" / f"review-{f.name}").write_text(redact(f.read_text()))


if __name__ == "__main__":
    main()
