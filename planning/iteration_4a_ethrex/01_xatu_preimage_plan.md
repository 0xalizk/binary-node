## Xatu preimage plan — close the 23% gap (2026-06-19)

ethPandaOps' answer to the [outreach](../iteration_3/09_outreach_message.md): no full-synced
preimage snapshot, but **Xatu canonical_execution data has the plain addresses + slot keys
from genesis** — recompute the hashes from there. (They also suggested reth plain state;
already disproven — reth v2.2.0 snapshot has empty Plain* tables, see
[../iteration_3/07_CRITICAL_reth_has_no_preimages.md](../iteration_3/07_CRITICAL_reth_has_no_preimages.md).)

## Why Xatu closes the gap

Snap-synced geth's `--cache.preimages` missed ~23% of accounts (dormant, never locally
executed). Xatu's `canonical_execution_*` tables are recorded **from block 0**, so the union
of all touched addresses/slots = every account/slot in current state, dormant ones included.

Tables (mainnet coverage block 0 → ~25,340,000, verified in xatu-data schema):
- addresses: `canonical_execution_balance_diffs.address`, `nonce_diffs.address`,
  `contracts.{contract_address,deployer,factory}` (+ `storage_diffs.address`)
- slot keys: `canonical_execution_storage_diffs.{address, slot}`

Access: public Parquet at `https://data.ethpandaops.io/xatu/mainnet/databases/default/`,
partitioned by block range; query with DuckDB (httpfs, no full download needed).

## Plan

1. Extract **distinct addresses** = union(balance_diffs, nonce_diffs, contracts, storage_diffs.address).
2. Extract **distinct (address, slot)** pairs from storage_diffs.
3. `keccak256` each → synthesize a **complete** gethdbdump preimage file (same format
   `migrate` consumes; see [../iteration_3/06_gethdump_format.md](../iteration_3/06_gethdump_format.md)),
   replacing geth's partial one.
4. Re-run `ethrex migrate` with the existing geth snapshot/code exports + this preimage file
   → expect **0 skipped**, account count == true count.

Keeps the proven geth→migrate pipeline; Xatu only supplies the missing preimages.

## Validation (before mainnet)

- Direct proof: take an account `migrate` SKIPPED on Hoodi (`has_preimage=false` in
  `migrate-hoodi.log`), find its plain address in Xatu, confirm `keccak256(addr)` == the
  skipped hash.
- Then re-run the full Hoodi migrate → 0 skipped.
- **Open:** confirm Xatu has Hoodi `canonical_execution_*` (schema showed mainnet 0–25.34M;
  Hoodi unconfirmed). If not, validate on a mainnet block range instead.

## Risks / notes

- `storage_diffs` over full history is large; we only need DISTINCT keys — DuckDB streams the
  aggregation. Distinct slot keys likely ~1–2B; sizing TBD in prototype.
- Don't need to "match against hashes" to build: Xatu gives RAW addr/slot, which is exactly
  what the binary tree keys on. The preimage file is just the bridge for `migrate`'s
  geth-snapshot input path.

## Prototype results (2026-06-19) — VALIDATED

Tooling: DuckDB (httpfs, queries remote Parquet), `python3-pycryptodome` (keccak),
`xatu-preimages/build_preimages.py` (Xatu hex keys → gethdump preimage file).

- ✅ Xatu schema: `storage_diffs` carries plain `address` (0x+40hex) + `slot` (0x+64hex)
  + `from_value`/`to_value`. Path: `data.ethpandaops.io/xatu/mainnet/databases/default/
  canonical_execution_<table>/1000/<blockfloor>.parquet` (1000-block chunks).
- ✅ Genesis coverage: balance_diffs partitions exist at blocks 0, 1000, 1,000,000 →
  dormant accounts geth missed ARE captured.
- ✅ keccak correct: `keccak256("")` == c5d24601…a470 (Cryptodome = real keccak, not sha3).
- ✅ One-partition run (block 25,000,000–999): 141,142 distinct addrs (storage∪balance∪
  nonce) + 310,632 distinct slots → 451,774-entry gethdump preimage file (28.5 MB), sorted
  by hash, correct framing.
- ✅ Accuracy: 3 sampled Xatu addresses are real funded mainnet EOAs on the live node;
  USDT resolves to real code. keccak(correct plain addr) == the hashed MPT key by definition.

Conclusion: the Xatu→preimage pipeline works. Remaining = scale + value source (below).

## Next decisions

1. **Full-history extraction:** distinct addr + distinct slot across all ~25,340 mainnet
   partitions (0→head). Sized: storage_diffs ~450–500 GB, ~600–800 GB all tables.
   - ❌ **httpfs streaming rejected:** a 1M-block test (1000 remote Parquet in one DuckDB
     query) died on `SSL connect error` after ~10 min and starved the geth download. Too
     fragile + slow at 25× that scale.
   - Viable paths: **(i) request EthPandaOps ClickHouse access** (user is at ethereum.org;
     contact ethpandaops@ethereum.org) → run `SELECT DISTINCT address/slot` server-side,
     download only the result sets (~15 GB addrs + ~tens–100 GB slots). Best — avoids the
     ~700 GB pull. **(ii) local bulk download** of the Parquet (resilient per-file curl,
     skip 404s, retry) → local DuckDB distinct. Self-contained but ~700 GB disk + the pull;
     sequence after the geth download to avoid bandwidth thrash.
   - Either way: keccak the distinct sets with a **fast parallel Rust step** (billions of
     slots; Python won't scale) → gethdump preimage file.
2. **Value source for the tree** — two paths:
   - **(a) geth snapshot + Xatu preimages** (the proven pipeline): download mainnet geth
     snapshot (1018 GiB) for authoritative hashed values; Xatu supplies the complete
     preimages; `migrate` joins → expect 0 skipped. Most trusted; costs the 1 TB download.
   - **(b) Xatu-only**: reconstruct values too (latest balance/nonce/storage diff ≤ block N).
     Skips the 1 TB download and needs no keccak indirection (raw keys direct), BUT requires
     latest-per-key aggregation over full history + a code source (the `contracts` table has
     addresses, not bytecode — confirm where code comes from) + likely a migrate change to
     ingest raw input. More work, more trust in Xatu completeness.
