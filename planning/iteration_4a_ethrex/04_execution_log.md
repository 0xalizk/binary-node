## Execution log — mainnet bootstrap (what actually happened)

The journal of executing the decided approach (`03_decided_approach.md`): incidents, pivots,
and final numbers. Plan docs say what we intended; this says what occurred. (Canonical
how-to-replicate lives in `../../docs/replication.md`.)

## Timeline & key numbers

- **Snapshot block:** geth mainnet 25,340,000 (block ts 2026-06-16). `snapshot.rlp` 140 G
  (1,987,444,991 entries), `code.rlp` 14 G (2,444,221 contracts).
- **Xatu raw:** 953 G across 4 `canonical_execution_*` tables, 25,341 partitions each.
- **Distinct:** 1,356,182,834 storage slots; 416,358,752 account addresses.
- **preimages.rlp:** 1,772,541,586 entries (= slots + accounts), 106 G, hash-sorted.
- **migrate:** mmap mode (preimages ~116 G > RAM), `--at-block 25340000`. [result TBD]

## Incidents & pivots (the hard-won parts)

1. **I/O incident (node outage).** First Xatu distinct ran DuckDB unthrottled (threads=4, no
   I/O cap) → saturated the single shared virtual disk → loadavg 100+ → **ethrex restarted,
   CL went `el_offline` and fell ~140 slots (~28 min) behind** before recovery. Fix, now
   standing rule: run heavy jobs under a hard systemd cap (`IOReadBandwidthMax`/
   `IOWriteBandwidthMax` + `CPUQuota` + `Nice`) + a watcher that auto-aborts on
   `el_offline`/high load. `nice` alone is useless here (CPU only; disk I/O is the bottleneck).
   Validated: 50–100 MB/s cap → loadavg ~3–5, node unaffected.

2. **Corrupt Parquet.** First capped distinct died on a truncated `storage_diffs` file. The
   downloader skipped-if-size>0, so dropped-mid-stream files looked present. Scanned all
   101,364 files for trailing `PAR1` magic → **6 truncated**, re-fetched. Lesson: validate by
   content (magic), not size.

3. **DISTINCT OOM → hash-partition.** Naive `SELECT DISTINCT slot` OOM'd repeatedly even with
   binary keys, threads=1, 20 GB limit, and 59 G spilled — DuckDB's in-memory hash-table state
   outgrows the limit at ~1.3 B distinct. Pivoted both slots and accounts to **hash-partition**
   (partition by first nibble → 16 buckets → distinct each in RAM → concat). Accounts also
   OOM'd the simple path → same fix. (Skipped `storage_diffs.address` for accounts: every
   storage-bearing addr is a contract, already in `nonce_diffs`.)

4. **keccak step needs sorted output.** Confirmed migrate's mmap preimage path binary-searches
   and does NOT sort internally → our preimage file must be globally hash-sorted (geth's native
   dump is, because its DB is key-ordered). Built `preimage-builder/` (Rust): keccak + the same
   hash-partition trick to sort (route by `hash[0]` → 256 buckets → in-memory pdqsort → concat,
   no merge). Validated on a bucket-0 subset (keccak-correct, sorted, right value-length tags)
   before the full 1.77 B run.

5. **threads vs I/O, refined.** Slots distinct was I/O-bandwidth-bound (cap is the limit →
   threads don't help). Account distinct's Pass-1 column-reads were overhead/latency-bound
   (well under the cap) → more threads *did* help there. Parallelism only helps when not
   gated by the I/O cap.

## Artifacts on disk (~/sharded-pir/binary-node/)

- `gethdump/mainnet/{snapshot,code}.rlp` — geth exports (migrate inputs).
- `xatu-preimages/preimages.rlp` — final hash-sorted preimage file (migrate input).
- `xatu-preimages/distinct/{slots,addrs}_*.parquet` — 16+16 distinct-key buckets.
- `preimage-builder/` — the Rust keccak+sort tool; `reth-state-extractor/` — shelved (reth had
  no preimages; `diag.rs` proved it), kept for the gethdump-writer reuse.
- `xatu-preimages/xatu-raw/` — 953 G raw Xatu (deletable once preimages.rlp is trusted).
- geth extracted datadir (~1.2 TB) kept as export insurance until migrate validates.
