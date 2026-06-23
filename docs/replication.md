## Replication runbook

How to stand up a binary-node shadow + equivalence check against mainnet. Each step is
status-tagged: **[done]** validated, **[wip]** in progress on our host, **[todo]** designed
but not yet executed/written from a real run. This is the seed of the public guide; wip/todo
sections get filled in as the mainnet bootstrap completes.

### Overview

geth snapshot (state values) + Xatu (complete preimages) → `ethrex migrate` builds the binary
tree at a recent block → launch the binary-node, feed it blocks from a normal node, compare
per block. Rationale and rejected alternatives: `design-rationale.md`.

### Recommended resources

Sized from this run (mainnet @ block 25,340,000: ~1.99 B state entries → 1.36 B distinct slots
+ 416 M accounts → 1.77 B preimages, 106 G `preimages.rlp`). **The bootstrap is the peak
demand**; steady-state shadowing afterward is far lighter (~1 block / 12 s).

- **RAM — the dominant lever.** The recurring constraint is "does the working set fit in RAM."
  61 GB (our box) *works* but forces workarounds: single-pass `DISTINCT`/sort OOM at ~1 B+ rows
  (→ hash-partition), and migrate drops to **mmap mode** because the ~108 G preimage bins can't
  be cached → its collect phase becomes random-disk-bound (~2 MB/s, ~19 h). **≥128–192 GB
  recommended** — enough to keep the ~108 G preimage bins in page cache, which makes migrate's
  collect memory-bound (~1–2 h vs ~19 h) and lets the simpler in-memory distinct / migrate
  `--fast` paths just work.
- **Disk I/O — the hottest contention, especially random reads.** Sequential stages (Xatu
  distinct, partition, keccak) are high-*bandwidth*; **migrate's collect is random-read-heavy**
  (binary-search over the preimage bins) → IOPS/latency-bound and by far the slowest stage on a
  slow disk. **Use NVMe SSD.** If co-located with a live node, either **hard-cap every heavy
  job's I/O** (see Lessons — an unthrottled scan knocked our node offline) or, better, put the
  binary-node work on a **dedicated disk** so they don't contend.
- **Disk capacity: ~4 TB free.** Peaks: geth tarball + extracted datadir coexist (~2.4 TB)
  during extraction; migrate temp CF + binary trie + inputs during the build. Stage deletes to
  hold peak ~2.5–3 TB (see Disk staging).
- **CPU: ~8 cores is ample** — the pipeline is I/O-bound; only keccak is CPU-heavy (~10 min for
  1.77 B). More cores won't move the bottleneck.

Where each peak lands: **RAM** & **disk-IOPS** → migrate collect (resident bins + random
lookups); **disk-bandwidth** → the bulk scans (distinct/partition/keccak); **disk-capacity** →
geth extraction + migrate build.

### 0. Host & prerequisites  [done]

- Linux host with enough disk for the bootstrap (peak ~2.5–3 TB with staged cleanup; see
  "Disk" below). Ours: 8 core / 61 GB / 7.7 TB.
- A live, synced normal Ethereum node (the MPT reference) to feed blocks and check against.
- Toolchain: Rust (rustup), Go 1.24+, clang/llvm, cmake, pkg-config, git-lfs, zstd, DuckDB.
- **ethrex fork** (binary-trie node + `migrate`): `lambdaclass/ethrex` branch `eip-7864-plan`,
  `cargo build --release` (built at commit `b0fe293`). Clone shallow; skip LFS test fixtures.
- **patched geth** (adds `db export code`): `edg-l/go-ethereum` branch `feat/export-code`
  (geth v1.17.2 base), `go build ./cmd/geth`. Use the geth version matching your snapshot.

### 1. Obtain state values — geth snapshot exports  [done]

1. Download a geth snapshot for the target block (ours: ethPandaOps
   `snapshots.ethpandaops.io/mainnet/geth/<block>/snapshot.tar.zst`). Use a resilient,
   resumable transfer (`curl --http1.1 -C -` in a retry loop — connections drop).
2. Extract to a datadir; point a geth datadir's `geth/` at it (symlink).
3. Export (skip the preimage export — Xatu replaces it):
   ```
   geth --datadir <dd> db export snapshot snapshot.rlp
   geth --datadir <dd> db export code     code.rlp
   ```
4. Delete the tarball after extraction and the extracted datadir after exports (~2.4 TB back).

### 2. Distinct preimage keys — Xatu  [done]

Xatu `canonical_execution_*` Parquet has plain addresses + slot keys from block 0 (953 G over
4 tables). Download locally with resilient per-file `curl` (skip 404 gaps) — **and validate
each file's trailing `PAR1` magic, not just non-zero size** (dropped connections leave
truncated-but-present files; we hit 6). Path:
`data.ethpandaops.io/xatu/<net>/databases/default/canonical_execution_<table>/1000/<blockfloor>.parquet`.

Then **distinct via hash-partition** (naive `SELECT DISTINCT` OOMs at this cardinality even
with spill — see Lessons): partition each value into 16 buckets by first hex nibble (one
streaming DuckDB pass, constant memory), `DISTINCT` each bucket (fits in RAM), concat.
- slots: from `storage_diffs.slot` → **1,356,182,834** distinct.
- accounts: union of `balance_diffs` / `nonce_diffs` / `contracts` addresses (skip
  `storage_diffs.address` — every storage-bearing addr is a contract, already in `nonce_diffs`)
  → **416,358,752** distinct.
Output: 16+16 Parquet files, raw binary keys (slot 32 B, addr 20 B). Delete raw Parquet after.

### 3. Build the preimage file  [done]

`preimage-builder/` (Rust): read distinct keys → `keccak256` each → emit a gethdbdump preimage
stream `keccak(x) → x` (value length tags addr=20 B vs slot=32 B), **globally sorted by hash**
— migrate's mmap path binary-searches it and does NOT sort internally. The sort reuses the
hash-partition trick (route by `hash[0]` → 256 buckets → in-memory pdqsort per bucket →
concat; no merge). Result: **1,772,541,586 entries, 106 G**.

### 4. Migrate  [wip: running on mainnet]

```
ethrex --network mainnet --datadir <bn-dd> migrate <preimages.rlp> <snapshot.rlp> --code <code.rlp> --at-block 25340000
```
Builds the binary-trie state DB at the snapshot block. Auto-tunes to **mmap mode** (preimages
~116 G > RAM → binary-search the sorted file). **Success check: `skipped ≈ 0`** in the collect
phase (nonzero = missing preimages — see `design-rationale.md`). On Hoodi with geth's partial
preimages this was 10.27 M; with the complete Xatu set it should be ~0.

### 5. Launch the binary-node  [todo]

Start the fork binary on `<bn-dd>` (p2p disabled). It holds mainnet state as of the snapshot
block. Confirm it serves correct values for known accounts.

### 6. Run the shadow — feeder + equiv-daemon + dashboard  [todo: not yet built]

Feeder pulls each new block from the reference node → binary-node executes it. equiv-daemon
compares value-level state per block (BAL-driven), records discrepancies, exports Prometheus
metrics → Grafana. See `architecture.md`.

### Lessons / gotchas (hard-won)

- **Hard-cap heavy jobs or they kill the live node.** This box shares ONE virtual disk with a
  live mainnet node; an unthrottled DuckDB scan saturated I/O → the node went `el_offline` and
  fell ~140 slots behind. Run every heavy job under a systemd cap (`IOReadBandwidthMax` /
  `IOWriteBandwidthMax` ~100 M, `CPUQuota`, `Nice`) + a watcher that aborts on `el_offline` /
  high load. `nice` alone is NOT enough — it only limits CPU; disk I/O is the bottleneck.
- **Validate downloads by content, not size** — dropped connections leave truncated Parquet
  that pass a size check but fail to parse. Verify the trailing `PAR1` magic.
- **`DISTINCT` (and sort) at ~1 B+ rows OOMs even with spill** — the in-memory hash-table
  state outgrows the limit. Use hash-partition divide-and-conquer for both distinct and sort.
- **migrate needs preimages pre-sorted by hash** (mmap binary-search; no internal sort).
- Use **fixed binary keys** (`unhex` → 32/20 B), not hex strings — halves memory and is the
  form keccak/migrate consume.

### Disk staging

Delete each intermediate once consumed: snapshot tarball after extract; extracted geth datadir
after exports; raw Xatu Parquet after distinct extraction. Keepers: `snapshot.rlp` /
`code.rlp` / `preimages.rlp` (migrate inputs) and the final binary-trie DB.
