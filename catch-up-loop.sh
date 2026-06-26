#!/bin/bash
# Full catch-up loop — runs AFTER the 10-block validation passes.
# Keeps re-running catch-up (crash-safe / resumes from last checkpoint) until
# the operator kills it or we're near the tip.
#
# Usage:
#   Phase 1 (ubt-node tunnel, for 25,340,001 → near tip):
#     RPC_URL=http://127.0.0.1:8545 bash catch-up-loop.sh
#   Phase 2 (local EL, for the last few hundred blocks once near tip):
#     RPC_URL=http://127.0.0.1:8545 bash catch-up-loop.sh
#
# Keep the tunnel open in a separate screen/tmux:
#   tsh ssh -L 8555:127.0.0.1:8545 0xalizk@ubt-node
set -euo pipefail

RPC_URL="${RPC_URL:-http://127.0.0.1:8545}"
ETHREX=~/sharded-pir/binary-node/ethrex/target/release/ethrex
DATADIR=~/sharded-pir/binary-node/bn-datadir

echo "catch-up-loop: source=$RPC_URL  binary=$ETHREX"

# Raise FD limit; fall back to sudo prlimit if hard limit is too low.
FD_OK=0
ulimit -n 1048576 2>/dev/null && FD_OK=1
if [[ $FD_OK -eq 0 ]]; then
  echo "ulimit -n 1048576 failed (hard limit=$(ulimit -Hn)); will use sudo prlimit per run."
fi

run_catchup() {
  if [[ $FD_OK -eq 1 ]]; then
    "$ETHREX" --datadir "$DATADIR" --network mainnet catch-up "$RPC_URL"
  else
    sudo prlimit --nofile=1048576:1048576 \
      "$ETHREX" --datadir "$DATADIR" --network mainnet catch-up "$RPC_URL"
  fi
}

attempt=0
while true; do
  attempt=$((attempt + 1))
  echo "[$(date -u +%FT%TZ)] attempt #$attempt — running catch-up against $RPC_URL"
  run_catchup && status=0 || status=$?
  echo "[$(date -u +%FT%TZ)] catch-up exited (status=$status)"
  if [[ $status -eq 0 ]]; then
    echo "catch-up reached tip cleanly — re-running to close the gap."
  else
    echo "catch-up crashed (status=$status) — sleeping 10s then resuming."
    sleep 10
  fi
done
