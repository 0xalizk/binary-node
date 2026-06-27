# HANDOVER — pir-ubt-node binary-node bring-up (snapshot: 2026-06-27 ~10:00 UTC)

You're continuing the binary-node deployment on **pir-ubt-node**. Read `~/sharded-pir/CLAUDE.md`
first for orientation; this file is the **live state + exact next steps**. **Everything
below is in flight — verify current state before acting.**

## State right now (verify each)
- **Mainnet pair (ethrex EL + lighthouse CL):** ✅ both up and synced (`sync_distance` ~1).
  EL on `:8545`. This is the block source + MPT reference.
- **Backfill-bodies:** ✅ **DONE** — bodies 25,340,001–25,401,794 are present in the mainnet EL
  (the snap-sync body gap is filled). `eth_getBlockByNumber(0x182a861)` returns non-null.
- **Binary-node datadir** `~/sharded-pir/binary-node/bn-datadir`: 406 G, checkpoint block
  25,340,000, migrated root `0x7f29471437843deeb81ddeb09e1121d9c21f5f03fe737936366fd86dbc6715e5`.
- **`rehash` pass:** ⏳ **RUNNING** (nohup, log `/tmp/rehash.log`). One-time pass writing a
  persisted Merkle hash onto every one of the trie's ~3.68 B nodes. See "Why" below.
  Monitor: `grep 'rehash]' /tmp/rehash.log | tail -3`
  Done when: `grep 'Rehash complete' /tmp/rehash.log` (prints the root hash).
  Resumable: if it dies, just relaunch the same command — it skips already-hashed subtrees.
- **ethrex fork binary:** ✅ built with the hash-persistence fix + `rehash` subcommand →
  `~/sharded-pir/binary-node/ethrex/target/release/ethrex`. Branch `feat/migrate-seed-and-catchup`.
- **`host` command:** ✅ SSHes to `0xalizk@pir-ubt-node` via `/workspace/utils/host_key`.
- **git:** commit-only — **do NOT push** (the user pushes manually). All current commits are
  already pushed; local == origin.

## Why the rehash (the big discovery this run)
Catch-up was re-walking the **entire** 3.68 B-node trie every block (~20 h / ~40 TB of cold
reads per block) and never completed even one block. Root cause: the binary trie is
**ID-addressed** and the Merkle hash was an in-memory-only `cached_hash` that was **never
serialized** — unlike the content-addressed MPT, where the hash IS the key so unchanged subtrees
are referenced by a stored hash and never re-read. The 2 M-node clean-LRU can't hold 3.68 B nodes,
so the top levels were evicted every block → full re-walk.

**Fix (committed + pushed on `feat/migrate-seed-and-catchup`):**
- `610e459d8` — persist `cached_hash` in node serialization; bulk builder stores the hash it
  already computes for free. (Fresh migrations now persist hashes automatically — no rehash needed.)
- `18a3bcea0` — `rehash` subcommand: one-time pass to backfill hashes into a trie migrated
  *before* the fix (our case). Streaming, bounded-memory, resumable.

The migrate inputs (snapshot/preimages) are **not on this box** (and ubt-node is unreachable),
so re-migrating wasn't an option — hence the in-place `rehash` of the existing datadir.

## DO THIS NEXT — exact sequence

### 1 — Wait for the rehash to finish
```bash
host "grep 'rehash]' /tmp/rehash.log | tail -3"
host "grep 'Rehash complete' /tmp/rehash.log"   # prints: Binary trie root: 0x...
```

### 2 — VERIFY the root (correctness gate — do not skip)
The printed root MUST equal the known migrated root:
`0x7f29471437843deeb81ddeb09e1121d9c21f5f03fe737936366fd86dbc6715e5`.
- **Match** → the rehash reproduced the trie exactly; proceed.
- **Mismatch** → STOP. Do not run catch-up on a possibly-corrupted trie; investigate.

### 3 — Validate catch-up on 10 blocks (now should be FAST)
```bash
host "sudo prlimit --nofile=1048576:1048576 -- \
  ~/sharded-pir/binary-node/ethrex/target/release/ethrex \
    --datadir ~/sharded-pir/binary-node/bn-datadir --network mainnet \
    catch-up http://127.0.0.1:8545 --to 25340010 2>&1 | tail -30"
```
The per-block log now prints `... this block Ns, N trie node disk-loads`. Expect **thousands**
of disk-loads per block (changed paths only), **not billions**, and seconds per block — that is
the proof the fix worked.

### 4 — Full catch-up loop to the tip
```bash
host "bash ~/sharded-pir/binary-node/catch-up-loop.sh"
```
Re-runs catch-up until the binary-node head reaches the moving mainnet tip. Resumable/crash-safe.

### 5 — Launch binary-node as a service (after catch-up reaches near-tip)
```bash
host "cp ~/sharded-pir/binary-node/binary-node.service ~/.config/systemd/user/"
host "systemctl --user daemon-reload && systemctl --user enable --now binary-node"
```
Distinct ports: http 8645 / authrpc 8651 / metrics 9190. `--p2p.disabled`.

### 6 — Step 7: feeder + equiv-daemon + Grafana (NOT BUILT)
The per-block binary↔MPT value-compare loop + dashboard. Design in `docs/architecture.md`.

## Gotchas / safety
- **FD limit:** `ulimit -n 1048576` (or `sudo prlimit --nofile=1048576:1048576 --`) for any ethrex
  op on a datadir — rehash / catch-up / node run. Default limit → "too many open files".
- The `host` shell is non-login: `cargo` isn't on PATH (`source ~/.cargo/env` first), and `ulimit`
  can't raise the hard limit (use `sudo prlimit`).
- Only one process can hold the `bn-datadir` RocksDB lock — don't run catch-up while the rehash
  is still running.
- Dedicated NVMe box; the live mainnet pair tolerated the rehash I/O fine (`sync_distance` stayed
  ~1). Still watch the CL if you stack more heavy jobs.
- **Don't push** — commit only.

## Repos / references
- `~/sharded-pir/binary-node` (`github.com/0xalizk/binary-node`, `main`).
- ethrex fork: `~/sharded-pir/binary-node/ethrex` (`feat/migrate-seed-and-catchup`,
  `github.com/0xalizk/ethrex`). Relevant commits: `ae4760881` (backfill-bodies + 3 handshake
  fixes), `610e459d8` (persist hashes), `18a3bcea0` (rehash subcommand).
- Runbook: `docs/replication.md` · Design: `docs/design-rationale.md` ·
  Step-7 design: `docs/architecture.md` · Code audit: `docs/audit_1_24_06.md` ·
  Journal: `planning/`.
- Scripts: `catch-up-loop.sh`, `catch-up-validate.sh`, `binary-node.service`.
