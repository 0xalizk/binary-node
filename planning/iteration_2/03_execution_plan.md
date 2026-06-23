# Execution plan — binary-node + equiv-daemon (DRAFT for approval)

> Contingent on one open decision: the equiv-check timing option in
> `02_q3_recommendation.md` (I've planned for **Option 1, two-phase**). If you pick 2/3,
> §6 changes.

## Locked decisions (from your answers + research)

- **Goal:** a binary-trie ethrex node ("binary-node") in sync with mainnet block by
  block, all RPCs working, with an equiv-daemon validating binary↔MPT value
  equivalence and a Grafana dashboard. Perf is secondary; collect state size +
  read/write speed.
- **mod #2 (binary trie):** fork ethrex `eip-7864-plan` (our own fork; borrow from
  `shared-trie`/geth as useful). Custom build from source on this box.
- **mod #1 (isolation):** binary-node runs with **p2p disabled**; blocks delivered by
  a **custom feeder** pulling from mainnet-node. (Strictly satisfies "no ethereum
  peers" — more isolated than one-static-peer, which ethrex can't reliably do.)
- **Sync strategy (#3):** **genesis full re-execution** (path a). No snap, no Geth
  migrate path.
- **Equiv-check:** continue-but-record; **halt at 1000 discrepancies** →
  investigate/fix/document/restart. Compare values via
  `getBalance`/`getTransactionCount`/`getCode`/`getStorageAt` (never proofs/roots).
- **Dashboard:** reuse existing Grafana/Prometheus. Keep discrepancy tuples
  `(block, address, slot, value_mpt, value_binary)` in a small local store; surface
  counts + latest offenders + progress + state size + exec speed.
- **Lifetime:** run indefinitely (future RPC data source).
- **Host:** this box only (8 cores / 61 GB / 7.4 TB), alongside the live mainnet
  pair.

## Architecture

```
            ┌─────────────────────┐    blocks (RPC pull)     ┌──────────────────────┐
            │   mainnet-ethrex     │ ───────────────────────▶ │       feeder         │
            │  (MPT, live, tip)    │                          │ (pull → import/feed) │
            │  RPC :8545           │ ◀── recent-state queries │                      │
            └─────────────────────┘        (tip phase)        └──────────┬───────────┘
                      ▲                                                    │ ethrex import / Engine API
                      │ recent-state queries (tip phase)                   ▼
            ┌─────────┴───────────┐                            ┌──────────────────────┐
            │     equiv-daemon     │ ◀── recent-state queries ─ │     binary-node       │
            │ (compare, metrics,   │                            │ (eip-7864 fork, full  │
            │  discrepancy store)  │ ── /metrics ─▶ Prometheus  │  re-exec, p2p OFF)    │
            └─────────────────────┘                            │  RPC :8547 (new)      │
                      │                                          └──────────────────────┘
                      ▼
                  Grafana (existing) — green/red, progress, state size, speeds
```

Ports/paths (proposed, all localhost): binary-node RPC `127.0.0.1:8547`, authrpc
`8553`, metrics `9092`; datadir `~/.local/share/ethrex/binary-mainnet`; equiv-daemon
metrics `127.0.0.1:9200`. (Avoids all mainnet-node ports.)

## Phased work

### Phase 0 — Prep, resource budget, safety rails
- **Disk budget:** estimate binary-node datadir (FKV latest-only; Hoodi was ~33M
  accts/268M slots → mainnet much larger; assume several hundred GB, verify during
  catch-up). Confirm headroom against mainnet-node datadir within 7.4 TB; set a hard
  disk alarm.
- **CPU/IO isolation so binary-node can't starve the live pair:** run binary-node +
  feeder under systemd with `CPUQuota` (e.g. cap ~5 of 8 cores), `IOWeight` low,
  `Nice`. Accept slower catch-up in exchange for protecting mainnet-node/CL.
- Toolchain: install Rust, clone fork, baseline build.

### Phase 1 — Build & smoke-test binary-node
- Fork `lambdaclass/ethrex@eip-7864-plan` → our repo. Build release.
- Configure: mainnet network/genesis, p2p disabled (`--p2p.disabled`),
  `--syncmode full`, new ports, datadir.
- Smoke test: import first N mainnet blocks from genesis via `ethrex import`; confirm
  receipts/gas/BAL validations pass and RPC reads work.
- systemd unit `binary-node.service` (User=0xalizk, Restart=always, resource caps).

### Phase 2 — Feeder (catch-up + live)
- Service that: (catch-up) streams blocks genesis→tip from mainnet-node, batches into
  `ethrex import` with **resume** (branch supports import-resume); (live) once at tip,
  polls mainnet-node head and feeds new blocks as they arrive.
- Source blocks from mainnet-node RPC (`eth_getBlockByNumber` full / raw block) — no
  devp2p. Track + expose feed lag (binary head vs mainnet head).
- **This is the long pole** (weeks of catch-up). Emit ETA from observed blocks/sec.

### Phase 3 — equiv-daemon
- **Catch-up mode:** record per-block intrinsic validation outcome (receipts/gas/
  requests/BAL pass = green) + metrics (state size from binary-node, exec speed,
  blocks behind). Discrepancy = a block that fails an enforced (non-state-root)
  validation, or import error.
- **Tip mode (auto-switches when lag ≈ 0):** per new block, derive **touched keys**
  (prefer the block's access list / BAL if the active fork provides one; else
  `debug_traceBlock` prestate-diff tracer on mainnet-node), then query both nodes for
  each touched account/slot and compare. Record `(block, addr, slot, v_mpt,
  v_bin)` tuples to a small local SQLite; increment Prometheus counters; **halt at
  1000 discrepancies**.
- Expose `/metrics` (blocks checked, keys/block, mismatches by type, lag, last-green
  block, state size, exec speed). Optional `/discrepancies` JSON for the dashboard
  table.

### Phase 4 — Prometheus + Grafana
- Add Prometheus scrape jobs: equiv-daemon (`:9200`), binary-node metrics (`:9092`).
- New Grafana dashboard "binary-node equivalence": top-line green/red status, catch-up
  progress (binary head vs mainnet head), blocks checked, discrepancy count + latest
  offenders table, state size (both nodes), read/write speeds. Datasource pinned to
  `prom-001` (per our hard-won lesson).

### Phase 5 — Steady state
- Feeder follows head; equiv-daemon value-checks every new block indefinitely. binary-
  node becomes the RPC data source for the next project.

## Risks / watch-items
- **Catch-up time:** full mainnet re-exec on a capped 8-core box ⇒ weeks; may lag tip
  for a long time before phase-2 value checks begin. (Mitigation: could temporarily
  raise CPUQuota during low mainnet activity.)
- **Branch maturity:** mainnet state correctness unproven on this branch; expect to
  *find* divergences — that's the point, but it means iterating on the fork (your
  ≤1000-then-halt loop). Budget for ethrex/LEVM debugging.
- **Reorgs:** deep-reorg handling on the branch is weak; mainnet reorgs are usually
  1–2 blocks (fine), but a deep reorg during catch-up/tip may need manual recovery.
- **Disk:** a second full state on the shared box — monitor closely.
- **Equiv coverage gap:** with Option 1, historical blocks get execution-equivalence
  (receipts/gas/BAL), not value-by-value diffs. Upgrade to Option 2 later if needed.

## What I need from you to start
1. **Confirm equiv-check Option 1** (or pick 2/3) — `02_q3_recommendation.md`.
2. **Ack the catch-up-time + branch-maturity risks** (weeks to tip, expect to debug
   real divergences).
3. **Approve the plan** (or redline). Then I'll start at Phase 0.
