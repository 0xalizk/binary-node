## Decided approach (iteration 4)

Final plan after the snapshot-bootstrap detours. Bootstrap the EIP-7864 binary-trie
state from **geth snapshot values + Xatu preimages**, then run the binary-node as a
p2p-disabled shadow of the live MPT node with a per-block equivalence check.

Audience: Ethereum-literate devs; client/sync internals explained inline.

## Why preimages are the blocker

State is keyed by hashes, and the two trees use *different* hashes:
- MPT (today): account at `keccak256(address)`, storage at `keccak256(slot)`.
- EIP-7864 binary trie: keys derived via **BLAKE3** over the *raw* address/slot —
  `get_tree_key(address, tree_index, sub_index) = BLAKE3(address32 ++ tree_index)[..31] ++ sub_index`,
  with multiple account fields packed under one stem and storage folded from raw
  `(address, slot)` (`crates/common/binary_trie/key_mapping.rs`).

Both hash, but you can't go `keccak256(address) → BLAKE3 key` directly: keccak is
one-way, and BLAKE3 needs the raw `address` as input. The **raw address/slot is the only
bridge** — the shared preimage both schemes consume:

```
keccak256(addr)  --preimage lookup-->  addr  --BLAKE3 derivation-->  binary-trie key
```

So a snapshot whose preimage set is incomplete yields a structurally incomplete tree
(migrate cannot compute keys for the missing accounts; it skips them).

## Two data sources, two halves

`ethrex migrate` (branch `eip-7864-plan`) consumes three Geth `gethdbdump` streams —
preimages, snapshot (state values), code — and derives the BLAKE3 keys itself
(see `03_ethrex_migrate_findings.md`, `06_gethdump_format.md`). We supply:

| | geth snapshot (ethPandaOps) | Xatu canonical_execution |
|---|---|---|
| Provides | state **values** at block 25,340,000 (accounts: nonce/balance/codehash; storage values; code) | the **preimages** — raw addresses + slot keys, complete, from block 0 |
| Keyed by | `keccak256(...)` (hashed) | raw (un-hashed) |
| Why this one | authoritative current state; also the MPT oracle the equiv-daemon checks against | its snap-synced `--cache.preimages` is only ~77% complete (dormant accounts never locally executed); Xatu's genesis-onward diffs are complete |
| We take | `snapshot.rlp` (values) + `code.rlp` (bytecode) | distinct addresses + distinct slot keys only (ignore its values) |

geth supplies *what accounts hold*; Xatu supplies *what they are addressed by*. Neither
alone suffices; migrate joins them. (reth was rejected — v2.2.0 stores hashed state with
no preimages at all; see `07_CRITICAL_reth_has_no_preimages.md`,
`08_CRITICAL_geth_preimages_incomplete.md`.)

## Bootstrap pipeline

1. **geth exports.** `tar`-extract the snapshot to a datadir, then
   `geth db export snapshot` → `snapshot.rlp` and `geth db export code` → `code.rlp`
   (patched `edg-l/go-ethereum@feat/export-code`, == geth v1.17.2). Skip the preimage
   export (Xatu replaces it). Then delete the tarball + extracted datadir (~2.4 TB).
2. **Xatu distinct extraction.** DuckDB over the ~953 GB of local Parquet:
   `DISTINCT address` (union of balance_diffs ∪ nonce_diffs ∪ contracts ∪
   storage_diffs.address) and `DISTINCT slot` (storage_diffs.slot). Then delete the raw
   Parquet (~953 GB).
3. **Preimage file.** Fast parallel Rust keccak over the distinct sets → complete
   `preimages.rlp` in gethdbdump format, sorted by hash (mmap-mode requirement). Value
   length tags addr (20B) vs slot (32B).
4. **migrate.**
   `ethrex --network mainnet --datadir <bn-dd> migrate <xatu-preimages.rlp> <geth-snapshot.rlp> --code <geth-code.rlp> --at-block 25340000`.
   Builds the binary-trie state DB (RocksDB) at the snapshot block. **Milestone:
   skipped ≈ 0** (vs Hoodi's 10.27M) — the proof the preimage set is complete.
   `--at-block` records where to resume; state-root validation is disabled on the branch.
5. **Launch binary-node.** Start the fork binary on `<bn-dd>` → a live node holding
   mainnet state in the binary trie as of block 25,340,000.
6. **Wire the shadow.** Feeder + equiv-daemon (below), then forward-execute from the
   snapshot block and begin per-block checks.

## Target architecture (both on this one host)

- **Vanilla node (authoritative, unchanged):** ethrex v16.0.0 (`ethrex.service`), MPT,
  p2p-synced to mainnet head. Ground-truth reference; never modified.
- **binary-node (shadow):** forked ethrex (`eip-7864-plan`), binary-trie state, **its own
  datadir**, **p2p disabled**. It does not sync itself.
- **Feeder:** pulls each new canonical block from the vanilla node and feeds it to the
  binary-node, which executes it and updates the binary trie. Both clients thus process
  the identical block stream — one into an MPT, one into a binary trie.
- **equiv-daemon:** per block, compares value-level state (getBalance / getTransactionCount
  / getCode / getStorageAt) between the two, cross-checked against the block's EIP-7928
  BAL as the touched-key oracle; records discrepancies `(block, addr, slot, v_mpt, v_bin)`;
  halt at 1000. Compares values, never roots/proofs (the roots differ by design).
- **Dashboard:** existing Grafana/Prometheus (datasource `prom-001`): discrepancy counts,
  latest offenders, progress, state size, read/write speeds.

Isolation is three-way — separate binary, separate datadir, p2p off (fed only by the
feeder) — so the binary-node cannot perturb the live node. Only the host's CPU/disk are
shared; the binary-node is rate-limited (nice / IOWeight) so the live node keeps priority.

## Disk staging

Peak is kept ~2.5–3 TB (of 7.7 TB) by deleting each intermediate once consumed: tarball
after extraction; extracted geth datadir after exports; raw Xatu Parquet after the distinct
sets are pulled. Keepers: `snapshot.rlp`/`code.rlp`/`preimages.rlp` (migrate inputs) and
the final binary-trie DB.

## Notes / open items

- Forward-sync-from-mid-chain (`--at-block`) is documented but unproven end-to-end; the
  feeder + first forward blocks are the real test (Hoodi couldn't validate it — no Xatu
  coverage there, and migrate built a deliberately-incomplete tree).
- Archival is out of scope: the branch is non-archive (~128-block window). Genesis archival
  would require full re-execution; forward archival needs versioned-state retention + much
  more disk. If historical queries are wanted later, serve them from Xatu, not the node.

## Scope boundary

iteration_4a_ethrex concludes when the **binary-node is shadowing mainnet at the tip block-by-block
and the equiv-daemon is running and capturing stats** (+ the Grafana dashboard). The public
live-status feed on privreads (box-side pusher → Cloudflare Worker/KV → GitHub-Pages widget)
is the first follow-on workstream — see `../iteration_5/01_status_feed.md`.
