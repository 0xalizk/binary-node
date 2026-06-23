## Design rationale & rejected options

Why the bootstrap is geth-snapshot values + Xatu preimages, and the dead-ends a replicator
should not repeat. Full evidence in `../planning/iteration_3/` and `../planning/iteration_4a_ethrex/`.

### The core problem: preimages

State is keyed by hashes, and the two trees use *different* hashes:
- MPT: account at `keccak256(address)`, storage at `keccak256(slot)`.
- EIP-7864 binary tree: keys derived via **BLAKE3** over the *raw* address/slot.

You cannot convert `keccak256(address) → BLAKE3 key` directly — keccak is one-way, and BLAKE3
needs the raw `address` as input. The raw address/slot is the only bridge:

```
keccak256(addr)  --preimage lookup-->  addr  --BLAKE3 derivation-->  binary-trie key
```

So migrating MPT state to a binary tree requires a **complete set of preimages** (raw
addresses + slot keys). An incomplete set yields a structurally incomplete tree — migrate
cannot compute keys for the missing accounts and silently skips them.

### Rejected: reth snapshot as a preimage source

reth v2.2.0 stores current state **hashed** (`HashedAccounts`/`HashedStorages`), keyed by
`keccak256`. Its plain-state tables (`PlainAccountState`/`PlainStorageState`) are **empty** in
the v2 storage layout. So a reth snapshot contains **zero preimages** — it cannot un-hash any
address. (Verified empirically: `../planning/iteration_3/07_CRITICAL_reth_has_no_preimages.md`.)
Pitfall: the table *definitions* exist in source, which misleads if you don't check runtime
population.

### Rejected: geth snapshot's `--cache.preimages` alone

geth keeps a `keccak→preimage` table, but a **snap-synced** node only records preimages for
keys touched during local execution. Dormant accounts (downloaded as hashed state, never
re-executed) have **no preimage**. Measured on the ethPandaOps Hoodi geth snapshot: preimages
for only **34.16M of 44.43M accounts (~77%)** — migrate skipped 10.27M entries. All public
ethPandaOps snapshots are snap-synced (no archive variant), so this is structural, not a
fluke; mainnet is the same or worse. (`../planning/iteration_3/08_CRITICAL_geth_preimages_incomplete.md`.)

### Chosen: geth snapshot (values) + Xatu (preimages)

- **geth snapshot** provides authoritative current state **values** at the snapshot block
  (`db export snapshot` + `db export code`), hash-keyed. It's also the MPT oracle the
  equiv-daemon checks against.
- **Xatu `canonical_execution_*`** provides the **complete preimages**: plain addresses and
  slot keys recorded **from block 0**, so dormant accounts are included. We take only the
  distinct keys (ignore Xatu's values) and keccak them into the preimage file.

geth supplies *what accounts hold*; Xatu supplies *what they're addressed by*. migrate joins
them. This was ethPandaOps' own suggestion after we showed the snapshot preimages were
incomplete. (`../planning/iteration_4a_ethrex/01_xatu_preimage_plan.md`, `03_decided_approach.md`.)

### Rejected transfer/exposure approaches (context for ops)

- httpfs-streaming all Xatu Parquet in one query: too fragile at scale (SSL drops abort the
  whole scan). Use resilient per-file download + local processing.
- Exposing node status by letting a public site poll the box: the box is `127.0.0.1`-only
  behind authenticated Teleport. Status is **pushed outbound** instead (see iteration_5).
