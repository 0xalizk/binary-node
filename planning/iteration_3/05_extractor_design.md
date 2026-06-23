# Bootstrap extractor — design decision (2026-06-17)

Synthesis of `04_reth_schema_findings.md` (reth v2.2.0 on-disk state) and
`03_ethrex_migrate_findings.md` (what ethrex `eip-7864-plan`'s `migrate` consumes). Both
verified against source at the pinned commits. This doc records the chosen extractor
architecture for Option D.

## The two endpoints (both confirmed)

**Source — reth v2.2.0 snapshot (704 GiB):**
- `PlainAccountState`: raw `Address` → `Account{nonce:u64, balance:U256, bytecode_hash:Option<B256>}` (Compact codec).
- `PlainStorageState` (DUPSORT): raw `Address` → `StorageEntry{slot:B256, value:U256}`; **zero slots absent**.
- `Bytecodes`: `bytecode_hash (B256)` → bytecode.
- **State is complete**: `--full` prunes only history/changesets/indices/bodies/receipts — there is no plain-state prune segment. State is MDBX-only (`<datadir>/db/mdbx.dat`); static files + RocksDB hold only history. So the snapshot's plain state at block 25,330,000 is the full current state. ✅

**Sink — ethrex `migrate` (on our fork of `eip-7864-plan`):**
- `ethrex --network mainnet migrate <preimages.rlp> <snapshot.rlp> --code <code.rlp> --at-block 25330000`.
- Consumes **three Geth `gethdbdump`-format binary streams**, keccak-keyed, with a preimage file recovering raw addr/slot. Streams + batches to disk (constant-RAM build). Derives EIP-7864 BLAKE3 tree keys itself.
- `--at-block` lets it bootstrap at a recent block and sync forward. ✅
- **Decode of the slim-RLP account skips the storage root** (`migrate.rs:104`) and uses only `(nonce, balance, code_hash)` → reth's lack of a stored storage root does not matter. ✅
- Balance is packed into a 128-bit field (`pack_basic_data` asserts `balance <= u128::MAX`). Mainnet per-account balances are far below 2^128 wei, so fine. ✅

## The gap & the decision

`migrate` wants **keccak-keyed Geth dumps**; reth stores **raw-keyed plain state**. Two ways to bridge:

- **D1 — native reth adapter inside our ethrex fork.** Add a `migrate-from-reth` source that reads reth's MDBX directly and feeds the existing collect→build pipeline with raw `(address, slot, value, code)`, skipping keccak/preimage indirection. Cleanest data flow, no intermediate files. **Risk:** pulls the `reth-db` crate tree into ethrex's workspace — two large Rust projects (overlapping alloy/revm-adjacent deps) that may not co-resolve. Dependency-hell risk is real.

- **D2 — standalone extractor → gethdump → unmodified `migrate` (CHOSEN).** A small separate Rust binary links `reth-db` only, iterates the three tables, and emits the three gethdbdump streams:
  - preimage file: for each address `keccak(addr)→addr` (20-byte value) and each slot `keccak(slot)→slot` (32-byte value).
  - snapshot file: `'a'+keccak(addr)` → slim-RLP `[nonce, balance, codehash]` (root omitted — migrate skips it); `'o'+keccak(addr)+keccak(slot)` → RLP(value).
  - code file: `'c'+bytecode_hash` → bytecode (the `Bytecodes` key **is** `keccak(code)`, no hashing needed).
  Then ethrex's `migrate` runs **unmodified** (the path the ethrex devs actually test).

**Why D2:** isolates the two dependency universes (no reth crates in ethrex's build), uses ethrex's import path as-is (lowest integration risk), and removes the patched-Geth dependency entirely (we synthesize the dumps; we never run geth). Cost: the extractor re-keccaks keys that migrate then looks back up, plus intermediate dump files on disk (~plan 3× headroom per the migration guide; we have 6.9 TB free). Acceptable.

D1 stays as the fallback if intermediate-file disk or the keccak round-trip ever becomes the bottleneck.

## Extractor crate pinning (from 04_reth_schema_findings.md)
- Path/DB crates from the v2.2.0 git tag: `reth-db`, `reth-db-api`, `reth-libmdbx` (+ `reth-provider` only if needed — prefer `reth_db::open_db_read_only` + raw cursors; **avoid `ProviderFactory`** at v2.2.0).
- Type/codec crates from crates.io: `reth-primitives-traits = "=0.3.1"`, `reth-codecs = "=0.3.1"` (so `Account`/`StorageEntry`/`Bytecode` decode natively — do NOT reimplement the Compact/hand codecs).
- Open read-only, `cursor_read::<PlainAccountState>()` + `cursor_dup_read::<PlainStorageState>()`, `Bytecodes` for code.

## Why NOT the geth snapshot directly
The geth snapshot (1018 GiB, `--cache.preimages`) is migrate's *native* format and would need zero extractor — BUT a snap-synced geth only records preimages for locally-executed keys, so its preimage table is almost certainly **incomplete** → missing accounts → unusable. We'd have to download 1 TB just to test that hypothesis (which I expect to fail). Reth's plain state is complete by construction, so we skip the gamble.

## ⚠️ Biggest open risk (validate EARLY, before the 704 GiB mainnet download)
`03_ethrex_migrate_findings.md` §Caveats: forward-sync re-execution from a mid-chain
`--at-block` is only **claimed** in the docs, not proven end-to-end. If ethrex can't
actually resume execution at block 25.33M from migrated state, the entire snapshot-
bootstrap premise collapses. **Smoke test first:** migrate a small network (Hoodi has
a reth snapshot too, ~3M blocks / 33M accounts) end-to-end — extractor → migrate →
confirm the node syncs forward a few hundred blocks and RPCs return correct values —
*before* committing to the mainnet download + build. Cheap insurance.

## Net plan for Option D
1. **Smoke test on Hoodi** (small reth snapshot) — prove extractor → migrate → forward-sync works end-to-end. Gate.
2. Download mainnet reth snapshot (`…/mainnet/reth/latest`, ~704 GiB, background/screen).
3. Run extractor → three gethdump files.
4. `ethrex migrate … --at-block <snapshot block>` → binary tree built.
5. Feeder drives forward sync from snapshot block + 1; equiv-daemon + dashboard.
