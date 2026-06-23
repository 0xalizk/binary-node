# ethpandaops snapshot findings — the preimage constraint was WRONG (2026-06-17)

Iteration 2 (`../iteration_2/04_preimage_path_findings.md`) concluded: *"There is NO
fast, Geth-free, no-re-execution path to a complete mainnet binary state you can take
unilaterally today."* **That conclusion is now overturned.** User pointed at
https://ethpandaops.io/data/ and there are fresh, public, full mainnet client
snapshots — and reth/erigon snapshots ARE a complete preimage source by construction.

## What's published (verified live 2026-06-17)

Snapshot block **25,330,000** (`0x1828150`), block timestamp **Tue Jun 16 2026
12:12:11 UTC** — i.e. ~1 day old, essentially at tip. All five EL clients are
snapshotted at the **same block**, served as `snapshot.tar.zst`.

URL scheme (from the snapshotter README, github.com/ethpandaops/snapshotter):
- Latest block number: `https://snapshots.ethpandaops.io/<network>/<client>/latest`  (plain text)
- Tarball:  `…/<network>/<client>/<block>/snapshot.tar.zst`
- Block info: `…/<block>/_snapshot_eth_getBlockByNumber.json`
- Client/version: `…/<block>/_snapshot_web3_clientVersion.json`
- Metadata (docker image + run args): `…/<block>/_snapshot_metadata.json`

(Note: `…/latest` returns the block number as plain text via the live origin; a bare
`GET` with `-L` may hit a soft-404 HTML page from cache — read the number, don't
follow redirects.)

| Client | docker image | run args (from `_snapshot_metadata.json`) | compressed size | preimage value |
|---|---|---|---|---|
| **reth**  | `ghcr.io/paradigmxyz/reth:v2.2.0` | `--full … --prune.bodies.pre-merge --prune.receipts.before=15537394 --engine.persistence-threshold=0` | **704.3 GiB** | ✅ **plain state, raw address/slot keyed — complete** |
| **erigon** | `erigontech/erigon:v3.4.0` | `--externalcl --prune.mode=full` | 968.0 GiB | ✅ temporal domains, raw address/slot keyed — complete |
| geth | `ethereum/client-go:v1.17.2` | `--state.scheme=path --cache.preimages --history.chain=postmerge` | 1018.0 GiB | ⚠️ has a preimage table but **likely PARTIAL** (see below) |
| besu | `hyperledger/besu:26.4.0` | `--sync-mode=SNAP --data-storage-format=BONSAI` | 1007.1 GiB | ❌ Bonsai/hashed — not a preimage source |
| nethermind | `nethermind/nethermind:1.37.2` | (RPC modules only) | 1061.8 GiB | ❌ hashed — not a preimage source |

Disk on this box: **6.9 TB free of 7.7 TB** (ethrex 465 G, lighthouse 15 G). Any of
these fits with room for the extracted DB + the binary-node datadir.

## Why reth/erigon are real preimage sources but geth probably isn't

The EIP-7864 binary tree is keyed by a derivation of the **raw** address and **raw**
storage slot — NOT `keccak(addr)`/`keccak(slot)` like the MPT. So bulk-building it
requires knowing every account's raw address and every slot's raw key (the
"preimages").

- **reth** stores state as *plain state* — `PlainAccountState (Address → Account)` and
  `PlainStorageState (Address → (StorageKey, value))`, keyed by the **raw** 20-byte
  address / 32-byte slot. The hashed trie tables (`HashedAccounts`/`HashedStorages`)
  are derived. The plain tables ARE the complete preimage set, **independent of how
  the node synced.**
- **erigon v3** uses temporal domains (`AccountsDomain` keyed by raw address,
  `StorageDomain` keyed by raw address+slot). Same story — complete by construction.
- **geth** stores the hashed trie and writes a preimage only when a key passes through
  its hasher during **local execution**. ethpandaops nodes are snap-synced; snap
  delivers state as already-hashed key/value pairs, so the addresses behind the bulk
  of accounts were never seen locally. ⇒ geth's `--cache.preimages` table is almost
  certainly **only the recently-touched subset**, NOT all ~250M accounts. Do not rely
  on it without first proving completeness (count preimages vs account count). We don't
  need to — reth gives completeness for free and is the smallest download.

## Consequence for the project

- **Old "no preimage dataset" constraint is dead.** A complete, ~1-day-fresh mainnet
  preimage source (reth, 704 GiB) is downloadable today, no permission needed.
- **Option B (ask the EF/lambdaclass team) is unnecessary** — the artifact is public.
  No outreach required.
- **Option A (genesis re-execution, days–weeks) demotes to fallback** — only if the
  snapshot extraction path fails.
- New recommended path is **Option D** (see `02_options_reconsidered.md` in this folder):
  download reth snapshot → extract plain state → feed ethrex `migrate` → build binary
  tree at block 25,330,000 → binary-node follows tip from there via the feeder.

## New work this introduces (vs. the iteration_2 plan)

One new, well-bounded component: a **plain-state extractor** that opens reth's (MDBX)
`PlainAccountState`/`PlainStorageState` tables and emits `(address → account)` +
`(address, slot → value)` in whatever form ethrex's `migrate` consumes. Options:
read the MDBX directly with a small Rust tool (reth-db crate or raw libmdbx), or use
`reth db` tooling. Erigon is the fallback source with an analogous extractor. Code
in the binary-node repo, not in ethrex.

## Open verification items before committing to D

1. Confirm reth v2.2.0 plain-state table names/layout on disk (reth-db schema) so the
   extractor targets the right tables.
2. Confirm what input shape ethrex `eip-7864-plan`'s `migrate` expects, and whether it
   wants a one-shot bulk import at a fixed block (25,330,000) — then have the feeder
   take over from block 25,330,001.
3. Sanity-check the extracted account count against a known mainnet figure (~250M+
   accounts) to prove the plain-state set is complete.
