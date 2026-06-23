> ⚠️ **SUPERSEDED 2026-06-17 — resume from `../iteration_3/10_STATUS_resume_here.md`.**
> The preimage constraint that drove the A/B/C decision below was overturned (fresh
> public mainnet snapshots exist). Kept for history.

# 🔖 RESUME HERE — binary-node project (paused 2026-06-16)

## What we're building
A modified ethrex node ("binary-node") that mirrors mainnet state in an **EIP-7864
binary tree** (not MPT), syncing only via our existing mainnet ethrex node
("mainnet-node"), plus an **equiv-daemon** that verifies binary↔MPT state value
equivalence per block, with a Grafana/Prometheus dashboard (green/red).

## ⏸️ WHERE WE STOPPED — ONE DECISION PENDING FROM USER
User will give their decision tomorrow. The decision is in
**`05_options_reconsidered.md`** — three parts:
1. **Approve Option C** (hybrid: start building now + ask EF/ethrex team for a
   preimage dataset / pre-built binary snapshot) — or pick **A** (self-contained
   re-execution, no deps, days–weeks) or **B** (wait on the external artifact).
2. If C/B: is the user willing to **contact lambdaclass (ethrex) / EF verkle team
   (jsign, gballet)** for a mainnet preimage export or pre-built binary snapshot?
   (Claude drafts the message; user sends. User is at ethereum.org.)
3. If A / while waiting: bootstrap from **genesis** or **post-merge (~15.5M)**?

**Claude's recommendation: Option C.** The build work is identical regardless of how
we bootstrap initial state, so starting now is zero-waste; the artifact ask is cheap
and could collapse weeks → hours.

## Planning docs (read in this order)
- `planning/01_questions.md` + `planning/02_answers.md` — the 14 Q&A (decisions locked).
- `planning/iteration_2/01_branch_findings.md` — what ethrex `eip-7864-plan` actually does.
- `planning/iteration_2/02_q3_recommendation.md` — sync-strategy reasoning + the
  no-archive constraint.
- `planning/iteration_2/04_preimage_path_findings.md` — why there's no off-the-shelf
  fast bootstrap.
- `planning/iteration_2/05_options_reconsidered.md` — **the pending decision (A/B/C).**
- `planning/iteration_2/03_execution_plan.md` — the full draft plan (assumes the older
  "Option 1 two-phase" equiv-check; will be revised once A/B/C is chosen).

## Decisions already LOCKED (from 02_answers.md)
- Goal: binary-node in sync block-by-block, all RPCs working, equiv-daemon validating,
  dashboard up. Perf secondary (collect state size + read/write speed).
- mod #2 (binary trie): fork ethrex **`eip-7864-plan`**, treat as our own fork; borrow
  from `shared-trie`/geth as useful.
- mod #1 (isolation): binary-node **p2p disabled**; blocks via a **custom feeder**
  pulling from mainnet-node (NOT devp2p static-peer — ethrex has no reliable one).
- Sync (#3): **genesis full re-execution** (snap can't feed a binary trie; no Geth).
- equiv-check: binary-node **executes** and **cross-checks against the block's BAL**
  (EIP-7928) as the oracle — NOT blind-apply (that would be tautological). Continue-
  but-record; **halt at 1000 discrepancies** → fix/document/restart. Compare values
  (getBalance/getTransactionCount/getCode/getStorageAt) never proofs/roots.
- Dashboard: reuse existing Grafana/Prometheus; keep discrepancy tuples
  (block, addr, slot, v_mpt, v_bin) in a small local store; show counts + latest
  offenders + progress + state size + speeds. Datasource hardcoded to `prom-001`.
- Host: this box only (8 cores / 61 GB / 7.4 TB), alongside the live mainnet pair.
  Must rate-limit binary-node (CPUQuota/IOWeight/Nice) so it can't starve mainnet-node.
- Lifetime: run indefinitely (future RPC data source for the NEXT project).

## Key technical constraints (don't re-derive these)
- Binary tree → different state root than MPT by design ⇒ binary-node can't follow
  mainnet via consensus; it's a **shadow executor** with stateRoot validation removed
  (already done on `eip-7864-plan`; receipts/gas/requests/BAL checks still enforced).
- **No fast bootstrap exists today:** no downloadable mainnet preimage dataset; every
  preimage source needs Geth or re-execution. ethrex has no native preimage export.
- Both mainnet-node and binary-node are **non-archive** (~128-block state window) ⇒
  value-level equiv checks only work at the moving tip, not at arbitrary old blocks.
- binary-node RPCs we need (getBalance/getCode/getStorageAt/getTransactionCount)
  **work** on the branch; `eth_getProof` is stubbed (empty) — we don't need it.
- `eip-7864-plan` only supports `--syncmode full` (snap stubbed out).
- Reorg handling on the branch is weak (shallow OK); mainnet reorgs usually 1–2 blocks.

## ⚠️ Terminal/session note
User is NOT running `screen`, so this session will likely be lost on disconnect.
**Next time: `tsh ssh ubt-node` → `screen -dR work` → `claude` (inside screen)** so
the session survives. See CLAUDE.md / earlier convo for the screen steps.

## First actions next session (once A/B/C is chosen)
- If C/B: draft the artifact-request message for the user to send.
- If C/A: begin Phase 0 — install Rust toolchain, clone/fork `eip-7864-plan`, baseline
  build, plan systemd resource caps + disk budget. (See 03_execution_plan.md Phase 0–1.)
