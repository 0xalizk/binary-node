## binary-node

Bootstrapping an **EIP-7864 binary-trie** "shadow" Ethereum node from a public
snapshot, plus an equivalence daemon that checks binary↔MPT state-value equivalence
per block. Built on the ethrex `eip-7864-plan` branch.

**Status: blocked.** The snapshot-based bootstrap cannot produce a *complete* binary
tree because no public snapshot exposes a complete set of keccak **preimages** (the raw
addresses/slots EIP-7864 needs). This repo documents the blocker with reproducible
evidence and a specific ask (see [The ask](#the-ask) below).

### The blocker

The MPT keys state by `keccak256(address)` / `keccak256(slot)` (one-way). EIP-7864 keys
its tree off the **raw** address/slot, so migrating state requires reversing those
hashes — i.e. a complete preimage set. We tested both ethPandaOps snapshot clients at
the same Hoodi block (3,030,000) and neither provides one:

| source | state stored as | preimages available |
|---|---|---|
| **reth v2.2.0** | hashed (`HashedAccounts`/`HashedStorages`); Plain* tables empty | **none** |
| **geth v1.17.2** (`--cache.preimages`) | flat snapshot, hash-keyed | **~77% only** (10.27M of 44.43M accounts missing) |

Root cause: the snapshots are **snap-synced**, and geth's `--cache.preimages` only
records a preimage when a key passes the hasher during *local* execution — so
snap-downloaded-but-untouched accounts have state but no preimage. No archive variant is
published. Mainnet (identical config) would be the same or worse.

### Reproducing the geth result (Hoodi, block 3,030,000)

```
# 1. snapshot (84.5 GiB) from ethPandaOps, extract to a geth datadir
wget https://snapshots.ethpandaops.io/hoodi/geth/3030000/snapshot.tar.zst
tar --zstd -xf snapshot.tar.zst -C <datadir>/geth

# 2. export with patched geth (edg-l/go-ethereum@feat/export-code, == geth v1.17.2 + code exporter)
geth --datadir <datadir> --hoodi db export preimage preimages.rlp
geth --datadir <datadir> --hoodi db export snapshot snapshot.rlp
geth --datadir <datadir> --hoodi db export code     code.rlp

# 3. migrate (ethrex @ lambdaclass/ethrex eip-7864-plan, commit b0fe293)
ethrex --network hoodi migrate preimages.rlp snapshot.rlp --code code.rlp --at-block 3030000
```

The migration log (`migrate-hoodi.log`, included) shows the smoking gun:

```
Parsed 143,110,410 preimages (34,163,095 addrs, 108,947,315 slots)
Collection complete: 34,161,485 accounts, 272,878,767 storage slots
10,268,484 entries SKIPPED — "missing preimages"
```

44,431,577 is the true account count at that block (cross-checked against the reth
snapshot's `HashedAccounts`), so ~23% of accounts could not be un-hashed and were
dropped — an incomplete tree, which diverges on the first forward block.

### The ask

A source of **complete mainnet preimages**: a full-synced/archive snapshot with a
complete preimage table, a standalone `keccak→preimage` export for a recent block, or a
pointer to an existing dataset / blessed way to generate one short of a from-genesis
re-sync.

### What's here

- `reth-state-extractor/` — Rust tool to read a reth v2.2.0 plain-state DB and emit
  gethdbdump files. Includes `src/bin/diag.rs`, which **proved reth stores no preimages**
  (the `PlainAccountState`/`PlainStorageState` row counts). Reading half is shelved;
  the gethdbdump emitter format is documented in the header comment of
  `reth-state-extractor/src/main.rs`.
- `migrate-hoodi.log`, `gethdump/hoodi/export.log`, `*build*.log` — raw evidence.

### Components (built locally, not vendored — clone these yourself)

- **ethrex** (binary-trie node + `migrate`): `lambdaclass/ethrex` branch `eip-7864-plan`
  (built at commit `b0fe293b75723b349e6563a966efeda41a24e425`).
- **patched geth** (adds `db export code`): `edg-l/go-ethereum` branch `feat/export-code`
  (geth v1.17.2 base).

Large data artifacts (snapshots, exports, migrated DB) are git-ignored — regenerate via
the steps above.
