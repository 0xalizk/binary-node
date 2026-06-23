# 🔖 RESUME HERE — binary-node project (iteration 3, 2026-06-17)

**This is the current resume point. It supersedes `../iteration_2/06_STATUS_resume_here.md`.**

## ▶ PHASE 0 IN PROGRESS (started 2026-06-17, Option D approved)
Workspace: `~/sharded-pir/binary-node/`. Done / in flight:
- ✅ Toolchain: Rust 1.96.0 (rustup) + system deps (clang/llvm-19, cmake, pkg-config,
  libssl, git-lfs) installed via apt.
- ✅ ethrex fork cloned: `~/sharded-pir/binary-node/ethrex` @ `eip-7864-plan` (b0fe293).
  LFS test fixtures skipped (GIT_LFS_SKIP_SMUDGE). NOT yet forked to our GH — clone of
  upstream; fork when we need to push changes (D2 keeps migrate unmodified, may not need to).
- 🔄 Baseline build RUNNING in background: `cargo build --release --bin ethrex`,
  niced -19 / -j6, log `~/sharded-pir/binary-node/build.log` (pid was 311194; re-check).
- 🔄 Hoodi reth snapshot DOWNLOADING (smoke-test source): `…/snapshots/hoodi-reth/`
  (block 3,030,000, 45.3 GiB, resumable curl). Background task id `benn56pgy`.
- ✅ **Extractor written** (NOT built yet): `~/sharded-pir/binary-node/reth-state-extractor/`
  (`Cargo.toml` + `src/main.rs`). Reads reth `PlainAccountState`/`PlainStorageState`/
  `Bytecodes`, emits the 3 gethdbdump files. Deterministic parts (RLP/keccak/framing)
  byte-exact per `06_gethdump_format.md`; reth-db cursor API written to v2.2.0 docs — **expect
  to fix import paths/signatures on first `cargo build`** (and possibly pin alloy-primitives
  to reth's Cargo.lock version if duplicate-version errors appear).
- 📄 **`06_gethdump_format.md`** (this folder) = byte-exact wire format the extractor emits,
  reverse-engineered from `migrate.rs` (header `0xc0`; op `0x80`; preimage switched on value
  len 20/32; slim acct `[nonce,balance,"",codehash]`; storage `RLP(trimmed-BE)`; code `'c'+hash`).

### ⛔ PIVOT (2026-06-17): reth is NOT a preimage source — use GETH instead
Hoodi smoke test proved reth v2.2.0 stores state **hashed** (`HashedAccounts`/
`HashedStorages`), Plain* tables EMPTY, no preimages. See
**`07_CRITICAL_reth_has_no_preimages.md`**. The `reth-state-extractor` reading half is
shelved (its gethdump-emitter spec `06_gethdump_format.md` is still valid — geth's native
export produces exactly that format). New source = **Geth snapshot w/ `--cache.preimages`**
(ethrex's documented path; geth keeps a preimage table reth lacks).

### Next concrete steps (resume here) — GETH path
1. ✅ Go 1.24.4 installed; patched geth fork cloned `~/sharded-pir/binary-node/go-ethereum`
   (`edg-l/go-ethereum feat/export-code`, adds `geth db export code`). Building `./geth`.
2. 🔄 Hoodi GETH snapshot downloading: `…/snapshots/hoodi-geth/` (block 3,030,000,
   84.5 GiB, task `b17hqz5ju`). [reth hoodi snapshot at `…/snapshots/hoodi-reth/` now unused.]
3. Extract geth snapshot → `./geth --datadir <dir> --hoodi db export preimage preimages.rlp`,
   `… db export snapshot snapshot.rlp`, `… db export code code.rlp`.
4. **Verify preimage completeness:** count addr-preimages (val len 20) vs 44,431,577
   accounts (known from reth diag); slot-preimages (val len 32) vs distinct slot keys.
   THIS is the real gate — confirms geth's `--cache.preimages` is complete on a snap-synced
   ethpandaops node (the one risk; see CRITICAL doc).
5. `ethrex --network hoodi migrate preimages.rlp snapshot.rlp --code code.rlp --at-block 3030000`
   (drop `--fast` only if RAM-tight; geth's preimage files ARE hash-sorted so mmap mode is fine).
6. Start migrated node, confirm forward sync from 3.03M + correct RPC values. GATE.
7. Only then mainnet GETH download (1018 GiB, block 25,330,000) + full run.

⚠️ Build + download together pushed load to ~7/8 cores; node stayed synced (niced build).
Keep watching node health (`curl -s 127.0.0.1:5052/eth/v1/node/syncing`) when adding load.

## What we're building (unchanged)
A modified ethrex node ("binary-node") that mirrors mainnet state in an **EIP-7864
binary tree** (not MPT), fed only by our existing mainnet ethrex node ("mainnet-node"),
plus an **equiv-daemon** that verifies binary↔MPT state value equivalence per block,
with a Grafana/Prometheus dashboard (green/red).

## 🆕 WHAT CHANGED THIS ITERATION
The iteration_2 blocker — "no downloadable mainnet preimage dataset, so bootstrap is
genesis re-execution (days–weeks) or beg the EF team" — **was wrong.** ethpandaops
publishes fresh, public, full mainnet client snapshots, and **reth/erigon snapshots are
a complete preimage source by construction** (plain state keyed by raw address/slot).
Full details + verified data in **`01_snapshot_findings.md`**. Updated options in
**`02_options_reconsidered.md`**.

Net effect: bootstrap drops from days–weeks → **hours-to-a-day** (download + build),
with **zero external dependency**. Option B (outreach) dropped; Option A (genesis
re-exec) demoted to fallback.

## ⏸️ WHERE WE STOPPED — DECISION PENDING FROM USER (now 2 questions, was 3)
See `02_options_reconsidered.md`:
1. **Approve Option D** — bulk-build the binary tree from the **reth** mainnet snapshot
   (704 GiB, block 25,330,000), vs. erigon as the source, vs. fallback to genesis
   re-execution (A)?
2. **Bootstrap block = 25,330,000** (post-merge, ~1 day old / ~tip) — confirm OK.
   (Genesis vs post-merge is moot now: our non-archive nodes can only do value-level
   equiv at the moving tip anyway.)

**Claude's recommendation: Option D from reth.** Smallest download, plain-state =
guaranteed-complete preimages, no permission needed.

## The snapshot, concretely (verified live 2026-06-17)
- Source: `https://snapshots.ethpandaops.io/mainnet/reth/25330000/snapshot.tar.zst`
  (704.3 GiB compressed). Block ts: Tue Jun 16 2026 12:12:11 UTC.
- Erigon fallback: `…/mainnet/erigon/25330000/snapshot.tar.zst` (968 GiB).
- Latest-block probe: `curl -s https://snapshots.ethpandaops.io/mainnet/reth/latest`
- Disk free on box: 6.9 TB of 7.7 TB — fits.

## Planning docs (read order)
- `../iteration_1/01_questions.md` + `02_answers.md` — the 14 Q&A (decisions locked).
- `../iteration_2/01_branch_findings.md` — what ethrex `eip-7864-plan` actually does.
- `../iteration_2/02_q3_recommendation.md` — sync-strategy reasoning + no-archive constraint.
- `../iteration_2/04_preimage_path_findings.md` — **SUPERSEDED** by `01_snapshot_findings.md`
  (its core conclusion is now wrong; keep for history).
- **`01_snapshot_findings.md`** (this folder) — the ethpandaops finding; the new truth.
- **`02_options_reconsidered.md`** (this folder) — the pending decision (D / A).
- `../iteration_2/03_execution_plan.md` — full draft plan; bootstrap section to be revised
  for Option D (snapshot download → plain-state extractor → migrate → feeder-from-tip).

## Decisions already LOCKED (from 02_answers.md) — still hold
- Goal: binary-node in sync block-by-block, all RPCs working, equiv-daemon validating,
  dashboard up. Perf secondary (collect state size + read/write speed).
- mod #2 (binary trie): fork ethrex **`eip-7864-plan`**, treat as our own fork.
- mod #1 (isolation): binary-node **p2p disabled**; blocks via a **custom feeder**
  pulling from mainnet-node.
- equiv-check: binary-node **executes** and **cross-checks against the block's BAL**
  (EIP-7928) as oracle — NOT blind-apply. Continue-but-record; **halt at 1000
  discrepancies**. Compare values (getBalance/getTransactionCount/getCode/
  getStorageAt) never proofs/roots.
- Dashboard: reuse existing Grafana/Prometheus; discrepancy tuples
  (block, addr, slot, v_mpt, v_bin) in a small local store. Datasource `prom-001`.
- Host: this box only (8 cores / 61 GB / 7.4 TB). Rate-limit binary-node
  (CPUQuota/IOWeight/Nice) so it can't starve mainnet-node.
- Lifetime: run indefinitely (future RPC data source).

## 🔄 What changes vs. iteration_2 (Sync strategy #3)
- **Sync #3 was "genesis full re-execution."** Now: **bulk-build from reth snapshot at
  block 25,330,000, then feeder-driven execution from 25,330,001 forward.** Genesis
  re-exec is the documented fallback only.

## Key technical constraints (don't re-derive)
- Binary tree → different state root than MPT by design ⇒ binary-node is a **shadow
  executor** with stateRoot validation removed (already done on `eip-7864-plan`;
  receipts/gas/requests/BAL checks still enforced).
- Both mainnet-node and binary-node are **non-archive** (~128-block window) ⇒ value-
  level equiv checks only work at the moving tip. (Reinforces: bootstrap-at-tip is
  fine; an old/genesis start bought the equiv-daemon nothing.)
- binary-node RPCs we need (getBalance/getCode/getStorageAt/getTransactionCount) work
  on the branch; `eth_getProof` is stubbed — we don't need it.
- `eip-7864-plan` only supports `--syncmode full` (snap stubbed).
- reth plain state = `PlainAccountState`/`PlainStorageState`, raw-address keyed; geth
  `--cache.preimages` is likely PARTIAL on a snap-synced node — don't use geth as the
  source without proving completeness.

## ✅ Verification DONE (2026-06-17) — both questions answered
Full results: `04_reth_schema_findings.md`, `03_ethrex_migrate_findings.md`; the resulting
extractor architecture: **`05_extractor_design.md`**. Headlines:
- reth v2.2.0 plain state = `PlainAccountState`/`PlainStorageState`/`Bytecodes`, raw-key.
  **`--full` does NOT prune plain state → snapshot state is COMPLETE.** State is MDBX-only.
- ethrex `migrate` is REAL, streams to disk, bootstraps at `--at-block`, derives tree
  keys itself, **skips the slim-RLP storage root** (so reth's missing storage root is fine).
- Input is **Geth `gethdbdump` format** (keccak-keyed + preimage file), NOT reth tables.
- **Chosen bridge = D2:** a standalone Rust extractor (links `reth-db` only) reads reth →
  emits the three gethdump streams → ethrex `migrate` runs UNMODIFIED. Avoids pulling
  reth into ethrex's build, and avoids the patched-Geth dependency. See 05_extractor_design.md.

## ⚠️ Biggest open risk — validate EARLY
Forward-sync from a mid-chain `--at-block` is only **claimed in docs**, not proven. If
ethrex can't resume execution at block 25.33M from migrated state, the whole bootstrap
premise fails. **Smoke-test on Hoodi** (small reth snapshot) end-to-end BEFORE the
704 GiB mainnet download. (05_extractor_design.md §"Biggest open risk".)

## First actions next session (once D approved)
1. **Smoke test on Hoodi first** — extractor → migrate → confirm forward-sync + correct
   RPC values on a small network. Gate before committing to mainnet.
2. Phase 0 in parallel: install Rust toolchain, fork/clone `eip-7864-plan`, baseline
   build, plan systemd resource caps + disk budget. (`../iteration_2/03_execution_plan.md` Phase 0–1.)
3. Then mainnet: download reth snapshot (`…/mainnet/reth/latest`, 704 GiB, background) →
   run extractor → `ethrex migrate … --at-block <N>` → feeder + equiv-daemon + dashboard.

## ⚠️ Terminal/session note
User may not be running `screen` — session can be lost on disconnect.
**`tsh ssh ubt-node` → `screen -dR work` → `claude` (inside screen)** so it survives.
A 704 GiB download MUST run detached (background/screen), not in the foreground.
