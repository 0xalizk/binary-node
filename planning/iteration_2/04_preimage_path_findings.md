> ⚠️ **SUPERSEDED 2026-06-17 by `../iteration_3/01_snapshot_findings.md`.** The core
> verdict below ("no downloadable preimage dataset") is WRONG: ethpandaops publishes
> fresh full mainnet reth/erigon snapshots whose plain state is a complete preimage
> source. Kept for history only.

# Preimage-dataset path — findings & verdict

Research done foreground (subagent kept hitting API 500s). Sources cited inline.

## Verdict (short)

**There is NO fast, Geth-free, no-re-execution path to a complete mainnet binary
state that you can take unilaterally today.** Every route to the preimages either
(a) requires running Geth/Erigon, or (b) requires re-execution to re-derive keys, or
(c) depends on a preimage dataset that **is not publicly downloadable** (it's future-
fork planning, not a shipped artifact).

**BUT** there's a practical shortcut that bypasses all of this: **ask the ethrex /
EF verkle team for the artifact directly** (you're at ethereum.org; they built exactly
this). Details below.

## What we found

### 1. No public preimage dataset exists today
- EIP-6873 (preimage retention) and jsign's "Preimages generation & distribution
  strategy" are **proposals for the future Verge fork**, not shipped downloads.
  - jsign hackmd: *"this is an actively discussed topic"*; distribution method
    (CDN/torrent/in-protocol) still **undecided**; no download location given.
    Est. size ~**40 GB** (jsign) / ~**70 GB** (EIP-6873). Flat file, "trivial
    encoding." https://hackmd.io/@jsign/vkt-preimage-generation-and-distribution
  - stateless.fyi/state-conversion/preimages: confirms it's architectural planning,
    *"no concrete implementation or downloadable artifact exists yet."*
    https://stateless.fyi/state-conversion/preimages.html
- So: **can't just download mainnet preimages.**

### 2. Preimages are fundamentally circular without a dataset
- MPT stores `keccak(addr)` / `keccak(slot)`; keccak is one-way. To get the real
  keys you must either have recorded them when they were inserted (preimage
  recording) or re-derive them by **executing** the blocks that touch them.
- A *complete* current-state preimage set therefore requires a **full sync with
  preimage recording** (re-execution) — in Geth (`--cache.preimages`), Erigon, or
  equivalent. There is no shortcut; this is itself days–weeks + big disk.

### 3. ethrex has no native Geth-free preimage source
- ethrex does **not** retain preimages or expose a `--cache.preimages` equivalent
  (gh search: no such flag on `main`).
- ethrex's `tooling/archive_sync` *does* obtain plaintext addresses + storage —
  **but only by talking to a Geth archive node** over IPC (`debug` dump API). README:
  *"We also rely on geth's debug api for this … not guaranteed to work for other
  non-geth-compatible implementations."* So it's Geth-dependent too.
  (It downloads full MPT state at a post-merge block so ethrex can full-sync forward
  from ~block 15,537,395 instead of genesis — useful for the *MPT* node, but it
  produces MPT state, not binary state.)

### 4. The eip-7864-plan `migrate` tool is also Geth-bound
- Confirmed earlier: needs `geth db export preimage` + a patched Geth code exporter.
  No ethrex-native equivalent.

## Why the "temporary Geth bootstrap" idea doesn't actually help
Standing up Geth just to export preimages sounds like scaffolding, but to have
preimages for *all* current state, that Geth must itself **full-sync with preimage
recording** (re-execution, days–weeks, >1 TB). That's no faster than just letting
binary-node re-execute. So spinning up Geth buys nothing unless it already exists
with a complete preimage DB.

## The actual shortcut: get the artifact from people who already made it
- The **ethrex team (lambdaclass)** built `eip-7864-plan` and *ran migrations* (the
  migration guide cites concrete Hoodi conversion runs). They plausibly have tooling
  output and possibly mainnet/testnet preimage exports.
- The **EF verkle/binary team (jsign, gballet)** have generated preimage datasets for
  their experiments.
- You are at **ethereum.org** — a direct ask ("do you have a recent mainnet preimage
  export or a pre-built binary-trie snapshot we can use to bootstrap?") could collapse
  weeks of re-execution into an hours-long bulk import. This is the highest-leverage
  move and costs one message.

## Honest bottom line for the plan
- If we must be **self-contained** (no external artifacts, no Geth): **re-execution is
  unavoidable.** Best self-contained variant = full-sync the binary branch from
  **post-merge (~15.5M)** using an MPT archive_sync bootstrap for the *reference* side,
  rather than genesis — fewer blocks, though post-merge blocks are heavy. binary-node
  still executes everything (strong differential test).
- If we can **obtain a preimage dataset / binary snapshot** from ethrex/EF: one-shot
  build in hours, then follow tip with execute-and-cross-check-against-BAL going
  forward. Fast start, execution test accrues on live blocks (fine for an
  indefinitely-running node, your #12).
