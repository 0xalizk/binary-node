## geth + EIP-7864 (binary state tree): does it offer a faster route?

Research date: 2026-06-22. Sources: live `gh api`/`gh search` against `ethereum/go-ethereum`
and `gballet/go-ethereum`, plus the EIP text and web. Goal context: we're building an
EIP-7864 binary-trie mainnet shadow node; our current route is a fork of ethrex's
`eip-7864-plan` branch (BLAKE3-keyed) bootstrapped via geth-snapshot + preimage `migrate`,
and it's RAM-bound and slow. Question: can geth itself be the binary-trie node, faster/simpler?

### TL;DR / bottom line

- **Yes, geth has a real, actively-maintained EIP-7864 binary-tree implementation** — and
  crucially, the *core* of it is **merged into upstream `ethereum/go-ethereum` master**
  (package `trie/bintrie`), not just a personal fork. It is being worked on continuously
  (commits Sep 2025 → late May 2026).
- **The MPT→binary-tree offline conversion tool we want exists** as a `geth bintrie convert`
  subcommand — but it lives on a **gballet fork branch (`bintrie-offline-conversion`), NOT yet
  upstream, NOT yet an open upstream PR**. It is recent (Mar 2026) and looks usable.
- **BUT — hard compatibility blocker for an equivalence comparison with ethrex:** geth's
  binary tree hashes with **SHA256**, while our ethrex reference uses **BLAKE3**. EIP-7864's
  hash function is explicitly **not finalized** (BLAKE3 / Keccak / Poseidon2 all candidates).
  geth deliberately switched blake3→sha256 "for compatibility" (with the EF test fixtures).
  **The two will NOT produce matching binary-tree state roots.** They are on different,
  unfinalized hash revisions of the same EIP. Any root-level equivalence check between
  geth-bintrie and ethrex-`eip-7864-plan` is meaningless until both pin the same hash.
- **Mainnet feasibility:** the conversion tool is offline (read synced MPT → write binary
  trie), so the *route* is sound for mainnet — but it has **the same preimage dependency**
  as our ethrex route (`--cache.preimages` required; it errors out without account/slot
  preimages). It is **not a re-execution-from-genesis or live-sync-in-binary-form path**;
  there is no consensus/devnet wired up for binary-tree mainnet. So geth would give us a
  binary-tree *snapshot/dump*, not a live binary-trie following the chain head.
- **Recommendation:** worth a parallel experiment because geth's conversion is offline and
  memory-bounded (explicit `--memory-limit`, periodic commit+GC cycles) — potentially less
  RAM-bound than the ethrex path. But treat it as a **SHA256-keyed** artifact. If our project
  requires BLAKE3 (to match ethrex), geth is **not** a drop-in equivalence target; it would
  need a one-line hash swap in `trie/bintrie/hasher.go` + key derivation, and a rebuild.

---

### 1. Does geth have an EIP-7864 (binary tree) implementation? Where, and how mature?

**Yes — merged into upstream master**, package [`trie/bintrie`](https://github.com/ethereum/go-ethereum/tree/master/trie/bintrie).

- **Seminal PR (MERGED): [#32365](https://github.com/ethereum/go-ethereum/pull/32365)** —
  "trie/bintrie: add eip7864 binary trees and run its tests", merged **2025-09-01** into
  `ethereum:master` (merged by rjl493456442 / Gary Rong). The package source explicitly cites
  the spec: `trie/bintrie/trie.go` contains `// according to EIP-7864 specification.` and
  `// Reference: https://eips.ethereum.org/EIPS/eip-7864`.
  - PR's own framing of maturity: *"This is only running the tests and will not be executed in
    production"* and *"will gradually replace verkle trees in the codebase."* It also notes
    *"Switch from blake3 to sha256 hashing for compatibility."*
- **Maturity: experimental but well past PoC, and steadily hardening.** It started as
  test-only node types and has since grown real machinery. Upstream `trie/bintrie` now
  contains: `binary_node.go`, `stem_node.go`, `internal_node.go`, `hashed_node.go`,
  `hasher.go`, `key_encoding.go`, `iterator.go`, `node_store.go`, `node_ref.go`,
  `store_commit.go`, `store_ops.go`, `recorder.go`, `trie.go` + full `_test.go` coverage.
- **Active, recent commit history on upstream master** (`gh api .../commits?path=trie/bintrie`),
  most recent first — this is NOT abandoned:
  - `#34843` 2026-05-28 record inserted leaves for t8n
  - `#34794` 2026-05-01 group 2^N binary trie nodes in serialization
  - `#34777` 2026-04-21 print todot path in binary
  - `#34055` 2026-04-20 replace BinaryNode interface with GC-free NodeRef arena
  - `#34754` 2026-04-18 skip clean nodes in CollectNodes (reduce commit write amplification)
  - `#34700` 2026-04-17 **split CachingDB into merkle + binary dbs**
  - `#34670` 2026-04-13 **spec change, big-endian hashing of slot key**
  - `#34690` 2026-04-10 fix GetAccount/GetStorage non-membership
  - `#34676` 2026-04-10 fix DeleteAccount no-op
  - `#34032`/`#33989`/`#33961` Mar 2026 hashing perf (parallelize, sync.Pool, clean-node cache)
  - `#33951` 2026-03-05 fix overflow in slot key computation
  - `#33900` 2026-02-27 fix endianness in code chunk key computation
  - `#33694` 2026-01-28 fix tree key hashing to match spec
  - `#33461` 2025-12-30 **remove all references to go-verkle and go-ipa** (verkle decommissioned)
- **Open upstream PRs still iterating on it** (as of research date): `#34772` (bitarray path
  encoding), `#34706` (**bintrie flat state support** in `core/state` + `triedb/pathdb`),
  `#34689` (children array instead of left/right). Flat-state support (#34706) is the kind of
  thing needed for it to back live state efficiently.

So: **real, merged, current** — but framed by its authors as not-yet-production and still
churning on spec details (endianness/hashing fixes were landing as recently as April 2026).

### 2. What can it actually do today?

(a) **Sync/build mainnet state in binary-tree form (live):** **No.** There is no consensus
integration, no fork activation, no live "sync mainnet into a binary tree" mode in upstream.
The binary tree is plumbing + an offline converter, not a synced state backend following head.

(b) **MPT→binary-tree conversion (the route we want): YES, on a fork branch.**
The branch **[`gballet/go-ethereum:bintrie-offline-conversion`](https://github.com/gballet/go-ethereum/tree/bintrie-offline-conversion)**
adds `cmd/geth/bintrie_convert.go` (+ `_test.go`):
  - Commit `cc763d090` 2026-03-12 "cmd/geth: add subcommand for offline binary tree conversion";
    latest on branch `313459db3` 2026-03-19 "review feedback".
  - New CLI: **`geth bintrie convert [--delete-source] [--memory-limit MB] [state-root]`**.
    Help text: *"Reads all state from the Merkle Patricia Trie and writes it into a Binary Trie,
    operating offline. Memory-safe via periodic commit-and-reload cycles. The optional
    state-root argument specifies which state root to convert. If omitted, the head block's
    state root is used."*
  - How it works (read from source): opens the source MPT (`trie.NewStateTrie`), iterates
    accounts and (per-account) storage tries, and writes each into a `bintrie.BinaryTrie`
    backed by a **pathdb** triedb at `triedb-bintrie` (note it reuses the verkle plumbing flag:
    `triedb.Config{IsVerkle: true, PathDB: {...}}`). For each account it also pulls code via
    `rawdb.ReadCode` and calls `binTrie.UpdateContractCode`.
  - **Memory behavior (relevant to our RAM problem):** explicit `--memory-limit` (default
    **16384 MB**); `maybeCommit` checks `runtime.ReadMemStats` every 5s and, once `Alloc`
    exceeds the limit, commits the binary trie to disk, then `runtime.GC()` +
    `debug.FreeOSMemory()`. Also force-commits every 1000 accounts / 10000 slots. So it is
    *designed* to be memory-bounded rather than load-everything-in-RAM.
  - **Preimage dependency (same gotcha as our ethrex route):** it converts by hashed-key
    iteration and resolves addresses/slot keys via `srcTrie.GetKey(...)` preimages. If a
    preimage is missing it aborts with: *"missing preimage for account hash %x (run with
    --cache.preimages)"* (and likewise for storage keys). So the geth MPT it reads **must have
    been synced/built with preimages**, exactly like the geth-snapshot+preimage approach we
    already use to feed ethrex.
  - `--delete-source` deletes MPT nodes after a successful conversion (in-place migration flavor).

(c) **Fresh devnet/genesis binary-tree chains (not mainnet):** **partially, on a fork.**
Branch **[`gballet/go-ethereum:binary-tree-in-testnet-genesis`](https://github.com/gballet/go-ethereum/tree/binary-tree-in-testnet-genesis)**
(latest 2025-08-26 "use binary tries instead of verkle"; "Implement EIP-7864 binary trie with
SHA256 hash") wires a binary tree into testnet genesis by repurposing the verkle-at-genesis
path. This is devnet/genesis scaffolding, not mainnet. I could **not** confirm a public,
named EF "binary-tree devnet" currently running mainnet-scale data (web search surfaced only
EIP discussion + general roadmap chatter; see gaps).

### 3. Build / run instructions (if you try the conversion route)

Prereq: a geth chaindata that was synced **with preimages enabled** (`--cache.preimages`), at
the state root you want to convert (head, or a specific root).

```
git clone https://github.com/gballet/go-ethereum
cd go-ethereum
git checkout bintrie-offline-conversion
make geth        # produces ./build/bin/geth

# offline convert the synced MPT to a binary trie (writes to <datadir>/.../triedb-bintrie)
./build/bin/geth bintrie convert \
    --datadir <your-geth-datadir> \
    --memory-limit 16384 \
    [<state-root-hex>]          # omit to use head block's root
# add --delete-source to drop the MPT after a successful conversion
```

Notes:
- The destination triedb is a pathdb journaled under `triedb-bintrie` in the datadir.
- It logs progress (`accounts`, `slots`, `codes`, `commits`, `accounts/sec`).
- Upstream master alone (`ethereum/go-ethereum`) does **not** have this subcommand — you must
  use the gballet branch. The branch is ~Mar 2026; expect it to be rebased on a fairly recent
  master (it sits atop bintrie commits up to `#33989`/Mar 2026 era).
- For a fresh binary-tree devnet instead, use `binary-tree-in-testnet-genesis` and the
  verkle-at-genesis style genesis config — but that's a clean chain, not mainnet state.

### 4. Mainnet feasibility

- **Via overlay/live conversion (verkle-transition style):** **Not available.** geth's binary
  work has no live overlay/transition-at-fork mechanism wired for mainnet. (The verkle
  predecessor *did* have an overlay-transition design — see §5 — but that machinery is being
  rebuilt for binary, not yet shipped as a live mainnet path.)
- **Via offline conversion from a synced MPT:** **Feasible in principle, today, with the fork
  branch.** Sync geth to mainnet head with `--cache.preimages`, then run `geth bintrie
  convert`. This yields a mainnet-state binary trie **as a static artifact at one root** — it
  does not then follow the chain head. Cost/practicality is similar in shape to our ethrex
  route (you still need a full synced + preimaged MPT first), but the converter itself is
  explicitly memory-bounded (`--memory-limit`, periodic commit + `FreeOSMemory`), so it may be
  **less RAM-bound than the ethrex migrate** — that's the main potential win to test.
- **Via re-execution from genesis:** not provided / not practical.
- **Snapshot import directly to binary:** no direct path; the converter consumes the *MPT*
  (and its preimages), not a flat snapshot, though it reads code from rawdb.

### 5. EIP-7864 ↔ verkle relationship in geth

**The binary-tree work is built directly on top of the verkle plumbing**, then is replacing it:
- The converter sets `triedb.Config{IsVerkle: true}` for the binary destination DB, and uses
  constants literally named `verkleNodeWidthLog2`, `VerkleNodeWidth`, `mainStorageOffsetLsh...`
  in `trie/bintrie/key_encoding.go` — the key-derivation layout (BasicData packing, code/header
  storage offsets, node width 256) is inherited from the verkle/EIP-6800 design.
- gballet's fork has an enormous verkle history (hundreds of `verkle/*`, `kaustinen*`,
  `overlay-implementation`, `transition-in-cachingdb`, `verkle-conversion-tool`,
  `snapshot-to-bintrie` branches) — the binary effort reuses that conversion/overlay/witness
  scaffolding.
- Upstream then **removed go-verkle/go-ipa entirely** (`#33461`, 2025-12-30) — i.e. the curve
  cryptography is gone, but the *tree-shape, key-derivation, and conversion patterns* carried
  over to `bintrie`. So: verkle tooling is indeed "most of what exists," now re-skinned as
  binary + hash-based instead of commitment-based.

### 6. Compatibility with our ethrex reference (CRITICAL)

**geth binary tree = SHA256. ethrex `eip-7864-plan` = BLAKE3. They will not match.**
- Evidence in geth: `trie/bintrie/hasher.go` imports `crypto/sha256` and the whole node-hash +
  key-derivation path (`getBinaryTreeKey` in `key_encoding.go`) uses `newSha256()`. PR #32365
  states it switched "blake3 to sha256 hashing for compatibility" (with EF execution-spec
  test fixtures, which adopted SHA256).
- Evidence in the EIP: EIP-7864 (Draft, created 2025-01-20) explicitly says the hash is **not
  final** — *"The current implementation uses BLAKE3 to reduce friction for EL clients
  experimenting... the final decision remains TBD"*; candidates are **BLAKE3 / Keccak /
  Poseidon2**. So ethrex (BLAKE3) and geth (SHA256) each picked a different still-tentative
  option.
- Additional spec-version skew to watch: geth landed *endianness* fixes for the binary tree
  recently — `#34670` (2026-04-13) "big-endian hashing of slot key" and `#33900` (2026-02-27)
  "endianness in code chunk key computation". If ethrex's `eip-7864-plan` predates the same
  fixes, the key derivation diverges even beyond the hash choice.
- **Implication for us:** a geth-converted binary trie is only equivalence-comparable to
  ethrex if both are forced onto the *same* hash (BLAKE3) **and** the same key-derivation
  endianness revision. To use geth as the node while keeping ethrex as the reference, we'd have
  to patch geth's `trie/bintrie` to BLAKE3 + match ethrex's exact EIP revision, then rebuild —
  a small but load-bearing change, and it must be re-verified against test vectors.

---

### What I could NOT confirm (gaps / honesty)

- **No public upstream PR for the offline `bintrie convert` subcommand.** As of research date,
  `convert` exists only on `gballet/go-ethereum:bintrie-offline-conversion`; I found no open
  `ethereum/go-ethereum` PR titled for it (only the incremental infra PRs #34772/#34706/#34689
  are upstream). So the converter is fork-only and not code-reviewed into master — treat as
  experimental, possibly rebase-rotting.
- **No confirmed, named, currently-running EF binary-tree devnet** processing mainnet-scale
  data. Web search returned EIP discussion + roadmap mentions (CFI status, "Devnet-5", a
  vague "activation expected around June") but nothing I could verify as a concrete binary-tree
  mainnet/devnet I could point you at. Do not assume one exists in usable form.
- **I did not run the converter or measure its actual RAM/throughput on mainnet data** — the
  "less RAM-bound" claim is inferred from its code (explicit memory-limit + commit/GC cycles),
  not benchmarked.
- **Exact ethrex `eip-7864-plan` hash/endianness revision was not cross-checked here** (out of
  scope of the geth repos); the BLAKE3-vs-SHA256 mismatch is certain, but the precise
  endianness delta vs ethrex needs a direct diff on our side.

### Concrete next experiment (if pursued)

1. Take an existing geth mainnet chaindata synced **with `--cache.preimages`** (or sync one).
2. Build `gballet/go-ethereum@bintrie-offline-conversion`; run `geth bintrie convert
   --memory-limit <RAM budget>` against a fixed historical state root; record peak RSS +
   accounts/sec vs our ethrex `migrate`.
3. Before any equivalence comparison, **patch `trie/bintrie/hasher.go` + `key_encoding.go` to
   BLAKE3 and to ethrex's exact key-derivation/endianness revision**, rebuild, and validate
   against shared EIP-7864 test vectors. Only then are geth's and ethrex's binary roots
   comparable.

### Key links

- Upstream package: https://github.com/ethereum/go-ethereum/tree/master/trie/bintrie
- Seminal merged PR #32365: https://github.com/ethereum/go-ethereum/pull/32365
- bintrie commit history (master): https://github.com/ethereum/go-ethereum/commits/master/trie/bintrie
- Offline converter branch: https://github.com/gballet/go-ethereum/tree/bintrie-offline-conversion
  (file `cmd/geth/bintrie_convert.go`)
- Binary-tree genesis branch: https://github.com/gballet/go-ethereum/tree/binary-tree-in-testnet-genesis
- EIP-7864: https://eips.ethereum.org/EIPS/eip-7864
- EIP-7864 magicians thread: https://ethereum-magicians.org/t/eip-7864-ethereum-state-using-a-unified-binary-tree/22611
