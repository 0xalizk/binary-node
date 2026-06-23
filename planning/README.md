## planning/ — research & execution journal

This directory is the project **journal**: how the binary-node setup was figured out and
built, in the order it happened (dead-ends included). It is **append-only history** — when
something here becomes settled fact, it gets distilled into `../docs/` rather than edited in
place. For the current, maintained truth and the replication guide, read **`../docs/`**, not
this.

Read order within any folder is the `NN_` numeric prefix (chronological).

## Current status / what remains  (as of 2026-06-22)

**Phase: mainnet bootstrap (iteration_4a_ethrex), executing — step 4 (migrate).**
- Done: toolchain; ethrex fork + patched geth built; geth snapshot (block 25,340,000) exported
  (`snapshot.rlp` 140 G, `code.rlp` 14 G); Xatu raw (953 G) downloaded; **Xatu distinct** via
  hash-partition (**1,356,182,834 slots + 416,358,752 accounts**); **`preimages.rlp` built**
  (1,772,541,586 entries, 106 G, hash-sorted).
- **Paused (4a):** `migrate` crashed on the FD limit and is projected at *days* on 61 GB RAM
  (random mmap reads over the 108 G preimage set) → blocked on **devops provisioning**
  (≥192 GB RAM + NVMe). Inputs intact; re-run with `LimitNOFILE` raised after the upgrade.
- **In parallel (4b_geth):** exploring whether an EIP-7864-compliant **geth** is a simpler
  route (research underway).
- Remaining (4a bootstrap): re-run migrate → launch binary-node → feeder + equiv-daemon + Grafana.
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
