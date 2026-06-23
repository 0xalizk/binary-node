# `eip-7864-plan` branch — findings & implications

Source: deep-dive of lambdaclass/ethrex PR #6380 (branch `eip-7864-plan`, head
`b0fe293b`, **closed, never merged**). All file refs are `?ref=eip-7864-plan`.

## TL;DR — what this means for us

1. **mod #2 is real and runnable, not from scratch.** The branch has a working
   binary trie (BLAKE3, sparse stem nodes), builds state by executing blocks, and
   was validated by importing **10k Hoodi blocks from genesis** with all
   non-state-root validations passing. We fork this.
2. **#3 is resolved → genesis full re-execution (path a).** The branch's *fast*
   alternative (one-shot `ethrex migrate` from a snapshot) **requires a Geth node**
   (Geth snapshot + Geth `--cache.preimages` DB + a *patched* Geth fork for code
   export). We only run **ethrex**, so the migrate path is not usable without
   standing up a whole Geth mainnet node + custom fork on this box. The clean path
   for us is the one the user instinctively wanted: **build the binary trie by
   re-executing blocks from genesis** — preimages come for free from execution.
3. **The RPCs we need for equiv-checking work**; `eth_getProof` does not (and we
   don't need it).
4. **New hard constraint:** binary-node is **not an archive node** — it only answers
   state queries for the **last ~128 blocks**. This (plus the fact that our
   mainnet-node is also non-archive) shapes *when* equiv-checks can run. See
   `02_q3_recommendation.md`.

## 1. How state is built (two independent paths)

- **(a) Forward execution** — genesis allocs seed the trie
  (`setup_genesis_binary_trie`), then every block's `AccountUpdate`s are applied at
  the merkleization stage (`apply_account_updates_batch` → `handle_merkleization`).
  This is how the 10k Hoodi blocks were built, **from genesis (block 0..10000)**,
  fed via the `ethrex import` subcommand. Preimages are trivial here:
  `AccountUpdate`s carry the real `Address` + storage `H256` straight from LEVM.
- **(b) One-shot migration** — `ethrex migrate` (`cmd/ethrex/migrate.rs`) bulk-builds
  the trie from **Geth** snapshot+preimage+code exports (collect → build phases via
  `BulkTrieBuilder`), then `--at-block N` records the resume point
  (`set_latest_block_number(N)`; pure bookkeeping, no execution). **Requires Geth**
  with `--cache.preimages` and the patched `edg-l/go-ethereum feat/export-code`
  fork. Keccak is one-way, so snapshot keys can't be reversed without that preimage
  DB — which is exactly why there is no generic in-place MPT→binary conversion.

→ **We use path (a).** Path (b) is off the table unless we deploy Geth (we won't).

## 2. `--at-block`

On the `migrate` subcommand only. Help: *"Block number the snapshot state
corresponds to (state is post-execution of this block)"*, `required=true`. It only
records where to resume forward-sync after a migration. **Irrelevant to us** since we
aren't using the migrate path.

## 3. Sync mode

- `--syncmode full` is the **default and only working mode** on this branch
  ("binary trie has no snap sync"). Snap sync entry points are stubbed to return
  `Err("snap sync not supported on binary trie branch")`; MPT types replaced with
  `mpt_stubs.rs`.
- Full-sync p2p machinery was exercised on **mainnet** (commits "Deployed to
  mainnet-7… sync completes all phases without stalling"), but **state correctness
  was only validated on Hoodi (10k blocks)** — there is **no** validated full
  mainnet state build yet. This is execution risk we carry (see plan §Risks).

## 4. RPC surface (critical for equiv-daemon)

- ✅ `eth_getBalance`, `eth_getTransactionCount`, `eth_getCode`, `eth_getStorageAt`
  return **correct values** against the binary-trie state (served via FKV; recent
  ~128 blocks cached in `BinaryTrieLayerCache`).
- ❌ `eth_getProof` returns correct values but **empty proof arrays** and
  `storage_hash = 0` ("binary trie proofs … not yet wired into this RPC handler").
  The proof machinery exists (`binary_trie/proof.rs`) but isn't connected. **We
  don't need it** — user confirmed proofs are meaningless across different tries.
- ❌ **Historical state beyond ~128 blocks → `None`.** No archive. This is the
  constraint that drives the equiv-check design.

## 5. Other limitations / tech debt on the branch

- Not archive; FKV stores latest state only, no history/undo log.
- **Reorg support limited** — shallow reorgs OK, deep reorgs expensive; a proper FKV
  undo log is *planned, not implemented* (`docs/binary-trie/fkv-undo-log-plan.md`).
  Mainnet reorgs are usually 1–2 blocks, so likely fine, but flag it.
- state-root validation permanently skipped (by design); `gas_used`,
  `receipts_root`, `requests_hash`, `block_access_list_hash` **still validated** —
  this is our free per-block equivalence signal during catch-up.
- Real state bugs were found & fixed during Hoodi validation (InternalNode collapse,
  U256 storage-key overflow, storage_root sentinel gas mismatch, BLOCKHASH cache
  miss). i.e. the implementation is young; mainnet will likely surface more — which
  is exactly the point of equiv-daemon.

## 6. Unknowns flagged by the agent

- Migrate (snapshot) path never validated on mainnet (Hoodi only: ~33M accounts,
  ~268M slots, ~30 min @ 32 GB RAM). N/A for us anyway.
- Full sync never produced a *validated* mainnet state (Hoodi-only correctness).
- `eth_getProof` behavior if proofs were wired in — untested live.
