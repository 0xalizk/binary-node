#!/usr/bin/env bash
# Emits a heartbeat every ~10 min and exits when EL+CL are both fully synced, or on fatal error.
i=0
while true; do
  i=$((i+1))
  # CL status
  cl=$(curl -s -m 8 http://127.0.0.1:5052/eth/v1/node/syncing)
  cl_syncing=$(echo "$cl" | grep -o '"is_syncing":[a-z]*' | cut -d: -f2)
  cl_dist=$(echo "$cl" | grep -o '"sync_distance":"[0-9]*"' | grep -o '[0-9]*')
  # EL status
  els=$(curl -s -m 8 -X POST -H 'content-type: application/json' \
        --data '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' http://127.0.0.1:8545)
  blk=$(curl -s -m 8 -X POST -H 'content-type: application/json' \
        --data '{"jsonrpc":"2.0","method":"eth_getBlockByNumber","params":["latest",false],"id":1}' http://127.0.0.1:8545)
  el_num_hex=$(echo "$blk" | grep -o '"number":"0x[0-9a-f]*"' | head -1 | grep -o '0x[0-9a-f]*')
  el_ts_hex=$(echo "$blk" | grep -o '"timestamp":"0x[0-9a-f]*"' | head -1 | grep -o '0x[0-9a-f]*')
  el_num=$([ -n "$el_num_hex" ] && printf '%d' "$el_num_hex" 2>/dev/null || echo 0)
  el_ts=$([ -n "$el_ts_hex" ] && printf '%d' "$el_ts_hex" 2>/dev/null || echo 0)
  now=$(date -u +%s)
  lag=$((now - el_ts))
  el_syncing_false=$(echo "$els" | grep -qE '"result":false' && echo yes || echo no)

  # Fatal error check
  if sudo journalctl -u ethrex.service -u lighthouse-bn.service --since "-2min" --no-pager --output=cat 2>/dev/null | grep -qiE "panic|CRIT|fatal|stack backtrace"; then
    echo "ERROR: fatal log line detected in EL/CL within last 2min — investigate"
  fi

  # Done condition: EL not syncing AND head within ~60s of now AND CL not syncing
  if [ "$el_syncing_false" = "yes" ] && [ "$el_ts" -gt 0 ] && [ "$lag" -lt 60 ] && [ "$cl_syncing" = "false" ]; then
    echo "DONE: fully synced — EL block $el_num (head lag ${lag}s), CL sync_distance ${cl_dist:-?}"
    exit 0
  fi

  # Heartbeat every 10th poll (~10 min at 60s interval)
  if [ $((i % 10)) -eq 1 ]; then
    echo "SYNC: EL block=${el_num} headlag=${lag}s el_syncing_false=${el_syncing_false} | CL syncing=${cl_syncing:-?} dist=${cl_dist:-?}"
  fi
  sleep 60
done
