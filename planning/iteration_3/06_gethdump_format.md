# gethdbdump format the extractor must emit (byte-exact, 2026-06-17)

Reverse-engineered from `cmd/ethrex/migrate.rs` on branch `eip-7864-plan` (commit
`b0fe293`). The extractor (reth → 3 files) must emit exactly this so ethrex `migrate`
consumes it unmodified. Line refs are into `migrate.rs`.

## Container framing (all three files)
- **Header:** one RLP **list** header, then `migrate` *skips its declared payload bytes*
  (`skip_gethdbdump_header`, :971). It does NOT validate content. ⇒ emit a single
  **`0xc0`** (empty list, payload len 0) and nothing else for the header. Verified:
  `0xc0` → `b>=0xc0` branch → `header_len = 0` → skips 0 bytes. ✅
- **Then a stream of entries**, each (`read_gethdbdump_entry`, :1016):
  `op_byte` ++ `RLP(key)` ++ `RLP(value)`.
  - `op_byte`: **`0x80`** = add (decoded to op 0; only op==0 is used). Use 0x80 for everything.
  - `RLP(key)`, `RLP(value)`: each a single RLP **byte string** (`read_rlp_bytes_into`, :1042).
- No outer length to backfill — it's a pure stream after the 1-byte header. Easy to write.

## RLP byte-string encoding (what we emit for key/value)
- empty → `0x80`.
- single byte `< 0x80` → that byte verbatim.
- len `1..=55` → `0x80+len` ++ bytes.
- len `56..` → `0xb7+len_of_len` ++ big-endian(len) ++ bytes.
(Matches the reader at :1042-1069. 33-byte key → `0xa1`++33B. 65-byte key → `0xb8 0x41`++65B.)

## File 1 — preimages.rlp  (`parse_geth_dump`, :824)
Per entry: `key = keccak(x)` (32B), `value = x`. ethrex takes `key[len-32..]` as the hash
(:888) and **switches on value length** (:890):
- **address preimage:** value length **20** → `keccak(address) → address`.
- **storage-slot preimage:** value length **32** → `keccak(slot_key) → slot_key`.
Emit one address-preimage per account, one slot-preimage per distinct storage slot key.

⚠️ **Sorting:** in `--fast` (in-memory HashMap) order is irrelevant. In mmap mode ethrex
**binary-searches** `preimage_addrs.bin`/`preimage_slots.bin` (:744), which it builds by
streaming our entries in order — so for mmap mode our preimage stream MUST be globally
sorted by the 32-byte hash (geth's is, because its DB is key-ordered). reth iterates by
raw address (NOT hash order), so our output is unsorted. → **Smoke test: run migrate with
`--fast`.** Mainnet: either `--fast` (needs enough RAM for ~all preimages) or add an
external sort-by-hash step before migrate. (TODO for mainnet scale.)

## File 2 — snapshot.rlp  (`collect_phase`, :294; keys at :334/:341)
- **Account entry:** `key = 'a'(0x61) ++ keccak(address)` (33B); `value = slim-RLP account`.
  - slim-RLP (`decode_slim_account`, :558) = RLP list **[nonce, balance, root, codehash]**:
    - `nonce`: RLP of minimal big-endian u64 (0 → `0x80`).
    - `balance`: RLP of minimal big-endian U256 (0 → `0x80`).
    - `root`: emit **empty string `0x80`** (migrate decodes-and-skips it, :581).
    - `codehash`: empty `0x80` if no code (→ migrate substitutes `EMPTY_CODE_HASH`, :590),
      else the 32-byte code hash.
- **Storage entry:** `key = 'o'(0x6f) ++ keccak(address) ++ keccak(slot_key)` (65B);
  `value = RLP(trimmed-big-endian(slot_value))` (decoded at :422-432 via `decode_rlp_item`
  then `U256::from_big_endian`). **Omit zero-valued slots** (migrate drops them, :434; reth
  omits them from the table anyway).

## File 3 — code.rlp  (`parse_code_dump`, :783)
- `key = 'c'(0x63) ++ code_hash` (33B); `value = raw bytecode`.
- `code_hash` is `keccak(code)` — which is exactly reth's `Bytecodes` table key, so **no
  hashing needed**: stream `Bytecodes` straight through. Omit the empty-code entry.
- Without `--code`, accounts still load (balances/nonces fine) but contract code chunks +
  `code_size` are absent (:380-398). We pass it.

## Constants
- `EMPTY_CODE_HASH` = `keccak256("")` =
  `c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470`.
- Address = 20 bytes, slot key / hashes = 32 bytes.

## reth → emission mapping (source tables, from 04_reth_schema_findings.md)
- `PlainAccountState`: `Address → Account{nonce:u64, balance:U256, bytecode_hash:Option<B256>}`
  → for each: addr-preimage + 'a' account entry. `bytecode_hash` None or == EMPTY_CODE_HASH
  → emit empty codehash; else the 32B hash.
- `PlainStorageState` (DUPSORT): `Address → StorageEntry{key:B256, value:U256}` (zeros absent)
  → for each: slot-preimage (keccak(key)→key) + 'o' storage entry (skip if value==0).
- `Bytecodes`: `B256(code_hash) → Bytecode` → 'c' code entry (raw code bytes).

## migrate invocation (smoke test)
`ethrex --network hoodi migrate preimages.rlp snapshot.rlp --code code.rlp --at-block <N> --fast`
where `<N>` = the block the reth snapshot's state is post-execution of (the snapshot block).
