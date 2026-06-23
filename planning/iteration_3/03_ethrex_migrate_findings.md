# ethrex `eip-7864-plan` — state import / migration findings

Authoritative source: `github.com/lambdaclass/ethrex`, branch **`eip-7864-plan`**
(tip commit `b0fe293b75723b349e6563a966efeda41a24e425`, "Add --at-block flag and update migration docs").
All file/line citations below are on that branch unless stated otherwise. Raw read via
`gh api repos/lambdaclass/ethrex/contents/<path>?ref=eip-7864-plan`.

---

## TL;DR

- **YES — a real `migrate` subcommand exists** and is fully wired. It is NOT aspirational.
  `ethrex --network <net> migrate <preimages.rlp> <snapshot.rlp> --code <code.rlp> --at-block <N>`.
- It does **NOT** consume a genesis `alloc` JSON for bulk load. It consumes **three Geth
  `gethdbdump`-format binary export files**: a **preimage** export, a **snapshot** export,
  and a **code** export. This is exactly the "preimage export + state snapshot" path your
  prior notes anticipated.
- Input is **raw `(address, slot, value)` semantics**, recovered via preimages — the importer
  does the EIP-7864 BLAKE3 key derivation itself. You provide keccak-keyed Geth dumps + the
  keccak→preimage map; you do NOT pre-derive tree keys.
- It **can bootstrap at an arbitrary recent block** via `--at-block` and then sync forward.
  That is the entire point of this branch's migration story.
- It **streams and batches to disk** (RocksDB temp CF + left-to-right bulk build with a tiny
  in-memory spine). Designed for full mainnet scale.
- Binary tree lives in crate **`ethrex-binary-trie`** (`crates/common/binary_trie/`),
  BLAKE3-based, backed by **RocksDB** (CF `binary_trie_nodes`).
- **MPT state-root validation is removed/disabled** — confirmed: `validate_state_root` is
  marked "Currently unused: binary trie skips MPT state root validation."

---

## 1. The `migrate` subcommand / state-import entrypoints

### Primary: `migrate` CLI subcommand (the bulk-load path you want)

- **CLI definition:** `cmd/ethrex/cli.rs:537-573` — `Subcommand::Migrate`:
  ```
  #[command(name = "migrate", about = "Build binary trie from geth snapshot + preimage exports")]
  Migrate {
      preimage_path: String,      // positional PREIMAGE_FILE  "geth db export preimage"
      snapshot_path: String,      // positional SNAPSHOT_FILE   "geth db export snapshot"
      code_path: Option<String>,  // --code CODE_FILE           "geth db export code"
      fast: bool,                 // --fast  force in-memory preimage lookups
      at_block: u64,              // --at-block  block the snapshot state corresponds to
  }
  ```
- **CLI dispatch:** `cmd/ethrex/cli.rs:684-702` — calls
  `crate::migrate::migrate_with_preimages(preimage_path, snapshot_path, code_path, datadir, genesis, fast, at_block)`.
  Note it still loads the network genesis (`network.get_genesis()`) to open the store, but
  genesis is **not** the bulk data source.
- **Implementation:** `cmd/ethrex/migrate.rs` — `migrate_with_preimages(...)` at
  `cmd/ethrex/migrate.rs:121` (doc comment), function body follows.
- **Datadir auto-migration note:** `cmd/ethrex/cli.rs:594-601` runs the ordinary
  network-subdir datadir migration before `Migrate` (unrelated to state import; it's the
  `--no-migrate` legacy-dir move).

### Bulk trie builder
- `cmd/ethrex/bulk_builder.rs` — `BulkTrieBuilder` (`struct` at line 31, `new` at 57,
  `insert_stem` at 219, `finish` at 293). Left-to-right streaming builder; keeps only the
  "right spine" in memory and flushes completed subtrees (`flush_threshold = 500_000`,
  line 62).

### Genesis path (the *other* way state enters the binary tree — NOT bulk import)
- `crates/blockchain/blockchain.rs:258-269` — `default_with_genesis_state` builds a
  `BinaryTrieState::new()` and applies the genesis allocation to it. This is the normal
  "start from real genesis" path (genesis JSON `alloc`), used for fresh small networks/tests,
  not for loading ~250M mainnet accounts.
- `crates/common/binary_trie/state.rs` — `BinaryTrieState` (`new` at line 80, `open` at 109);
  comment at 108 notes "caller is responsible for applying genesis."

### Things searched for that do NOT exist as separate bulk paths
- No EIP-7928/BAL-driven import path for state bootstrap.
- No RLP-block-import path that builds the binary tree from blocks for bulk state load
  (`import`/`import-bench` exist at `cli.rs:457`/`470` but those replay blocks, they are not
  a snapshot bulk loader).
- No "from_genesis"/"rebuild" subcommand beyond the genesis apply above.
- The only true bulk-import entrypoint is `migrate`.

---

## 2. Exact input format

Three files, all in **Geth `gethdbdump` format** (the binary stream produced by
`geth db export <kind>`). This is **NOT** JSON and **NOT** ethrex's own DB.

### gethdbdump wire format (parsed in `cmd/ethrex/migrate.rs`)
- Header: one RLP list header, skipped — `skip_gethdbdump_header` (`migrate.rs`, ~line 974).
- Then a stream of entries, each: `op` (RLP: `0x80`=add/0, else delete) + RLP byte-string `key`
  + RLP byte-string `value`. Reader: `read_gethdbdump_entry` (~line 1011),
  `read_rlp_bytes_into` (~line 1040). Only `op == 0` (add) entries are used.

### (a) Preimage file — `geth db export preimage`
- Entry: `key` ends in the 32-byte keccak hash; `value` is the preimage.
- `value.len() == 20` → an **address** preimage (`keccak(addr) → addr`).
- `value.len() == 32` → a **storage-slot key** preimage (`keccak(slot) → slot`).
- Parsed by `parse_geth_dump` (`migrate.rs`, doc ~line 825). Stored either as in-memory
  `FxHashMap` (fast mode) or as sorted mmap'd flat files `preimage_addrs.bin` /
  `preimage_slots.bin` with binary search (`enum Preimages`, `migrate.rs` ~line 600).

### (b) Snapshot file — `geth db export snapshot` (the actual state values)
Parsed in `collect_phase` (`migrate.rs`, doc ~line 263). Two key shapes:
- **Account**: `key.len() == 33 && key[0] == b'a'` → `key[1..33]` = `keccak(address)`,
  `value` = Geth **slim-RLP account** `[nonce, balance, root?, codehash?]` (decoded by
  `decode_slim_account`, `migrate.rs` ~line 600+; root is skipped, empty codehash defaults to
  the empty-code hash). → `RawEntry::Account`.
- **Storage**: `key.len() == 65 && key[0] == b'o'` → `key[1..33]` = `keccak(address)`,
  `key[33..65]` = `keccak(storage_slot)`, `value` = RLP-encoded slot value (zero values are
  dropped). → `RawEntry::Storage`.

### (c) Code file — `geth db export code` (optional, `--code`)
- Entry: `key.len() == 33 && key[0] == b'c'` → `key[1..33]` = `keccak(code)` (= code hash),
  `value` = raw bytecode. Parsed by `parse_code_dump` (`migrate.rs`, doc ~line 760) into
  `FxHashMap<[u8;32], Vec<u8>>`. Without it, `code_size = 0` and code chunks are skipped
  (accounts still load; balances/nonces fine, but contract code chunks/code_size absent).

> **IMPORTANT for producing your own input:** the importer expects the **Geth slim-RLP
> account** layout and the `a`/`o`/`c` key-prefix + keccak-hash conventions above, NOT raw
> addresses in the snapshot file. Addresses/slots are recovered by looking the keccak hashes up
> in the preimage file. So to feed ~250M accounts you must produce gethdbdump-shaped streams
> where snapshot keys are keccak hashes and a matching preimage file supplies
> `keccak(addr)→addr` and `keccak(slot)→slot`. The patched Geth fork
> `github.com/edg-l/go-ethereum/tree/feat/export-code` is what the docs assume produced these.
> (See `docs/binary-trie/migration-guide.md`.)

### Concrete struct/serde types
There is **no serde**. Everything is hand-rolled binary/RLP parsing in `migrate.rs`. The
in-flight types are plain enums:
- `enum RawEntry { Account { keccak_addr:[u8;32], slim_rlp:Vec<u8> }, Storage { keccak_addr:[u8;32], keccak_slot:[u8;32], raw_value:Vec<u8> } }`
- `enum ProcessedEntry { Account { basic_data_key, basic_data, code_hash_key, code_hash, code_chunks }, Storage { tree_key:[u8;32], value_bytes:[u8;32] } }`
- The slim account fields come out as `(nonce: u64, balance: U256, code_hash: [u8;32])`.

---

## 3. How the binary tree is keyed (EIP-7864 stem/subindex derivation)

You provide **raw `(address, slot, value)` semantics** (via keccak-keyed dumps + preimages);
the importer derives tree keys itself. Key-derivation lives in
**`crates/common/binary_trie/key_mapping.rs`**:

- `tree_hash(data)` (`key_mapping.rs:40`) = plain **BLAKE3** (no zero-input special case here).
- `get_tree_key(address, tree_index, sub_index)` (`key_mapping.rs:48`):
  `input = address32(32B) ++ tree_index_be(32B)`; `stem = BLAKE3(input)[..31]`; key = `stem ++ sub_index`.
- `get_stem_for_base(address)` (`key_mapping.rs:69`): stem at tree_index 0, reused for
  basic_data / code_hash / header storage / first 128 code chunks.
- `tree_key_from_stem(stem, sub_index)` (`key_mapping.rs:80`).
- Leaf sub-indices: `BASIC_DATA_LEAF_KEY = 0`, `CODE_HASH_LEAF_KEY = 1` (`key_mapping.rs:8,11`).
- Offsets: `HEADER_STORAGE_OFFSET = 64`, `CODE_OFFSET = 128`, `STEM_SUBTREE_WIDTH = 256`
  (`key_mapping.rs:14,17,20`).
- `get_tree_key_for_storage_slot(address, storage_key)` (`key_mapping.rs:113`): slots `0..63`
  → header area (sub 64..127); slots `≥64` → main storage at `2^248 + slot`.
- `get_tree_key_for_code_chunk(address, chunk_id)` (`key_mapping.rs:101`): chunks start at
  `CODE_OFFSET`.
- `pack_basic_data(version, code_size, nonce, balance)` (`key_mapping.rs:149`): 32-byte leaf —
  byte0 version, bytes5-7 code_size (3B), bytes8-15 nonce, bytes16-31 balance (low 128 bits).
  **Note balance is asserted `<= u128::MAX`** (EIP-7864 128-bit balance field).
- `chunkify_code(code)` (`key_mapping.rs:208`) → 31-byte code chunks.

In `migrate.rs collect_phase`, the per-entry derivation calls
`get_stem_for_base`, `tree_key_from_stem(stem, 0)` for basic_data, `tree_key_from_stem(stem,
CODE_HASH_LEAF_KEY)` for code hash, `get_tree_key_for_code_chunk` per chunk, and
`get_tree_key_for_storage_slot` for storage. Each produces a 32-byte key whose first 31 bytes
are the stem and last byte the sub-index — written to the temp CF, then sorted-merged by stem
in `build_phase`.

---

## 4. Arbitrary-block bootstrap (block 25,330,000?) — YES

- `--at-block <N>` is mandatory and records the snapshot's block:
  `migrate.rs` step 6 calls `store.set_latest_block_number(at_block)` with comment
  "Record the block number the migrated state corresponds to." The tip commit message:
  "Record the block number the snapshot corresponds to so the node knows where to resume
  syncing from."
- `docs/binary-trie/migration-guide.md` ("Snapshot freshness" + Step 6): the node resumes /
  syncs forward from `at_block` after migration. Explicitly a "genesis-like starting state at a
  recent block" workflow — it does **not** require building from real genesis.
- Caveat: genesis is still loaded to `init_store` (for chain config), but the **state** comes
  from the snapshot. So yes, you can bootstrap at ~25.33M and execute forward. You must pass the
  exact block the snapshot's state is post-execution of.

---

## 5. Scale / memory behavior

Designed for full mainnet-sized state; it streams and batches to disk:
- **Two-phase** design (`migrate.rs`): Phase 1 *collect* streams the snapshot with an 8 MiB
  `BufReader`, processes batches of `BATCH_SIZE = 200_000` raw entries in parallel (rayon —
  preimage lookups, RLP decode, BLAKE3), and `write_batch`es `(tree_key, value)` pairs into the
  RocksDB **`migration_temp`** CF. No trie built yet.
- Phase 2 *build* (`build_phase`) iterates the temp CF in **sorted key order** and feeds the
  `BulkTrieBuilder` left-to-right; only the right spine (~25 KB per docs) stays resident,
  completed subtrees flush immediately.
- **Preimage memory auto-tuning** (`auto_tune_config`, `migrate.rs:48`): reads
  `/proc/meminfo`; if RAM allows, loads preimages into `FxHashMap` (fast); otherwise falls back
  to sorted **mmap'd flat files** with binary search (constant RAM). `--fast` forces in-memory.
- Docs ("Performance"): Hoodi ~33M accounts / ~268M storage slots → collect ~30 MB/s, build
  ~5M entries/sec, ~30 min total on 32 GB RAM + SSD. **Mainnet (~250M+ accounts) will be larger
  but the path is the streaming one** — the build phase itself is near-constant memory; the main
  RAM pressure is optionally holding preimages (mmap mode avoids it). Plan ~3x snapshot size
  disk headroom (export files + temp CF + final DB) per the guide.

---

## 6. Binary tree crate, DB backend, state-root validation

- **Crate:** `ethrex-binary-trie` at `crates/common/binary_trie/` (Cargo `name =
  "ethrex-binary-trie"`, dep `blake3 = "1"`). Modules: `key_mapping.rs`, `state.rs`
  (`BinaryTrieState`), `node.rs`, `node_store.rs`, `merkle.rs`, `trie.rs`, `hash.rs`,
  `layer_cache.rs`, `proof.rs`, `witness.rs`, `db.rs`.
- **DB backend:** **RocksDB** — `crates/storage/backend/rocksdb.rs` (uses `rocksdb::DBWithThreadMode`,
  column families, `iterator_cf`). Relevant CFs in `crates/storage/api/tables.rs`:
  - `BINARY_TRIE_NODES = "binary_trie_nodes"` (tables.rs:61) — the actual trie nodes.
  - `BINARY_TRIE_STORAGE_KEYS = "binary_trie_storage_keys"` (tables.rs:65).
  - `MIGRATION_TEMP = "migration_temp"` (tables.rs:96) — the collect-phase staging CF.
  - `ACCOUNT_FLATKEYVALUE` / `STORAGE_FLATKEYVALUE` (tables.rs:85,90) — the FKV flat layer.
  - `store.create_trie_backend()` yields the `Arc<dyn TrieBackend>` the bulk builder writes to.
- **State-root validation removed/disabled — CONFIRMED:**
  `crates/blockchain/blockchain.rs:1810-1821` — `validate_state_root(...)` carries the comment
  **"Currently unused: binary trie skips MPT state root validation."** The block-execution path
  populates the binary trie (lines ~853-870, 1070-1073, 1453) and sets a binary-trie root but
  does not gate on matching the header's MPT `state_root`. So your migrated state will not be
  rejected for a state-root mismatch against the legacy MPT root.

---

## Caveats / unconfirmed

- I did not exhaustively trace forward-sync re-execution from `at_block` end-to-end (whether
  every consensus check downstream is happy resuming at an arbitrary mid-chain block) — the
  migration writes `latest_block_number` and the docs claim forward sync works, but a live
  mainnet run is the only real confirmation.
- The exact mainnet export sizes/throughput are extrapolated from Hoodi numbers in the docs; no
  mainnet figures are published on the branch.
- Producing the snapshot/preimage/code dumps depends on the patched Geth fork
  (`edg-l/go-ethereum feat/export-code`) for the `code` export; upstream Geth lacks `db export
  code`. If you generate gethdbdump streams yourself for ~250M accounts, you must match the
  `a`/`o`/`c` prefixes, keccak-hash keys, slim-RLP account encoding, and provide a matching
  sorted preimage file (the mmap mode requires the flat files to be **sorted** by hash —
  produced internally, but your source preimage dump order is handled by ethrex either way:
  fast mode hashes, mmap mode sorts on write).
