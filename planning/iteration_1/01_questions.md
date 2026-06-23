# binary-node project — interview questions

Differential-testing harness for ethrex's EIP-7864 (binary tree) implementation
against its own MPT implementation, on live mainnet.

## The crux (confirm or correct this framing)

EIP-7864 changes how the state root is *computed* (binary tree, not MPT). So
**binary-node's computed state root for any block will NOT equal the `stateRoot` in
that block's mainnet header.** binary-node therefore *cannot* validate or follow
mainnet via normal consensus — every block would "fail" the state-root check by
design. So binary-node must run as a **shadow executor**: take mainnet's block
bodies/txs from mainnet-node, **re-execute them**, commit results to its binary
tree, and **skip/override stateRoot validation**. Execution is trie-agnostic (the
EVM produces the same state changes; only the *commitment* differs), so the binary
tree is purely an internal representation. This is also *why* mod #1 (only talk to
mainnet-node) is necessary — it can't peer with real mainnet because its
blocks/roots are incompatible.

---

## Research findings so far (context for your answers)

- **Crux confirmed:** binary tree → different state root than MPT; mainnet headers
  carry the MPT root, so binary-node can't follow mainnet via normal consensus.
  Shadow-executor model holds.
- **equiv-daemon is robust to spec churn:** we compare *state values* via RPC, not
  tree roots, so the unsettled hash function (spec=BLAKE3, geth=SHA256) and root
  format don't affect the differential test.
- **Preimage problem (affects Q3):** building a binary tree from an existing MPT
  snapshot needs the *unhashed preimages* of every address/storage slot (~40–70 GB
  for full mainnet). Option A (convert-then-follow) depends on whether ethrex
  retains these. Option B (genesis re-exec) sidesteps it (execution yields real
  keys).
- **ethrex already has EIP-7864 work (mod #2 not from scratch):** issue #6379;
  PR #6380 branch `eip-7864-plan` (monolithic, BLAKE3 binary_trie crate, imports
  10k Hoodi blocks, stateRoot validation removed, has `--at-block` + migration
  docs); PR #6471 `shared-trie` (clean abstraction); PR #6564 `shared-trie-binary`
  (cleaner backend, incomplete). All off-`main`.
- **ethrex sync (affects mod #1):** no static/trusted-peer flag exists; `--bootnodes`
  only seeds discovery. Full sync executes every block; `ethrex import` + Engine API
  run the same execute→commit pipeline with no p2p. stateRoot validation is
  mandatory on `main`, disabled only by source change (as `eip-7864-plan` does).

---

## A. Goal & success criteria

1. **What's the real objective?** Lean: *differential testing* — prove ethrex's
   EIP-7864 binary-tree implementation produces state equivalent to the
   battle-tested MPT, on real mainnet workload. Or also: performance numbers / path
   to production / spec-conformance evidence for the EIP? (Changes rigor vs. speed.)

2. **What does "done" look like?** e.g. "N consecutive blocks fully equiv-checked
   green," "caught and root-caused any divergence," or "a sustained live dashboard
   following head." Define the finish line.

## B. Sync strategy — the biggest fork

3. **Where does binary-node start its state?**
   - **(A) Convert-then-follow** *(strong lean)*: take a recent state snapshot from
     mainnet-node at block N, do a one-shot MPT→binary-tree conversion, then both
     nodes execute forward from N and equiv-check *new* blocks live. Tractable; gets
     a live green/red dashboard quickly.
   - **(B) Genesis re-execution**: binary-node re-executes all ~25.3M blocks from
     genesis building the binary tree. "Purest" but enormous (days–weeks, full
     archive); the check is historical, not live. Sidesteps the preimage problem.
   Which? (Could do A now, B later.) NOTE: `eip-7864-plan`'s `--at-block` flag +
   migration docs may already implement a conversion/start point — I can read those
   to see what's supported before you finalize.

4. **How do blocks get from mainnet-node to binary-node?** Lean: feed them via the
   Engine API / block import from a trusted single static peer (mainnet-node),
   discovery fully disabled. Do you care: devp2p-with-one-static-peer vs. a custom
   feeder that pulls blocks over RPC and submits them? (Research checking what
   ethrex actually supports.)

## C. Equivalence checking

5. **Granularity — "every bit of state read/written per block."** Lean: for each
   block, get the state access set (touched accounts + storage slots) via a
   `debug_traceBlock` prestate/poststate tracer or access list, then for each
   touched key compare across nodes: `eth_getBalance`, `eth_getTransactionCount`,
   `eth_getCode`, `eth_getStorageAt`, at that block height. Plus the full account
   set. Match your intent, or want something stronger (e.g. `eth_getProof`)?

6. **Querying at height** requires both nodes to answer historical-state queries at
   block H. Run both nodes in **archive mode**, or equiv-check each block
   *immediately at head* before advancing? Lean: check-at-head per block (no archive
   needed), since we're following live.

7. **On a discrepancy (red): halt or continue?** Lean: continue but record (find
   *all* divergences and their patterns), with an option to pause-on-first for
   debugging.

## D. equiv-daemon & dashboard

8. **Reuse the existing Grafana/Prometheus stack?** Lean: yes — equiv-daemon exposes
   Prometheus metrics (blocks checked, keys compared/block, mismatches by type, lag
   vs head, last-green block) and we build a Grafana dashboard with green/red
   indicators. Confirm, or want a standalone web UI?

9. **What granularity of "discrepancy" data to retain** — just counts, or the actual
   `(block, address, slot, value_mpt, value_binary)` tuples for forensics? Lean:
   keep the tuples (small local store / logs) and surface counts + latest offenders
   on the dashboard.

## E. Resources & build

10. **This box the only host?** 8 cores / 61 GB / 7.4 TB, already running
    mainnet-node (EL+CL+monitoring). A second full ethrex doubles EL CPU/IO and adds
    a second full state (~hundreds of GB–1TB+). Acceptable here, or separate
    machine? Any CPU/disk budget to respect (don't starve mainnet-node)?

11. **binary-node = custom build from ethrex source** (Rust) on whatever branch has
    the EIP-7864 work — vs mainnet-node which is the v16.0.0 release binary. OK to
    compile from source on this box? Preference for branch/fork if the ethrex team's
    7864 work is on one?

## F. Scope guardrails

12. **How far / how long** should the first cut sync and check — a fixed window
    (e.g. 10k blocks from head), or "run indefinitely following head until told to
    stop"? Lean: convert at head, follow live indefinitely, dashboard as the
    artifact.

## G. ethrex-specific decisions (surfaced by research)

13. **D1 — Which ethrex branch to base binary-node on?**
    - **`eip-7864-plan`** (PR #6380, monolithic) *(lean)* — most complete/runnable:
      already imports+executes blocks, disables stateRoot validation, has
      `--at-block` + migration docs. Diverged from main (ahead 120 / behind 200).
    - **`shared-trie-binary`** (PR #6564, on `shared-trie`) — cleaner architecture
      but incomplete (RPC, CLI activation, transition backend, E2E deferred).
    Which? (Also: do you want to coordinate with the ethrex team on their newer
    stack, or treat this as our own fork?)

14. **D2 — How are blocks delivered from mainnet-node to binary-node?**
    - **Custom feeder: pull blocks from mainnet-node RPC → `ethrex import` / Engine
      API into binary-node, p2p fully disabled** *(lean)* — deterministic, satisfies
      "no ethereum peers" strictly, doesn't rely on the missing static-peer feature.
    - **devp2p full sync with mainnet-node as sole bootnode + discovery off** — closer
      to your original wording but unreliable (no static-peer guarantee in ethrex).
    Which?
