#!/usr/bin/env bash
# Auto-abort backstop for snapshot-resort: stop it if the live CL falls behind.
LOG=/home/0xalizk/sharded-pir/binary-node/snapshot-resorter/resort-monitor.log
bad=0
while systemctl is-active --quiet snapshot-resort; do
  d=$(curl -s --max-time 10 127.0.0.1:5052/eth/v1/node/syncing \
      | python3 -c "import sys,json;print(json.load(sys.stdin)['data']['sync_distance'])" 2>/dev/null)
  ld=$(cut -d' ' -f1 /proc/loadavg)
  ts=$(date -u +%H:%M:%S)
  if [ -z "$d" ]; then echo "$ts WARN: CL query failed" >> "$LOG"; bad=$((bad+1));
  elif [ "$d" -gt 16 ]; then echo "$ts BAD: sync_distance=$d load=$ld (bad=$bad)" >> "$LOG"; bad=$((bad+1));
  else echo "$ts ok: sync_distance=$d load=$ld" >> "$LOG"; bad=0; fi
  if [ "$bad" -ge 3 ]; then
    echo "$ts ABORTING snapshot-resort: node degraded 3x" >> "$LOG"
    sudo systemctl stop snapshot-resort
    break
  fi
  sleep 60
done
echo "$(date -u +%H:%M:%S) monitor exit (resort no longer active)" >> "$LOG"
