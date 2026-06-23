# ⛔ CRITICAL FINDING #2 — ethpandaops geth preimages are INCOMPLETE (~77%) (2026-06-17)

The Hoodi smoke test's `migrate` run completed but produced an **incomplete** binary
tree. This is a showstopper for the snapshot-bootstrap approach and resolves the
"is geth's `--cache.preimages` complete?" question that was the whole point of the test.

## The numbers (Hoodi geth snapshot @ block 3,030,000)
From `~/sharded-pir/binary-node/migrate-hoodi.log`:
- Preimages parsed: 143,110,410 = **34,163,095 address** + 108,947,315 slot preimages.
- Collection: **34,161,485 accounts**, 272,878,767 storage slots loaded.
- **10,268,484 entries SKIPPED — "missing preimages" (`has_preimage=false`).**
- Truth (from reth snapshot @ same block, `HashedAccounts`): **44,431,577 accounts.**
- ⇒ geth had preimages for only **34.16M / 44.43M = 76.9%** of accounts.
  **~10.27M accounts (23%) cannot be un-hashed** and were dropped from the tree.

## Root cause (structural, not fixable by re-downloading)
- ethpandaops nodes are **snap-synced**. Snap delivers state as already-hashed
  key/value pairs. geth's `--cache.preimages` only records `keccak(x)→x` when a key
  passes its hasher during **local block execution** — so only accounts touched by
  blocks executed *after* the node caught up get preimages. The ~23% never-touched-
  since-sync accounts have state (they're in the snapshot, hashed) but **no preimage.**
- Confirmed there is **no archive / full-sync variant** on ethpandaops (probed
  geth-archive/reth-archive/etc → none). All variants are snap-synced ⇒ all incomplete.
- The geth *snapshot* (flat state) IS complete (all 44.4M accounts, hashed); only the
  *preimage table* is partial. So the data exists — we just can't reverse keccak for 23%.

## Why incomplete is fatal for this project
binary-node must mirror mainnet state exactly. Missing 23% of accounts ⇒ forward
execution reads 0-balance for them ⇒ diverges from mainnet on first touch ⇒ the
equiv-daemon floods with our-own-fault discrepancies (hits the 1000-halt instantly).
Not viable. We need ~100% preimage coverage.

## Where this leaves us
**Both ethpandaops snapshot clients fail to provide complete preimages:**
- reth v2.2.0: stores hashed state, **0 preimages** (07_CRITICAL_reth_has_no_preimages.md).
- geth v1.17.2: has a preimage table but only **~77% complete** (this doc).
This empirically confirms iteration_2's `04_preimage_path_findings.md` conclusion that we
prematurely overturned: **there is no complete, downloadable mainnet preimage source;
complete preimages require re-execution (or an archive node that recorded them).**

## What the smoke test DID prove (positive)
- ethrex `migrate` works end-to-end at scale: built 504,186,669 entries, 353M nodes,
  valid binary state root `0xef32c6f3…`, recorded at block 3,030,000, in ~52 min on
  this box (collect ~31 min + build ~52 min overlap; total wall ~1h25m incl. exports).
- The full pipeline (geth export → gethdump format → migrate → RocksDB binary trie)
  is sound. The ONLY blocker is preimage completeness of the source.

## Options forward (DECISION NEEDED)
1. **Full re-execution from genesis** with geth `--syncmode full --cache.preimages`
   (NOT snap). Generates guaranteed-complete state + preimages; snapshot then unneeded.
   Hoodi: ~a day (3M blocks). Mainnet: ~weeks. Self-contained, guaranteed. (= old Option A.)
2. **Complete the preimage set by address-harvesting** from chain history (we have the
   full ancient store): recover raw addresses from tx senders (ecrecover), tx `to`,
   contract-creation addrs (receipts), withdrawals, coinbases, access lists, log topics;
   keccak & match against the 10.27M missing hashes. Cheap (no EVM). Coverage uncertain
   — misses internal CREATE/CREATE2 + internally-only-touched accounts (worse on mainnet
   than Hoodi). Residual would need tracing.
3. **Outreach to EF / ethrex / verkle team** (jsign, gballet — preimage generation/
   distribution is literally their EIP-6873 workstream) for a complete preimage dataset
   or a full-synced node with preimages. User is at ethereum.org. (= old Option B, now
   empirically justified.) High upside, async, cheap to ask.
4. **Hybrid**: full-sync a geth on HOODI from genesis (~a day) to finish the end-to-end
   smoke test (prove forward-sync works) while pursuing #3 for mainnet.

## Recommendation
We've effectively re-derived iteration_2's Option C, now with proof. Suggest:
(a) send the EF preimage-artifact ask now (#3) — cheap, high-upside, user is well-placed;
(b) as the self-contained fallback, evaluate #2 (harvesting) on Hoodi to measure achievable
coverage cheaply before betting on it; (c) keep #1 (full re-exec) as the guaranteed backstop.
Mainnet bootstrap stays blocked until a complete preimage source exists.
