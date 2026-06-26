#!/bin/bash
# Validate catch-up on 10 blocks FIRST -- it has never completed a block end-to-end.
# Run this after the tsh tunnel is open (8555 → ubt-node:8545).
# Confirms: head advances to 25,340,010; binary-node processes blocks without error.
set -euo pipefail
# Raise FD limit; if the hard limit is < 1048576, use sudo prlimit.
if ! ulimit -n 1048576 2>/dev/null; then
  exec sudo prlimit --nofile=1048576:1048576 \
    ~/sharded-pir/binary-node/ethrex/target/release/ethrex \
    --datadir ~/sharded-pir/binary-node/bn-datadir \
    --network mainnet \
    catch-up http://127.0.0.1:8545 --to 25340010
fi
exec ~/sharded-pir/binary-node/ethrex/target/release/ethrex \
  --datadir ~/sharded-pir/binary-node/bn-datadir \
  --network mainnet \
  catch-up http://127.0.0.1:8545 --to 25340010
