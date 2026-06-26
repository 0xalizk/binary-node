## binary-node — EIP-7864 binary-trie shadow node

Maintained documentation for the binary-node setup: a forked ethrex node that mirrors mainnet
state in an **EIP-7864 binary state tree**, runs as a p2p-disabled shadow of a normal MPT
node, and verifies binary↔MPT value-equivalence per block. This `docs/` tree is the
**current truth** and the basis for a community replication guide. (For how we got here —
the research, dead-ends, and decisions — see `../planning/`.)

### Why this exists

There is a proposal to replace Ethereum's MPT state representation with an EIP-7864 binary
tree. Before adoption, the reorganization must be shown to preserve state exactly. binary-node
is the differential test: same mainnet blocks executed into both an MPT (the live reference
node) and a binary tree (this node), with a daemon comparing values block-by-block at the tip.

### Documents

- `architecture.md` — the two-node shadow design and components.
- `replication.md` — the reproducible procedure: prerequisites → bootstrap → run the shadow.
  (The headline doc; status-tagged as the mainnet bootstrap completes.)
- `design-rationale.md` — why geth-snapshot + Xatu-preimages; the options rejected and why
  (pitfalls a replicator should avoid).
- `reference/` — durable technical specs (gethdbdump format, migrate, client schemas).

### Status

Mainnet **bootstrap complete**: state migrated into the binary trie; the node boots and serves
correct balance/nonce/code at the checkpoint block. **Catch-up** to the tip is built but its first
full run awaits NVMe-class hardware; the **live feeder + equivalence-daemon + dashboard are not
yet built**. Steps are status-tagged ([done] / [NOT BUILT]) so this doc never runs ahead of what's
actually been done.
