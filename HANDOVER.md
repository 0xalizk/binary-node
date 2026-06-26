# HANDOVER — pir-ubt-node binary-node bring-up (snapshot: 2026-06-26 ~20:00 UTC)

You're continuing the binary-node deployment on **pir-ubt-node**. Read `~/sharded-pir/CLAUDE.md`
first for orientation; this file is the **live state + exact next steps**. **Everything
below is in flight — verify current state before acting.**

## State right now (verify each)
- **Mainnet CL (`lighthouse-bn`):** ✅ checkpoint-synced. May be a few hundred slots behind
  while mainnet EL is stopped — will catch up once EL restarts.
- **Mainnet EL (`ethrex`):** ⚠️ **STOPPED** (systemctl stop ethrex) so backfill-bodies can hold
  the RocksDB lock. Restart it as soon as backfill finishes.
- **Backfill-bodies:** ⏳ **RUNNING** in screen session `backfill8` (log `/tmp/backfill8.log`).
  Downloading block bodies 25,340,001–25,401,794 from mainnet p2p into the mainnet EL RocksDB.
  Last known position: block ~25,364,558 / 25,401,794 (~37/120 chunks done at ~1.3 min/chunk).
  Monitor: `grep 'backfill:.*done' /tmp/backfill8.log | tail -5`
  Done when: `grep 'BACKFILL_EXIT:0' /tmp/backfill8.log` or
  `grep 'complete' /tmp/backfill8.log`
- **Binary-node datadir** `~/sharded-pir/binary-node/bn-datadir`: ✅ rsync complete (406 G,
  byte-exact). Checkpoint block 25,340,000.
- **ethrex fork binary:** ✅ built (with bug fixes, see below) →
  `~/sharded-pir/binary-node/ethrex/target/release/ethrex`
  Branch `feat/migrate-seed-and-catchup`.
- **`host` command:** ✅ SSHes to `0xalizk@pir-ubt-node` via `/workspace/utils/host_key`.

## DO THIS NEXT — exact sequence

### 1 — Wait for backfill to finish
```bash
# Monitor:
host "grep 'backfill:.*done' /tmp/backfill8.log | tail -5"
# Done when this appears:
host "grep -E 'BACKFILL_EXIT|complete' /tmp/backfill8.log"
```

### 2 — Restart mainnet EL
```bash
host sudo systemctl start ethrex
```

### 3 — Verify bodies are present at the gap boundary
```bash
host "curl -s -X POST 127.0.0.1:8545 -H 'content-type: application/json' \
  -d '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_getBlockByNumber\",\"params\":[\"0x182a860\",false]}' \
  | python3 -m json.tool | head -5"
```
Must return a block object (not null). `0x182a860` = 25,340,000.

### 4 — Validate catch-up on 10 blocks first (NEVER run end-to-end before)
```bash
host "ulimit -n 1048576 && \
  ~/sharded-pir/binary-node/ethrex/target/release/ethrex \
    --datadir ~/sharded-pir/binary-node/bn-datadir --network mainnet \
    catch-up http://127.0.0.1:8545 --to 25340010"
```
Confirm latest advances to 25,340,010, no errors.

### 5 — Full catch-up loop
Use the committed script:
```bash
host "bash ~/sharded-pir/binary-node/catch-up-loop.sh"
```
Or manually:
```bash
host 'while true; do
  ulimit -n 1048576
  ~/sharded-pir/binary-node/ethrex/target/release/ethrex \
    --datadir ~/sharded-pir/binary-node/bn-datadir --network mainnet \
    catch-up http://127.0.0.1:8545
  sleep 30
done'
```

### 6 — Launch binary-node as service (after catch-up reaches near-tip)
```bash
host "cp ~/sharded-pir/binary-node/binary-node.service ~/.config/systemd/user/"
host "systemctl --user daemon-reload && systemctl --user enable --now binary-node"
```
Distinct ports: http 8645 / authrpc 8651 / metrics 9190. `--p2p.disabled`.

### 7 — Step 7: feeder + equiv-daemon + Grafana (NOT BUILT)
The per-block binary↔MPT value-compare loop. Design in `binary-node/docs/architecture.md`.

## What was done this session (2026-06-26)

### Bug fixes in `feat/migrate-seed-and-catchup` (ethrex fork)

Three bugs prevented `backfill-bodies` from connecting to mainnet p2p peers. All fixed:

**`crates/storage/backend/rocksdb.rs`** (opening mainnet EL RocksDB with v9 fork binary):
- WAL recovery mode changed from `PointInTime` to `TolerateCorruptedTailRecords` — v16 uses
  WAL-disabled writes for some CFs; `PointInTime` incorrectly rejected them.
- CF open logic: open only `existing_cfs` (not TABLES) so binary-trie-specific CFs are not
  created in the mainnet EL DB (would prevent v16 from reopening).
- `passthrough_merge` function + applied to unknown CFs — v16's `block_access_lists` CF uses
  merge operators; opening without one fails WAL replay.
- Removed the "clean up obsolete CFs" drop loop that would have destroyed v16-only CFs.

**`cmd/ethrex/backfill.rs`** — p2p peers connected but Status handshake failed:
- `open_store_unchecked` initializes `chain_config` to `Default` (chain_id=0). Every peer's
  Status message carries chain_id=1; `validate_status` rejected all of them.
- Fix: call `store.add_initial_state(genesis)` (using `network.get_genesis()`) instead of
  `set_chain_config`. This also warms `latest_block_header` from the DB so the outgoing Status
  message carries the correct head block, not block 0.

**`crates/storage/store.rs`** — genesis hash mismatch in `validate_status`:
- `get_block_header(0)` short-circuits to the `latest_block_header` cache. The cache defaults
  to a zeroed block header (number=0, all fields zero). Its hash != mainnet genesis hash.
- `add_initial_state` fixed the `chain_config` and `latest_block_header`, but only AFTER the
  cache was checked for block 0.
- Fix: added `block_number != 0` guard so genesis always reads from DB (where the correct hash
  `0xd4e56740...` is stored by the snap-synced v16 binary).

### Other work
- Committed `binary-node.service`, `catch-up-loop.sh`, `catch-up-validate.sh` to
  `github.com/0xalizk/binary-node` main (commit `f928eed`).

## Gotchas / safety
- **FD limit:** `ulimit -n 1048576` for any ethrex op on either datadir.
- **Mainnet EL must be STOPPED** while backfill-bodies runs (exclusive RocksDB lock).
- **Don't restart mainnet EL until backfill is done** — backfill will crash and you'll need
  to restart it.
- Dedicated NVMe box (no shared-disk contention). Mainnet pair is live — watch CL
  `sync_distance` (`curl -s 127.0.0.1:5052/eth/v1/node/syncing`).
- catch-up requires the local mainnet EL synced AND retaining block bodies back to 25,340,000
  (hence the backfill).

## Repos / references
- `~/sharded-pir/binary-node` (`github.com/0xalizk/binary-node`, `main`).
- ethrex fork: `~/sharded-pir/binary-node/ethrex` (`feat/migrate-seed-and-catchup`,
  `github.com/0xalizk/ethrex`).
- Runbook: `binary-node/docs/replication.md` · Design: `docs/design-rationale.md` ·
  Step-7 design: `docs/architecture.md` · Code audit: `docs/audit_1_24_06.md` ·
  Journal: `binary-node/planning/`.
- Scripts: `binary-node/catch-up-loop.sh`, `binary-node/catch-up-validate.sh`,
  `binary-node/binary-node.service`.
