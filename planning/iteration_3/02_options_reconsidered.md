# Options — re-reconsidered after the snapshot finding (2026-06-17)

Supersedes `../iteration_2/05_options_reconsidered.md`. The iteration_2 A/B/C revolved
around "there's no downloadable preimage dataset." That premise was false (see
`01_snapshot_findings.md`). New landscape:

## ❌ Option B — ask the EF/lambdaclass team for an artifact
**Dropped.** The artifact (a complete, ~1-day-fresh mainnet preimage source) is public
at ethpandaops. No outreach needed.

## ⬇️ Option A — self-contained re-execution from genesis
**Demoted to fallback.** Still the strongest *differential* test (executes every block
ever), but days–weeks to tip. Only fall back to this if the snapshot-extraction path
(D) proves unworkable.

## ✅ Option D — bulk-build from a reth/erigon snapshot (RECOMMENDED)
1. Download the **reth** mainnet snapshot (704 GiB compressed) at block 25,330,000.
   (Erigon, 968 GiB, is the equivalent fallback source.)
2. Extract its **plain state** — `(raw address → account)` and
   `(raw address+slot → value)`. This is the complete preimage set by construction.
3. Feed it to ethrex `eip-7864-plan`'s `migrate` to **bulk-build the EIP-7864 binary
   tree** at block 25,330,000.
4. **binary-node follows tip from block 25,330,001** via the custom feeder (p2p
   disabled), executing each block and cross-checking against its EIP-7928 BAL.
5. equiv-daemon + Grafana dashboard as already designed (unchanged).

- **Pro:** bootstrap in hours-to-a-day (download + build), **zero external dependency**,
  complete state, fresh. **Con:** adds one new component — a reth-DB plain-state
  extractor — and tests execution only on live blocks from 25.33M forward (which is
  exactly what the equiv-daemon needs anyway; our non-archive nodes can only do
  value-level equiv at the moving tip).

## Recommendation
**Option D, sourced from reth.** Smallest download, plain-state = guaranteed-complete
preimages, no permission needed. Keep Option A as the documented fallback. Skip geth
unless we deliberately want to verify its preimage table (we don't need to).

## Decisions needed from you  (was 3 questions, now 2)
1. **Approve Option D from the reth snapshot?** (vs. erigon source, vs. fallback A.)
2. **Bootstrap block = 25,330,000** (post-merge, ~tip) — confirm OK. Genesis start is
   moot now; the equiv-daemon only validates at the moving tip regardless.

## On approval — first actions
- Start the **reth snapshot download** in the background (704 GiB; resumable).
- In parallel begin **Phase 0**: install Rust toolchain, fork/clone `eip-7864-plan`,
  baseline build, plan systemd resource caps (CPUQuota/IOWeight/Nice so binary-node
  can't starve the live mainnet pair) + disk budget.
- Then: write the plain-state extractor; wire it to `migrate`; build at 25.33M; bring
  up the feeder + equiv-daemon + dashboard.
