# ⛔ CRITICAL FINDING — reth v2.2.0 is NOT a preimage source (2026-06-17)

Discovered during the Hoodi smoke test (the whole point of testing small first).
**Overturns the reth-extractor plan in `05_extractor_design.md` / `04_reth_schema_findings.md`.**

## What happened
Built the reth extractor (D2), extracted the Hoodi reth snapshot (block 3,030,000,
reth/v2.2.0-88505c7), ran the extractor → it emitted `code.rlp` but **empty**
`preimages.rlp` / `snapshot.rlp`. Diagnostic (`reth-state-extractor/src/bin/diag.rs`)
on the live DB:

```
TABLE                        ENTRIES
PlainAccountState            0            <-- EMPTY
PlainStorageState            0            <-- EMPTY
Bytecodes                    4,807,591
HashedAccounts               44,431,577   <-- state lives here
HashedStorages               273,203,407  <-- and here
AccountsHistory/…/Headers    0            (those live in static_files)
```

## Why this kills the reth path
- reth v2.2.0 stores current state **hashed**: `HashedAccounts` keyed by
  `keccak(address)`, `HashedStorages` keyed by `keccak(address)+keccak(slot)`. The
  **Plain (raw-key) tables are empty.**
- reth never needs to go hash→address: for execution it hashes the address and looks
  up `HashedAccounts`. So it **does not store preimages at all** — there is no
  `keccak(addr)→addr` mapping anywhere in the DB.
- The EIP-7864 binary tree needs the **raw** address/slot to derive BLAKE3 tree keys.
  reth cannot provide them. ⇒ **reth (any version ≥ the v2 hashed-state layout) is not
  a usable source for this migration.** Mainnet reth = identical format ⇒ the 704 GiB
  mainnet reth download would have hit the same wall.
- This contradicts `04_reth_schema_findings.md`, which read the Plain* table *definitions*
  in source (they compile) but did not establish they're *populated at runtime*. They
  are not, in the v2 storage layout. Lesson: verify table population on a real DB, not
  just the schema.

## The pivot: use the GETH snapshot (the ethrex-documented path)
- ethrex `migrate`'s native, documented input IS a **Geth snapshot with
  `--cache.preimages`** (`docs/binary-trie/migration-guide.md`). Geth *does* keep a
  preimage table (`keccak(x)→x`) — exactly what reth lacks.
- ethpandaops publishes mainnet **and** hoodi geth snapshots run with `--cache.preimages`
  (verified: mainnet/geth args include `--state.scheme=path, --cache.preimages`; hoodi
  too). Block 25,330,000 (mainnet) / 3,030,000 (hoodi).
- With geth as the source, **no custom extractor is needed** — use geth's own
  `geth db export {preimage,snapshot,code}`. (code export needs the patched fork
  `edg-l/go-ethereum feat/export-code`; preimage+snapshot are upstream geth v1.17+.)

## The ONE open risk to settle on Hoodi (now the smoke test's real purpose)
**Is geth's `--cache.preimages` set COMPLETE on a snap-synced ethpandaops node?**
- Geth records a preimage only when a key passes its hasher during local execution;
  snap sync delivers hashed state, so a snap-synced node *might* lack preimages for
  untouched accounts.
- BUT the ethrex team documents this exact ethpandaops-geth→migrate path and cites
  Hoodi numbers (~138M preimages), and Hoodi has 44.4M accounts + 273M slots — distinct
  slot keys + addresses plausibly ≈ 138M, i.e. complete. So it likely works; the ethrex
  devs presumably tested it.
- **Decisive check (cheap, on Hoodi geth):** after `geth db export preimage`, count
  address-preimages (value len 20) and compare to 44,431,577 accounts; count
  slot-preimages (value len 32) vs distinct slot keys. If migrate runs without
  "missing preimage" skips and the resulting tree has ~44.4M accounts, geth is complete
  ⇒ same approach works on mainnet geth (25,330,000).

## Status of the reth extractor work
- The `reth-state-extractor` binary + `06_gethdump_format.md` (the byte-exact gethdbdump
  emitter spec) are still VALID and reusable — that format knowledge is exactly what
  geth's native export produces and what migrate consumes. The reth *reading* half is
  shelved (reth isn't a source). Keep `diag.rs` — it's how we proved this.

## Revised Option D
1. Hoodi: download geth snapshot (84.5 GiB, downloading now) → build geth (+ patched
   fork for code) → `geth db export {preimage,snapshot,code}` → verify preimage
   completeness → `ethrex migrate … --at-block 3030000` → confirm forward sync. GATE.
2. Mainnet: download geth snapshot (1018 GiB) at block 25,330,000 → same export →
   migrate → feeder + equiv-daemon + dashboard.
