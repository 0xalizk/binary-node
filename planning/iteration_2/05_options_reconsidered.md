# Options reconsidered (after preimage research)

The dream of "download preimages → one-shot build in hours, no Geth, no re-exec" is
**not available off-the-shelf today**. So the real options are:

## Option A — Self-contained re-execution (no external deps)
- binary-node full-syncs the `eip-7864-plan` branch, **executing every block**, from
  genesis (or from post-merge ~15.5M if we accept a bootstrap for the reference side).
- Equivalence: during catch-up, the branch's free per-block intrinsic checks
  (receipts_root / gas_used / requests_hash / block_access_list_hash); at tip, full
  value-level + **execute-and-cross-check-against-BAL** per new block.
- **Pro:** no Geth, no external artifacts; strongest execution differential test
  (covers all executed blocks). **Con:** days–weeks to tip.

## Option B — Ask ethrex/EF for a preimage dataset or pre-built binary snapshot
- One message to lambdaclass / jsign / verkle team (you're at ethereum.org).
- If they share a recent mainnet **preimage export**: adapt `migrate` to consume it +
  an ethrex/Geth state snapshot → bulk-build binary tree in **hours**.
- If they share a **pre-built binary-trie snapshot**: import directly, skip the build.
- Then binary-node follows tip, executing + cross-checking each block vs its BAL.
- **Pro:** potentially collapses weeks → hours; **Con:** depends on someone having &
  sharing the artifact; tests execution only on live blocks going forward (fine for an
  indefinitely-running node).

## Option C — Hybrid (recommended sequencing)
1. **Send the Option-B ask now** (cheap, async, high upside). In parallel:
2. **Start Option-A scaffolding** (fork, build, systemd, feeder, equiv-daemon, dash)
   so we make progress regardless of the answer.
3. When B lands: bulk-import and jump to tip. If B never lands: A is already running.
- This way we don't block on an external dependency, and we don't waste the wait.

## My recommendation
**Option C.** It's the only one that neither gambles on an external artifact nor
eats weeks unnecessarily. The build work (binary-node + feeder + equiv-daemon +
dashboard) is identical either way — only the *initial state bootstrap* differs — so
starting the build now is zero-waste.

## Decision needed from you
1. **Approve Option C** (build now + send the artifact ask), or pick A or B outright.
2. If C/B: are you willing/able to **contact the ethrex (lambdaclass) / EF verkle
   team** for a mainnet preimage export or pre-built binary snapshot? (I can draft the
   message; you send it.)
3. If A (or while waiting): **genesis** re-exec, or **post-merge (~15.5M)** start via
   an MPT archive_sync bootstrap for the reference side?
