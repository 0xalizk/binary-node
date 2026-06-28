## binary-node

Bootstrapping an **EIP-7864 binary-trie** "shadow" Ethereum node for mainnet, plus an
equivalence daemon that checks binary↔MPT state-*value* equivalence per block. The shadow
runs p2p-disabled, fed by a local mainnet node; it mirrors the same state in a binary tree
(keyed off **raw** addresses/slots via BLAKE3) instead of the keccak-keyed MPT. State roots
differ by design, so the check compares **values**, not roots.

Built on a fork of ethrex's binary-trie work plus a memory-bound bulk-migration pipeline.

### How to replicate and run your own binary-node

**Recommended hardware:** 32 vCPU · 256 GB RAM · **~7 TB local NVMe SSD**. The NVMe is not
optional — per-block binary-trie execution is random-read-latency-bound, so network-attached
or HDD-class volumes won't keep up (rationale: [audit](docs/audit_1_24_06.md),
[design](docs/design-rationale.md)). The RAM caches the trie working set; the disk absorbs the rest.

**You need this repo + two external forks + a synced mainnet geth node:** this repo (tooling +
docs + journal), the [**ethrex fork**](https://github.com/0xalizk/ethrex/tree/feat/migrate-seed-and-catchup)
(the binary-trie node + `migrate` / `seed-head` / `seed-code` / `catch-up`), and
[**patched geth**](https://github.com/edg-l/go-ethereum/tree/feat/export-code) (adds `db export code`).

**Pipeline** — full commands in **[docs/replication.md](docs/replication.md)**; in brief:

1. **Export** current mainnet state from a synced geth: `geth db export snapshot` + `db export code`.
2. **Preimages** — download [Xatu `canonical_execution_*`](https://ethpandaops.io/data/) and extract
   distinct addresses/slots ([`xatu-preimages/`](xatu-preimages/)), then keccak → `preimages.rlp`
   ([`preimage-builder/`](preimage-builder/)).
3. **Re-sort + migrate** — re-sort the snapshot by `keccak(slot)` ([`snapshot-resorter/`](snapshot-resorter/))
   so `ethrex migrate`'s preimage lookups go sequential (memory-bound), then `ethrex migrate` builds the binary trie.
4. **Seed** the datadir bootable + code-complete: `ethrex seed-head …` then `ethrex seed-code …`.
5. **Catch up** — `ethrex catch-up <local-node-rpc>` re-executes blocks forward to the tip, then
   run the node (`--p2p.disabled`). Raise the open-file limit (`ulimit -n 1048576`) for the
   seed / catch-up / run steps — RocksDB opens many SSTs.

> ⚠️ **The live feeder + equivalence-daemon are not built yet.** Today the pipeline
> gets you a binary-trie node bootstrapped, caught up to the tip, and serving state; the per-block
> binary↔MPT comparison loop is still to come.

**Shortcut (fast path), if you already have a migrated datadir:** skip 1–4 — **first stop any node
holding that datadir** (RocksDB takes a single-process lock, so copying a live datadir yields a
corrupt copy), then copy it over and start at **catch-up** (step 5).

### Status: mainnet binary trie built ✅

The full mainnet state has been migrated into a binary trie on a commodity (61 GB RAM) box:

```
Recorded migrated state at block 25,340,000
Binary trie state root: 0x7f29471437843deeb81ddeb09e1121d9c21f5f03fe737936366fd86dbc6715e5
Collection complete: 388,383,489 accounts, 1,563,779,711 storage slots, 3,666,719,467 entries
```

The node also **boots and serves correct balance/nonce/code** at the checkpoint block.
Known caveat: ~1.78% of accounts/slots lacked a preimage in the derived set and were skipped
(coverage gap, not a pipeline failure) — see `planning/` and `docs/design-rationale.md`. Next:
catch up to the tip (on NVMe hardware — see below), then build the equivalence daemon.

### The core problem, and how it's solved

The MPT keys state by `keccak256(address)` / `keccak256(slot)` (one-way). EIP-7864 keys its
tree off the **raw** address/slot, so migrating state needs a complete set of keccak
**preimages**. Public snapshots don't provide one (reth stores state hashed → zero preimages;
geth's `--cache.preimages` is only ~77% complete on snap-synced nodes — see git history of
this README and `planning/iteration_3/`). The working recipe instead:

- **values** from a geth snapshot export (`db export snapshot` + `code`) — authoritative
  current state at the snapshot block, and the MPT oracle the equivalence daemon checks against
- **preimages** from ethPandaOps **Xatu** `canonical_execution_*` parquet (plain addresses +
  slot keys recorded from block 0), keccak'd into a preimage file
- ethrex `migrate` joins them to bulk-build the binary trie

### The memory-bound trick (why this runs on 61 GB)

`migrate` resolves ~2 B leaves against a ~108 GB preimage file. Naively that's ~1 B random
mmap reads (the file can't fit in RAM) → days, and it crashed once. The fix doesn't touch
migrate: **re-sort its input**. The snapshot's storage is ordered by `keccak(addr)+keccak(slot)`,
so `keccak(slot)` lookups scatter randomly. `snapshot-resorter` re-sorts storage by
`keccak(slot)` → lookups become monotonic → the mmap binary-search streams sequentially →
memory-bound, no extra RAM. Output is provably identical (migrate writes to a temp CF sorted by
tree key, so input order can't change the result). See `snapshot-resorter/`.

### Bootstrap pipeline

```
0  toolchain + builds (ethrex fork, patched geth, duckdb)
1  geth export        -> snapshot.rlp + code.rlp
2  Xatu distinct      -> distinct storage slots + account addresses   (xatu-preimages/)
3  keccak -> preimages.rlp                                            (preimage-builder/)
3.5 snapshot-resort   -> snapshot-sorted.rlp                          (snapshot-resorter/)
4  migrate            -> binary trie (RocksDB datadir)
5  seed-head + seed-code, launch binary-node (p2p off)
6  catch-up           -> re-execute blocks from the checkpoint to the tip
7  feeder + equivalence daemon + Grafana                 (NOT BUILT YET)
```

Full step-by-step: **`docs/replication.md`**. Design rationale + rejected approaches:
**`docs/design-rationale.md`**. Chronological research/execution journal: **`planning/`**.

### What's in this repo

- `snapshot-resorter/` — Rust; the memory-bound resort (the key enabler). Tested + the
  gethdbdump round-trip validated.
- `preimage-builder/` — Rust; reads distinct addr/slot parquet → keccak256 → hash-partitioned,
  globally-sorted `preimages.rlp` in gethdbdump format.
- `xatu-preimages/` — download (`download_xatu.py`) + bounded-memory distinct extraction
  (`build_addrs_distinct.sh`, `build_slots_distinct.sh`) over ~1 TB of Xatu parquet.
- `reth-state-extractor/` — shelved; the tool that proved reth stores no preimages (historical).
- `docs/`, `planning/` — canonical docs and the journal.
- `*.log`, `gethdump/**/export.log` — raw run evidence.

Large data artifacts (snapshots, exports, preimage files, the migrated DB, raw Xatu) are
git-ignored — regenerate via the pipeline.

### External components (forks, not vendored — clone these yourself)

- **ethrex** (binary-trie node + `migrate` / `seed-head` / `seed-code` / `catch-up`):
  [`0xalizk/ethrex`](https://github.com/0xalizk/ethrex/tree/feat/migrate-seed-and-catchup) branch
  `feat/migrate-seed-and-catchup` (a fork of `lambdaclass/ethrex`'s `eip-7864-plan`). Clone with
  `GIT_LFS_SKIP_SMUDGE=1` (skips ~356 MB of LFS test fixtures not needed to build), then
  `cargo build --release`. (The `migrate` skipped-slots fix is also up as a PR to upstream.)
- **patched geth** (adds `db export code`): `edg-l/go-ethereum` branch `feat/export-code`
  (geth v1.17.2 base).
