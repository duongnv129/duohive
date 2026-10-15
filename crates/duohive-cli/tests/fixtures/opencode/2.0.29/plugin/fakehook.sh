#!/bin/sh
# fake duohive: <exe> hook <agent> <event>, JSON on stdin
body=$(cat)
python3 -I -c 'import json,sys,time; print(json.dumps({"t":int(time.time()*1000),"agent":sys.argv[1],"event":sys.argv[2],"payload":json.loads(sys.argv[3] or "{}")}))' "$2" "$3" "$body" >> "$FAKEHOOK_LOG"
