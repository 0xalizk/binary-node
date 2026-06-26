## planning/ — research & execution journal

This directory is the project **journal**: how the binary-node setup was figured out and
built, in the order it happened (dead-ends included). It is **append-only history** — when
something here becomes settled fact, it gets distilled into `../docs/` rather than edited in
place. For the current, maintained truth and the replication guide, read **`../docs/`**, not
this.

Read order within any folder is the `NN_` numeric prefix (chronological).

## Current status / what remains  (as of 2026-06-26)

**Phase: mainnet bootstrap (iteration_4a_ethrex), executing — step 6 (catch-up).**
- Hardware: **pir-ubt-node** (32 vCPU / 251 GB RAM / ~7 TB NVMe). Migrated datadir (406 G)
  copied from ubt-node; checkpoint block 25,340,000.
- Done: toolchain; ethrex fork + patched geth built; geth snapshot exported; Xatu raw downloaded;
  Xatu distinct; preimages.rlp built; `migrate` complete; `seed-head` + `seed-code` applied.
  Binary-node datadir at `bn-datadir/`, checkpoint 25,340,000.
- **Step 6 — active:** `backfill-bodies` running to fill the snap-sync body gap
  (blocks 25,340,001–25,401,794) in the mainnet EL RocksDB. Once done, `catch-up` will
  re-execute blocks from 25,340,001 to tip against the binary trie.
  Three p2p handshake bugs in the ethrex fork fixed 2026-06-26 to make backfill work.
  See HANDOVER.md for exact next steps and current progress.
- Remaining: finish catch-up → feeder + equiv-daemon + Grafana (step 7, NOT BUILT).
- After bootstrap (roadmap): `iteration_5/` — live-status feed on privreads (01), then
  `eth_getProof` on the binary-node (02).

**Authoritative trackers** (keep these current; this block is just the quick glance):
- step-by-step bootstrap progress → `../docs/replication.md` (`[done]`/`[wip]`/`[todo]` tags).
- post-bootstrap roadmap → `iteration_5/`.

### Research log — how we explored & decided
- `iteration_1/` — requirements Q&A (goals, constraints).
- `iteration_2/` — first plan; hit the "no complete preimage source" wall.
- `iteration_3/` — snapshot detour: reth ruled out (hashed-only) → geth ~77% preimages →
  bootstrap blocked; outreach to ethPandaOps. Some docs carry SUPERSEDED banners (kept).

### Execution log — what we're building
- `iteration_4a_ethrex/` — **primary route**: ethrex-fork binary trie bootstrapped via geth
  snapshot + Xatu preimages. At step 4 (migrate), **paused** pending RAM/disk provisioning.
  Settled design: `iteration_4a_ethrex/03_decided_approach.md`.
- `iteration_4b_geth/` — **hedge route** (while 4a is blocked): can an EIP-7864-compliant geth
  sync/convert mainnet state directly? `iteration_4b_geth/00_README.md`.
- `iteration_5/` — roadmap; first follow-on workstream (public live-status feed on privreads).

### Where to look
- Newcomer / "why is it built this way": start at `iteration_4a_ethrex/03_decided_approach.md`, then
  the iteration_3 CRITICAL findings for the rejected options.
- Replicating the setup / current truth: `../docs/`.
