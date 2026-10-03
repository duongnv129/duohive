#!/usr/bin/env python3
"""Throwaway stdio MCP server for spike S1: one tool, `ping`, returns `pong`."""
import json
import sys

TOOL = {"name": "ping", "description": "Reply pong",
        "inputSchema": {"type": "object", "properties": {}}}

for line in sys.stdin:
    msg = json.loads(line)
    mid, method = msg.get("id"), msg.get("method")
    if mid is None:
        continue  # a notification, e.g. notifications/initialized
    if method == "initialize":
        result = {"protocolVersion": msg["params"]["protocolVersion"],
                  "capabilities": {"tools": {}},
                  "serverInfo": {"name": "duohive-dummy", "version": "0.0.1"}}
    elif method == "tools/list":
        result = {"tools": [TOOL]}
    elif method == "tools/call":
        result = {"content": [{"type": "text", "text": "pong"}]}
    else:
        result = {}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": mid, "result": result}) + "\n")
    sys.stdout.flush()
