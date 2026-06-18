//! reth-state-extractor
//!
//! Reads a reth v2.2.0 plain-state database (read-only) and emits three Geth
//! `gethdbdump`-format files that ethrex's `migrate` (branch `eip-7864-plan`)
//! consumes UNMODIFIED to bulk-build an EIP-7864 binary trie:
//!
//!   preimages.rlp   keccak(x) -> x      (addr: value len 20, slot: value len 32)
//!   snapshot.rlp    'a'+keccak(addr)    -> slim-RLP [nonce, balance, "", codehash]
//!                   'o'+keccak(addr)+keccak(slot) -> RLP(trimmed-BE value)
//!   code.rlp        'c'+keccak(code)    -> raw bytecode
//!
//! Byte-exact format spec: planning/iteration_3/gethdump_format.md
//! Source tables (reth v2.2.0): planning/iteration_3/reth_schema_findings.md
//!
//! Usage:
//!   reth-state-extractor <reth_datadir> <out_dir>
//!     <reth_datadir>: the extracted reth datadir (must contain db/mdbx.dat)
//!     <out_dir>:      where preimages.rlp / snapshot.rlp / code.rlp are written
//!
//! NOTE: preimage entries are emitted in reth iteration order (by raw address),
//! NOT sorted by keccak hash. Run `ethrex migrate ... --fast` (in-memory preimage
//! lookups) so sortedness is not required. For mainnet/mmap mode, sort the
//! preimage files by their 32-byte hash key first (see gethdump_format.md).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use alloy_primitives::{keccak256, B256, U256};

// reth v2.2.0 DB access.
use reth_db::{open_db_read_only, tables, Database};
use reth_db_api::cursor::{DbCursorRO, DbDupCursorRO};
use reth_db_api::transaction::DbTx;

/// keccak256("") — Geth's empty-code-hash sentinel.
const EMPTY_CODE_HASH: B256 = B256::new([
    0xc5, 0xd2, 0x46, 0x01, 0x86, 0xf7, 0x23, 0x3c, 0x92, 0x7e, 0x7d, 0xb2, 0xdc, 0xc7, 0x03, 0xc0,
    0xe5, 0x00, 0xb6, 0x53, 0xca, 0x82, 0x27, 0x3b, 0x7b, 0xfa, 0xd8, 0x04, 0x5d, 0x85, 0xa4, 0x70,
]);

// ----------------------------------------------------------------------------
// gethdbdump writer (pure; no reth deps — these are byte-exact and testable)
// ----------------------------------------------------------------------------

/// A gethdbdump stream: a single `0xc0` empty-list header, then `op + RLP(key) +
/// RLP(value)` entries with op = 0x80 (add).
struct DumpWriter {
    w: BufWriter<File>,
    entries: u64,
}

impl DumpWriter {
    fn create(path: &Path) -> eyre::Result<Self> {
        let mut w = BufWriter::with_capacity(8 * 1024 * 1024, File::create(path)?);
        // Header: empty RLP list. migrate skips its (zero-length) payload.
        w.write_all(&[0xc0])?;
        Ok(Self { w, entries: 0 })
    }

    fn entry(&mut self, key: &[u8], value: &[u8]) -> eyre::Result<()> {
        self.w.write_all(&[0x80])?; // op = add
        write_rlp_string(&mut self.w, key)?;
        write_rlp_string(&mut self.w, value)?;
        self.entries += 1;
        Ok(())
    }

    fn finish(mut self) -> eyre::Result<u64> {
        self.w.flush()?;
        Ok(self.entries)
    }
}

/// Write one RLP byte-string. Matches ethrex's `read_rlp_bytes_into`.
fn write_rlp_string<W: Write>(w: &mut W, bytes: &[u8]) -> eyre::Result<()> {
    let len = bytes.len();
    if len == 1 && bytes[0] < 0x80 {
        w.write_all(bytes)?;
    } else if len <= 55 {
        w.write_all(&[0x80 + len as u8])?;
        w.write_all(bytes)?;
    } else {
        let be = (len as u64).to_be_bytes();
        let len_be = trim_left(&be);
        w.write_all(&[0xb7 + len_be.len() as u8])?;
        w.write_all(len_be)?;
        w.write_all(bytes)?;
    }
    Ok(())
}

/// RLP-encode a U256 as a minimal big-endian byte string (0 -> empty string 0x80).
fn rlp_u256(v: &U256) -> Vec<u8> {
    let be: [u8; 32] = v.to_be_bytes();
    rlp_string_owned(trim_left(&be))
}

/// RLP-encode a u64 as a minimal big-endian byte string (0 -> 0x80).
fn rlp_u64(v: u64) -> Vec<u8> {
    rlp_string_owned(trim_left(&v.to_be_bytes()))
}

/// RLP a raw byte string into an owned Vec (used to assemble the slim account list).
fn rlp_string_owned(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 9);
    write_rlp_string(&mut out, bytes).expect("vec write");
    out
}

/// RLP list header for a payload of `payload_len` bytes, prepended to `payload`.
fn rlp_list(payload: Vec<u8>) -> Vec<u8> {
    let len = payload.len();
    let mut out = Vec::with_capacity(len + 9);
    if len <= 55 {
        out.push(0xc0 + len as u8);
    } else {
        let be = (len as u64).to_be_bytes();
        let len_be = trim_left(&be);
        out.push(0xf7 + len_be.len() as u8);
        out.extend_from_slice(len_be);
    }
    out.extend_from_slice(&payload);
    out
}

/// Strip leading zero bytes (minimal big-endian). All-zero -> empty slice.
fn trim_left(b: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < b.len() && b[i] == 0 {
        i += 1;
    }
    &b[i..]
}

/// Build a Geth slim-RLP account: [nonce, balance, root="", codehash|""].
fn slim_account(nonce: u64, balance: &U256, code_hash: Option<B256>) -> Vec<u8> {
    let mut payload = Vec::with_capacity(80);
    payload.extend_from_slice(&rlp_u64(nonce));
    payload.extend_from_slice(&rlp_u256(balance));
    payload.extend_from_slice(&[0x80]); // root: empty string (migrate skips it)
    match code_hash {
        Some(h) if h != EMPTY_CODE_HASH => payload.extend_from_slice(&rlp_string_owned(h.as_slice())),
        _ => payload.push(0x80), // empty -> migrate substitutes EMPTY_CODE_HASH
    }
    rlp_list(payload)
}

// ----------------------------------------------------------------------------
// reth read + emit
// ----------------------------------------------------------------------------

fn main() -> eyre::Result<()> {
    let mut args = std::env::args().skip(1);
    let reth_datadir = PathBuf::from(
        args.next()
            .ok_or_else(|| eyre::eyre!("usage: reth-state-extractor <reth_datadir> <out_dir>"))?,
    );
    let out_dir = PathBuf::from(
        args.next()
            .ok_or_else(|| eyre::eyre!("usage: reth-state-extractor <reth_datadir> <out_dir>"))?,
    );
    std::fs::create_dir_all(&out_dir)?;

    let db_dir = reth_datadir.join("db");
    eprintln!("opening reth db (read-only): {}", db_dir.display());
    let db = open_db_read_only(&db_dir, Default::default())
        .map_err(|e| eyre::eyre!("open reth db: {e}"))?;
    let tx = db.tx().map_err(|e| eyre::eyre!("begin ro tx: {e}"))?;

    let mut preimages = DumpWriter::create(&out_dir.join("preimages.rlp"))?;
    let mut snapshot = DumpWriter::create(&out_dir.join("snapshot.rlp"))?;
    let mut code = DumpWriter::create(&out_dir.join("code.rlp"))?;

    // ---- Accounts: PlainAccountState (Address -> Account) ----
    eprintln!("streaming PlainAccountState ...");
    let mut acc_cur = tx.cursor_read::<tables::PlainAccountState>()?;
    let mut walker = acc_cur.walk(None)?;
    let mut n_acc = 0u64;
    while let Some(res) = walker.next() {
        let (address, account) = res?;
        let kaddr = keccak256(address.as_slice()); // B256
        // address preimage: keccak(addr) -> addr (value len 20)
        preimages.entry(kaddr.as_slice(), address.as_slice())?;
        // 'a' + keccak(addr) -> slim account
        let mut key = Vec::with_capacity(33);
        key.push(b'a');
        key.extend_from_slice(kaddr.as_slice());
        let slim = slim_account(account.nonce, &account.balance, account.bytecode_hash);
        snapshot.entry(&key, &slim)?;
        n_acc += 1;
        if n_acc % 1_000_000 == 0 {
            eprintln!("  accounts: {n_acc}");
        }
    }

    // ---- Storage: PlainStorageState DUPSORT (Address -> StorageEntry{key,value}) ----
    eprintln!("streaming PlainStorageState ...");
    let mut st_cur = tx.cursor_dup_read::<tables::PlainStorageState>()?;
    let mut st_walker = st_cur.walk_dup(None, None)?;
    let mut n_slot = 0u64;
    while let Some(res) = st_walker.next() {
        let (address, entry) = res?;
        // reth omits zero slots, but guard anyway.
        if entry.value.is_zero() {
            continue;
        }
        let kaddr = keccak256(address.as_slice());
        let slot_key: B256 = entry.key; // raw 32-byte slot key
        let kslot = keccak256(slot_key.as_slice());
        // slot preimage: keccak(slot) -> slot (value len 32)
        preimages.entry(kslot.as_slice(), slot_key.as_slice())?;
        // 'o' + keccak(addr) + keccak(slot) -> RLP(trimmed-BE value)
        let mut key = Vec::with_capacity(65);
        key.push(b'o');
        key.extend_from_slice(kaddr.as_slice());
        key.extend_from_slice(kslot.as_slice());
        let val = rlp_u256(&entry.value);
        snapshot.entry(&key, &val)?;
        n_slot += 1;
        if n_slot % 5_000_000 == 0 {
            eprintln!("  slots: {n_slot}");
        }
    }

    // ---- Code: Bytecodes (B256 code_hash -> Bytecode) ----
    eprintln!("streaming Bytecodes ...");
    let mut code_cur = tx.cursor_read::<tables::Bytecodes>()?;
    let mut code_walker = code_cur.walk(None)?;
    let mut n_code = 0u64;
    while let Some(res) = code_walker.next() {
        let (code_hash, bytecode) = res?;
        if code_hash == EMPTY_CODE_HASH {
            continue;
        }
        let raw = bytecode.bytecode().as_ref(); // raw bytecode bytes
        let mut key = Vec::with_capacity(33);
        key.push(b'c');
        key.extend_from_slice(code_hash.as_slice());
        code.entry(&key, raw)?;
        n_code += 1;
    }

    let p = preimages.finish()?;
    let s = snapshot.finish()?;
    let c = code.finish()?;
    eprintln!(
        "done: {n_acc} accounts, {n_slot} storage slots, {n_code} bytecodes\n  preimages.rlp entries={p}\n  snapshot.rlp entries={s}\n  code.rlp entries={c}"
    );
    Ok(())
}
