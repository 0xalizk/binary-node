# CLAUDE.md — pir-ubt-node (EIP-7864 binary-node + its mainnet reference)

> Resuming work? Read HANDOVER.md (this directory) FIRST -- live state and the exact next steps.

Host `pir-ubt-node` (DigitalOcean, 32 vCPU / 251 GB RAM / ~7 TB NVMe on `/`). Dedicated to the
**binary-node**: an EIP-7864 binary-trie "shadow" of mainnet that verifies binary↔MPT *value*
equivalence per block, plus its own mainnet ethrex+lighthouse pair as the block source + MPT
reference. The original bootstrap ran on sibling host `ubt-node`; this is the provisioned box it
was migrated to (the 400 GB migrated trie was copied here, not re-bootstrapped).

## Layout
- `~/sharded-pir/binary-node/` — the project repo (`github.com/0xalizk/binary-node`): tooling, **`docs/`**,
  and the `planning/` journal. Start at `docs/replication.md` (runbook), `docs/architecture.md`,
  `docs/design-rationale.md`, `docs/audit_1_24_06.md` (code audit).
- `~/sharded-pir/binary-node/ethrex/` — the ethrex fork, branch `feat/migrate-seed-and-catchup` (the binary-trie node +
  `migrate` / `seed-head` / `seed-code` / `catch-up`). Build: `cd ~/sharded-pir/binary-node/ethrex && cargo build
  --release --bin ethrex` → `~/sharded-pir/binary-node/ethrex/target/release/ethrex`.
- `~/sharded-pir/binary-node/bn-datadir/` — the migrated binary-trie state (~400 GB), checkpoint **block
  25,340,000**, real head hash `0xbcaad1b0…`, code-complete (seed-head + seed-code applied).
  This is the binary-node's datadir.
- `~/jwt.hex` — shared JWT for the local mainnet pair.

## The two nodes
- **Mainnet pair (ethrex EL + lighthouse CL)** — the MPT reference + the block source `catch-up`
  pulls from. Default ports (EL: http 8545 / authrpc 8551 / metrics 9090 / p2p 30303; CL: 5052).
  Snap-synced (EL) + checkpoint-synced (CL).
- **Binary-node** — the *fork* binary run on `bn-datadir`, `--p2p.disabled`, fed by the mainnet
  pair. Use **distinct ports** (e.g. http 8645 / authrpc 8651 / metrics 9190) so it doesn't clash
  with the mainnet EL.

## Phase / plan (bootstrap steps 0–7)
Steps 0–5 (toolchain → geth export → Xatu distinct → preimages → re-sort+migrate → seed+launch)
are **done** (on ubt-node; datadir copied here). Remaining:
- **6 — catch-up:** `ulimit -n 1048576; ~/sharded-pir/binary-node/ethrex/target/release/ethrex --datadir
  ~/sharded-pir/binary-node/bn-datadir --network mainnet catch-up http://127.0.0.1:8545 [--to <block>]`.
  Re-executes blocks 25,340,001 → tip against the binary trie. **Validate on 10 blocks first**
  (`--to 25340010`) — it has *never* completed a block end-to-end. It is crash-safe / resumable
  (resumes from `max(latest, checkpoint)`); wrap in a re-run loop to converge on the moving tip.
  Requires the local mainnet node synced enough to serve block bodies back to 25,340,000.
- **7 — feeder + equiv-daemon + Grafana:** **NOT BUILT.** The per-block binary↔MPT value-compare
  loop (BAL-driven) + dashboard is the next thing to build. Design in `docs/architecture.md`.

## Key facts / constraints
- **FD limit:** run `ulimit -n 1048576` (or `LimitNOFILE=1048576` in a systemd unit) for any
  ethrex op that opens `bn-datadir` — migrate / seed-code / catch-up / node run. RocksDB opens
  many SSTs; the default limit triggers "too many open files".
- **Coverage gap:** ~1.78% of accounts/slots lacked a preimage and were skipped at migrate time
  (non-execution-diff sources — block rewards, beacon withdrawals, genesis, internal transfers —
  not in Xatu's diff tables). Known + accepted; the equiv-daemon will flag those as expected holes.
  Details: `docs/design-rationale.md`, `docs/audit_1_24_06.md`.
- **Node-safety:** dedicated NVMe box (none of ubt-node's shared-virtual-disk contention), but the
  mainnet pair is live — still cap genuinely heavy parallel jobs and watch the CL `sync_distance`
  (`curl -s 127.0.0.1:5052/eth/v1/node/syncing`).
- This box has **no Grafana / Prometheus / Teleport-app exposure** (unlike ubt-node) — just the nodes.

## Known open items (from the code audit, mostly defensive)
- catch-up's first full run is unvalidated — hence validate-small-first. RPC retry + resume guard
  are in place (commit on the feat branch).
- Minor hardening backlog (silent-drop guards in the resorter / preimage-builder) — see
  `docs/audit_1_24_06.md`; not blockers for running.
