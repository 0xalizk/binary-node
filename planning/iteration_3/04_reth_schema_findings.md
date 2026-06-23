# reth v2.2.0 on-disk schema — plain-state extractor reference (2026-06-17)

Authoritative source: **paradigmxyz/reth at tag `v2.2.0`** (commit
`88505c7fcbfdebfd3b56d88c86b62e950043c6c4`, released 2026-04-30). All citations are at
that tag, read via `gh api`/`static.crates.io`, NOT main.

**Crucial layout note:** the reth `v2.2.0` *workspace* is `version = "2.2.0"`, but the
types we care about (`Account`, `StorageEntry`, `Bytecode`, the `Compact` codec) are NOT
in the workspace tree — they come from external crates.io deps pinned in `Cargo.lock`:
- `reth-primitives-traits = 0.3.1` (source: `registry+crates.io`) — defines `Account`,
  `StorageEntry`, `Bytecode`.
- `reth-codecs = 0.3.1` / `reth-codecs-derive = 0.3.1` — the `Compact` codec + derive.
The DB plumbing crates (`reth-db`, `reth-db-api`, `reth-provider`, `reth-libmdbx`) ARE
path crates in the v2.2.0 tree. This split matters for the extractor's `Cargo.toml`
(see §4). Verified against `Cargo.lock` at v2.2.0.

**Storage-version note (IMPORTANT — corrects any "MDBX-only" assumption):** v2.2.0 is a
**v2 storage layout = MDBX + static files + RocksDB** (`database.version` file = `2`,
`crates/storage/db/src/version.rs:12`). `rocksdb = "0.24"` is a real workspace dep
(`Cargo.toml:613`), there is a `reth db migrate-v2` command, and **history indices**
(`AccountsHistory`/`StoragesHistory`) may live in RocksDB. **BUT the *current plain state*
(`PlainAccountState`/`PlainStorageState`) is still in MDBX** (§1, §6) — so the extractor
still only touches MDBX. RocksDB and static files carry history/changesets, not state.

---

## 1. Tables holding plain (raw-key) state — CONFIRMED

Defined by the `tables!` macro in `crates/storage/db-api/src/tables/mod.rs` at v2.2.0.

**`PlainAccountState`** (mod.rs lines 389–393) — account state keyed by RAW 20-byte address:
```rust
/// Stores the current state of an [`Account`].
table PlainAccountState {
    type Key = Address;     // raw 20-byte address, NOT keccak(address)
    type Value = Account;
}
```

**`PlainStorageState`** (mod.rs lines 395–400) — storage keyed by RAW address + RAW 32-byte slot.
This is a **DUPSORT** table:
```rust
/// Stores the current value of a storage key.
table PlainStorageState {
    type Key = Address;        // raw 20-byte address (the MDBX primary key)
    type Value = StorageEntry; // = { key: B256 (raw slot), value: U256 }
    type SubKey = B256;        // raw 32-byte slot is the DUPSORT subkey
}
```

Dupsort structure: one MDBX key (`Address`) maps to many duplicate values; each value is a
`StorageEntry` whose first 32 bytes (`key`) are the slot — that slot acts as the dupsort
subkey for `seek_by_key_subkey`. So the physical encoding of each duplicate is
`slot(32 bytes) || compact(U256 value)` (see §2).

`Account` / `StorageEntry` / `Bytecode` are imported from `reth_primitives_traits`
(mod.rs line 32). The `tables!` macro sets `const DUPSORT` from the presence of `SubKey`
(mod.rs lines 157, 163–167) — confirms `PlainStorageState` is dupsort and
`PlainAccountState` is not.

These are the **raw-key** (preimage) tables. The hashed equivalents `HashedAccounts`
(key `B256 = keccak(addr)`) and `HashedStorages` (mod.rs 468–481) are derived and NOT
what the EIP-7864 builder wants.

---

## 2. Key & value encodings

### Account value (`PlainAccountState` value)
`reth-primitives-traits-0.3.1/src/account.rs` lines ~26–37:
```rust
#[cfg_attr(feature = "reth-codec", derive(reth_codecs::Compact))]
pub struct Account {
    pub nonce: u64,
    pub balance: U256,
    pub bytecode_hash: Option<B256>,
}
```
Encoded with the **derived `Compact` codec** (`reth-codecs-derive-0.3.1`). The derive
emits a **flag header (bitfield)** followed by the field bytes, in field-declaration order.

**Flag header byte layout** (from `reth-codecs-derive-0.3.1/src/compact/flags.rs` +
`compact/mod.rs::get_bit_size`):
- `nonce` (u64) → `B4` = **4 bits** holding the byte-length of the compacted nonce (0–8).
  (`get_bit_size`: `"u64" => 4`, mod.rs line 193.)
- `balance` (U256) → `B6` = **6 bits** holding the byte-length of the compacted balance
  (0–32). (`"U256" => 6`, mod.rs line 195.)
- `bytecode_hash` (Option) → **1 bit** present/absent flag. (`"Option" => 1`, line 191;
  Option is a "flag type" so it's a single bool bit, not a length.)
- Total = 4+6+1 = **11 bits** → padded up to **2 bytes** (16 bits, 5 unused — pad logic in
  `flags.rs::pad_flag_struct`). So **the Account record starts with a 2-byte flag header.**

**Field bytes after the header** (derive `generate_to`/`generate_from` in
`compact/generator.rs`, `compact/structs.rs`):
- `nonce`: `u64::to_compact` → big-endian, leading zero bytes trimmed; `nonce_len` (the B4
  field) = number of bytes written. `0` writes 0 bytes.
- `balance`: `U256::to_compact` (`reth-codecs-0.3.1/src/lib.rs` lines 390–413) → big-endian,
  leading zero bytes trimmed; `balance_len` (the B6 field) = bytes written (0–32). `0`
  writes 0 bytes. Decode pads back to 32 bytes big-endian.
- `bytecode_hash`: if the present-bit is set, the **32 raw bytes** of the `B256` follow
  (B256 is a known fixed type, written verbatim, no length). If absent, nothing.

So a typical EOA `{nonce: n, balance: b, bytecode_hash: None}` = `[2-byte flags][n_len
bytes][b_len bytes]`. A contract appends 32 bytes of code hash. `bytecode_hash == None`
⇔ EOA / no code (decode side: present-bit clear).

### StorageEntry value (`PlainStorageState` value) — HAND-WRITTEN codec, NOT derived
`reth-primitives-traits-0.3.1/src/storage.rs`:
```rust
pub struct StorageEntry { pub key: B256, pub value: U256 }
```
Compact impl (storage.rs lines ~52–70) is **manual** (so the subkey stays uncompressed for
dupsort `seek_by_key_subkey`):
```rust
fn to_compact(...) {
    buf.put_slice(&self.key[..]);     // 32 raw bytes of slot, verbatim
    self.value.to_compact(buf) + 32   // then U256-compact of value
}
fn from_compact(buf, len) {
    let key = B256::from_slice(&buf[..32]);
    let (value, out) = U256::from_compact(&buf[32..], len - 32);
    ...
}
```
**No flag header.** Layout of each stored duplicate value =
`slot(32 bytes, raw) || value(U256 compact: big-endian, leading zeros trimmed, 0..32 bytes)`.

**Zero-value handling (critical):** `U256::to_compact` of `0` writes **zero bytes**, so a
slot with value 0 is encoded as just its 32-byte key + empty value (`len == 32`,
`from_compact` returns `U256::ZERO`). In the **PlainStorageState** table, slots that are
zero are simply **not present as rows** — reth deletes the storage entry when a slot is
cleared to zero (a zero slot has no MPT/state meaning). So the extractor should treat
"absent slot" = 0 and will only see non-zero slots. (The doc on `StorageChangeSets`,
mod.rs 455–457, notes a zero value there means "storage was not existing / needs removal"
— consistent with zero slots not living in plain state.)

### Keys
- `Address` encode = 20 raw bytes (fixed). `B256` (slot) = 32 raw bytes. Both via the
  `Encode`/`Decode` impls for fixed byte arrays (no length prefix, no compaction).
- MDBX orders keys lexicographically by these raw bytes.

---

## 3. Bytecode — `Bytecodes` table — CONFIRMED

`crates/storage/db-api/src/tables/mod.rs` lines 380–387:
```rust
/// Stores all smart contract bytecodes.
table Bytecodes {
    type Key = B256;        // = the account's bytecode_hash (keccak of code)
    type Value = Bytecode;
}
```
So `Account.bytecode_hash` (when `Some`) is the key into `Bytecodes` → contract code.

`Bytecode` codec (`reth-primitives-traits-0.3.1/src/account.rs` lines ~117–212, manual
`Compact`):
- `to_compact`: `u32 BE length` of the raw bytecode, then the raw bytecode bytes, then a
  **1-byte variant id**, then variant-specific data:
  - `LEGACY_ANALYZED_BYTECODE_ID = 2`: `u8(2)` + `u64 BE original_len` + jump-table bitmap
    bytes. (This is what reth stores for normal analyzed bytecode.)
  - `EIP7702_BYTECODE_ID = 4`: `u8(4)` only.
  - `LEGACY_RAW_BYTECODE_ID = 0`: raw, no extra (decode-only path).
  - `REMOVED_BYTECODE_ID = 1`: removed — `unreachable!` if seen.
- Variant ids in `compact_ids` module (account.rs lines 11–24).
- For an extractor that only needs the **raw bytecode bytes**, read the leading
  `u32 BE len` and take the next `len` bytes — that's the contract code; the trailing
  variant/jump-table data is revm analysis metadata you can ignore.

---

## 4. DB engine & how to open it read-only — CONFIRMED MDBX

### Engine + on-disk files
- Engine = **MDBX (libmdbx)**; reth vendors a fork at `crates/storage/libmdbx-rs`
  (`reth-libmdbx`), wrapped by `crates/storage/db` (`reth-db`,
  `src/implementation/mdbx/mod.rs`).
- On disk: the DB lives in the **`db/` subdirectory** of the datadir
  (`crates/node/core/src/dirs.rs:287-289` `db()` = `data_dir().join("db")`; `:294-300`
  `static_files()` = `data_dir().join("static_files")`; test asserts the path ends with
  `reth/mainnet/db`, `:412`).
- reth opens the env in **subdir mode** (never sets `MDBX_NOSUBDIR` — `no_sub_dir` defaults
  `false` at `crates/storage/libmdbx-rs/src/flags.rs:139-141`, `make_flags` at `:160-162`;
  `DatabaseEnv::open` at `crates/storage/db/src/implementation/mdbx/mod.rs:392-466` passes the
  `db/` directory to `Environment::open`). In subdir mode libmdbx's C code creates
  **`mdbx.dat`** (data) and **`mdbx.lck`** (lock) inside the dir. (The literal strings
  `mdbx.dat`/`mdbx.lck` are NOT in reth source — they're produced by libmdbx internally;
  reth only controls the dir name `db` and the subdir flag.)
- Alongside MDBX's files, reth also writes its own **`lock`** file
  (`crates/storage/db/src/lockfile.rs:14` `LOCKFILE_NAME = "lock"`) and **`database.version`**
  (`crates/storage/db/src/version.rs:9`, `DB_VERSION = 2` at `:12`).
- Default mainnet datadir on Linux: `~/.local/share/reth/mainnet/`, so the file is
  **`~/.local/share/reth/mainnet/db/mdbx.dat`**. (For the ethpandaops snapshot, it's wherever
  you extract the tarball: `<extract>/db/mdbx.dat`, `<extract>/static_files/`, and a RocksDB
  dir.)
- `DatabaseEnv::open` / `open_db_read_only` take the **`db/` directory path**, not the file.

### (a) Library path — read-only from a separate Rust program
Lowest-friction (no chainspec, no static-file provider needed for plain state):
```rust
use reth_db::{open_db_read_only, mdbx::DatabaseArguments, tables};
use reth_db_api::{transaction::DbTx, cursor::{DbCursorRO, DbDupCursorRO}};
use reth_db_api::models::ClientVersion;

let db = open_db_read_only(
    "<datadir>/db".as_ref(),
    DatabaseArguments::new(ClientVersion::default()),
)?;                              // -> DatabaseEnv (RO)
let tx = db.tx()?;               // RO transaction (Database::tx)

// Accounts — raw Address -> Account
let mut cur = tx.cursor_read::<tables::PlainAccountState>()?;
let mut w = cur.walk(None)?;
while let Some((address, account)) = w.next().transpose()? {
    // address: Address (raw 20 bytes); account.nonce/balance/bytecode_hash
}

// Storage — dupsort: raw Address -> StorageEntry{ key: B256 slot, value: U256 }
let mut scur = tx.cursor_dup_read::<tables::PlainStorageState>()?;
let mut sw = scur.walk(None)?;
while let Some((address, entry)) = sw.next().transpose()? {
    // entry.key = raw 32-byte slot, entry.value = U256 (non-zero only)
}
```
- `open_db_read_only` is defined at `crates/storage/db/src/mdbx.rs:140-147`, re-exported
  from `reth_db` (`crates/storage/db/src/lib.rs:35`); opens the env in `DatabaseEnvKind::RO`.
  Signature: `open_db_read_only(path: impl AsRef<Path>, args: DatabaseArguments) ->
  eyre::Result<DatabaseEnv>`. (`init_db` at `mdbx.rs:101`.) `path` = the `db/` directory.
- Cursor trait line refs (crate `reth-db-api`): `DbTx::cursor_read` at
  `crates/storage/db-api/src/transaction.rs:42`, `cursor_dup_read` at `:44`, `get` at `:28`;
  `DbCursorRO::walk` at `crates/storage/db-api/src/cursor.rs:39`, `walk_range` at `:44`;
  `DbDupCursorRO::walk_dup` at `:99`, `seek_by_key_subkey` at `:86`. Walkers are
  `Iterator<Item = Result<(Key, Value), DatabaseError>>`.
- If you instead want full `ProviderFactory`/`DatabaseProviderRO` (crate `reth-provider`,
  `crates/storage/provider/src/providers/database/mod.rs:76`): note that at v2.2.0
  `ProviderFactory::new` (`:121`) and `new_with_database_path` (`:339`) require **extra args
  vs older reth** — a `rocksdb_provider: RocksDBProvider` AND a `runtime: reth_tasks::Runtime`
  — and the `ProviderFactoryBuilder::open_read_only(...)` helper (`builder.rs:95`) spins up
  RocksDB + static-file providers too. **So for a plain-state dump, AVOID ProviderFactory** —
  the raw `open_db_read_only` → `db.tx()` → `cursor_read` path above is strictly simpler and
  needs no chainspec/RocksDB/static-files/runtime.
- MDBX supports multiple concurrent readers, so a separate RO open is fine even alongside a
  running node (though for a static snapshot the node isn't running anyway).

**Extractor `Cargo.toml` (version pinning — IMPORTANT):** the DB crates are path-pinned to
the v2.2.0 tree, so depend on them by git tag; the type crate is on crates.io:
```toml
reth-db                 = { git = "https://github.com/paradigmxyz/reth", tag = "v2.2.0" }
reth-db-api             = { git = "https://github.com/paradigmxyz/reth", tag = "v2.2.0" }
reth-provider           = { git = "https://github.com/paradigmxyz/reth", tag = "v2.2.0" } # only if using ProviderFactory
reth-primitives-traits  = "0.3.1"   # Account/StorageEntry/Bytecode (matches v2.2.0 lock)
```
A version mismatch against the on-disk `database.version` will be rejected — keep these at
v2.2.0 / 0.3.1.

### (b) `reth db` CLI subcommands
Subcommand enum at `crates/cli/commands/src/db/mod.rs:41-85` (Stats, List, Checksum, Copy,
Diff, Get, Drop, Clear, RepairTrie, StaticFileHeader, Version, Path, Settings,
PruneCheckpoints, StageCheckpoints, AccountStorage, State, MigrateV2). Useful ones:
- **`reth db stats`** (`db/stats.rs:20`) — per-table entry counts/sizes (+ static-file +
  RocksDB stats). Use for the ~250M-account sanity check.
- **`reth db list <TABLE>`** (`db/list.rs:13`) — `reth db list PlainAccountState` /
  `reth db list PlainStorageState` both work (positional `table: Tables`, `:17`). Has
  **`--raw`** (`:51`, raw encoded bytes — useful to verify the Compact layout above). Default
  is a TUI; `--json`/`-j` dumps JSON; `--count`/`-c`; pagination `--skip`/`--len`/`--reverse`.
- **`reth db get <SUB> <TABLE> <KEY>`** (`db/get.rs:32`) — at v2.2.0 `get` is split into
  `mdbx` / `static-file` / `rocksdb` sub-subcommands. For plain state use **`reth db get mdbx
  PlainAccountState <key>`** (and `... PlainStorageState <key> [<subkey>]` for dupsort). The
  bare `reth db get <Table> <key>` form does NOT exist at v2.2.0. All three carry `--raw`.
- **`reth db checksum`** (`db/checksum/mod.rs:28`) — also split: `reth db checksum mdbx
  PlainAccountState [start_key] [end_key] [limit]`.
- Table names parse generically from the `Tables` enum (`FromStr`, tables/mod.rs:242-253), so
  any variant name (incl. `PlainAccountState`/`PlainStorageState`) is accepted.
- Read subcommands open the DB read-only.

---

## 5. Pruning impact of `--full` — CONFIRMED: plain state is NEVER pruned

Snapshot node ran `--full --prune.bodies.pre-merge --prune.receipts.before=15537394
--engine.persistence-threshold=0`.

**`--full` does not prune `PlainAccountState`/`PlainStorageState`. No prune segment in reth
v2.2.0 can touch current plain state at all** — pruning is structurally limited to
historical/derived data.

- `--full` flag: `crates/node/core/src/args/pruning.rs` lines 98–101.
- What `--full` sets (`DefaultPruningValues`, pruning.rs 68–94; applied in
  `prune_config()` lines 217–233):
  - `sender_recovery = Full` (prunes `TransactionSenders`)
  - `transaction_lookup = None`
  - `receipts = Distance(10_064)` (overridden below by your flag)
  - `account_history = Distance(10_064)` — prunes account **changesets + `AccountsHistory`**,
    NOT `PlainAccountState`
  - `storage_history = Distance(10_064)` — prunes storage **changesets + `StoragesHistory`**,
    NOT `PlainStorageState`
  - `bodies_history = Before(Paris/merge block ≈ 15_537_394)` (prunes block bodies in static files)
  - `MINIMUM_UNWIND_SAFE_DISTANCE = 10_064` (`crates/prune/types/src/lib.rs`).
- **`PruneModes`** (`crates/prune/types/src/target.rs`) has fields ONLY for:
  `sender_recovery, transaction_lookup, receipts, account_history, storage_history,
  bodies_history, receipts_log_filter`. **No plain-state field exists.**
- **`PruneSegment`** enum (`crates/prune/types/src/segment.rs` lines 19–55): active variants
  are `SenderRecovery`, `TransactionLookup`, `Receipts`, `ContractLogs`, `AccountHistory`
  ("prunes account changesets and `AccountsHistory`"), `StorageHistory` ("prunes storage
  changesets and `StoragesHistory`"), `Bodies`. **There is NO `PlainAccountState`/
  `PlainStorageState` segment** — current state is outside the prune model entirely.
- Your explicit flags only refine historical pruning:
  - `--prune.bodies.pre-merge` → `bodies_history = Before(Paris)` (block **bodies**;
    redundant with `--full`).
  - `--prune.receipts.before=15537394` → `receipts = Before(15537394)` (**receipts**).
  - `--engine.persistence-threshold=0` → engine flush cadence knob
    (`crates/node/core/src/args/engine.rs`); not a prune segment, cannot prune state.

**Conclusion: `PlainAccountState` and `PlainStorageState` in this snapshot contain the
COMPLETE current state at the snapshot block (25,330,000).** The extractor is valid.

> Caveat (flagged by the pruning sweep): the CLI→`PruneModes`→`PruneSegment` definition
> chain was verified exhaustively; the prune *executor* bodies in
> `crates/prune/prune/src/segments/` were not byte-read, but they dispatch solely on the
> `PruneSegment` variants above, of which none is plain-state — so plain state is
> unreachable by the executor.

---

## 6. Static files / RocksDB vs MDBX — CONFIRMED: PLAIN STATE IS MDBX-ONLY

`StaticFileSegment` enum, `crates/static-file/types/src/segment.rs:31-65` at v2.2.0 has
**SIX** variants (more than older reth — flag this):
```rust
pub enum StaticFileSegment {
    Headers,             // CanonicalHeaders, Headers, HeaderTerminalDifficulties
    Transactions,        // Transactions table (block bodies)
    Receipts,            // Receipts table
    TransactionSenders,  // TransactionSenders table
    AccountChangeSets,   // AccountChangeSets (historical per-block account diffs)
    StorageChangeSets,   // StorageChangeSets (historical per-block storage diffs)
}
```
**No `Account`/`Storage`/`PlainAccountState`/`PlainStorageState` variant** — grep of
`segment.rs` + `static-file/types/src/lib.rs` returns nothing for plain state. Static files
hold headers, transactions, receipts, tx-senders, and **changesets (diffs)** — NOT current
state. Additionally, **history indices** (`AccountsHistory`/`StoragesHistory`) may live in
RocksDB (`reth db get rocksdb accounts-history|storages-history`).

**Current account & storage plain state lives EXCLUSIVELY in the MDBX
`PlainAccountState`/`PlainStorageState` tables** (`crates/storage/db-api/src/tables/mod.rs:390-393`
and `:396-400`) under `<datadir>/db/mdbx.dat`. The extractor needs **MDBX only**; it can
ignore `static_files/` and RocksDB entirely (those carry history/changesets, not state).

---

## Uncertainties / things to double-check at v2.2.0 (flagged)
1. `DatabaseArguments` constructor: I show both `DatabaseArguments::new(ClientVersion::
   default())` and `::default()`. Confirm the exact form against the tag if it fails to
   compile — the rest of `open_db_read_only`'s signature (`mdbx.rs:140`) is pinned.
2. `reth db` flag *names* (`--raw` pinned at `list.rs:51`; `--skip`/`--len`/`--reverse`/
   `--json`/`-c` present) — sanity-check against `reth db list --help` on the actual binary.
   Note the `get`/`checksum` `mdbx` sub-subcommand requirement (no bare `get <table> <key>`).
3. Prune *executor* segment bodies in `crates/prune/prune/src/segments/` not byte-read (see
   §5 caveat) — the `PruneModes`/`PruneSegment` definitions already prove plain state is never
   a prune target; the executor only dispatches on those variants.

Everything else — table names + dupsort structure (§1), Account/StorageEntry/Bytecode
encodings + flag-byte layout (§2–§3), MDBX subdir-mode file names + `open_db_read_only` +
cursor trait line refs (§4), the `--full`→`PruneModes` mapping (§5), and the six
`StaticFileSegment` variants (§6) — is confirmed directly from source at tag `v2.2.0` and the
pinned `reth-primitives-traits` 0.3.1 + `reth-codecs` 0.3.1.
