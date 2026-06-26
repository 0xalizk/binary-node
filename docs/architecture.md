## Architecture

Two ethrex processes on one host, processing the identical mainnet block stream into two
different state representations, with a daemon comparing them.

> **Implementation status:** the vanilla node, the binary-node, and its migrate/seed/catch-up
> bootstrap are built and working. The **feeder, equiv-daemon, and dashboard described below are
> the design — not yet implemented.**

### Components

- **Vanilla node (reference, authoritative):** stock ethrex, MPT state, p2p-synced to mainnet
  head. Ground truth; never modified. (On our host: ethrex v16.0.0, `ethrex.service`.)
- **binary-node (shadow):** forked ethrex ([`0xalizk/ethrex`](https://github.com/0xalizk/ethrex/tree/feat/migrate-seed-and-catchup),
  `feat/migrate-seed-and-catchup`, on `lambdaclass/ethrex`'s `eip-7864-plan`) — state held in an
  EIP-7864 binary tree instead of an MPT. **Its own datadir; p2p disabled.** It does not sync
  itself.
- **Feeder:** pulls each new canonical block from the vanilla node and hands it to the
  binary-node, which executes it and updates the binary tree. Both clients thus process the
  same blocks — one into an MPT, one into a binary trie.
- **equiv-daemon:** per block, compares value-level state (`getBalance` / `getTransactionCount`
  / `getCode` / `getStorageAt`) between the two nodes, using the block's EIP-7928 BAL
  (block access list) as the touched-key oracle. Records discrepancies `(block, addr, slot,
  v_mpt, v_bin)`; halts at a threshold (1000). Compares **values, never roots/proofs** — the
  two trees' roots differ by design.
- **Dashboard:** Prometheus + Grafana — discrepancy counts, latest offenders, sync progress,
  state size, read/write speeds.

### Why values, not roots

An EIP-7864 binary tree and an MPT produce different state roots for the same state by
construction (different structure and hash: BLAKE3-derived keys vs `keccak256`). So
equivalence is checked at the **value** level — for every account/slot touched in a block,
the two nodes must agree on balance/nonce/code/storage — not by comparing roots.

### Isolation

Three-way: separate binary, separate datadir, p2p off (fed only by the feeder). The
binary-node cannot perturb the reference node. The only shared resource is host CPU/disk; the
binary-node is rate-limited (nice / IOWeight) so the reference node keeps priority.

### Non-goals

- **Not archival.** The branch keeps a ~128-block state window; equivalence is checked at the
  moving tip. Historical-state queries, if ever needed, are better served from Xatu
  (genesis-onward diffs) than by making this node an archive. See `design-rationale.md`.
