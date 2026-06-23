//! snapshot-resorter
//!
//! Re-orders a geth `gethdbdump` snapshot so `ethrex migrate` resolves storage-slot preimages
//! with SEQUENTIAL (monotonic) lookups instead of random ones — making migrate memory-bound on
//! a small-RAM box, with NO change to migrate itself (output is order-invariant: migrate writes
//! (tree_key,value) to a RocksDB temp CF sorted by tree_key, then builds from that).
//!
//! Accounts ('a' entries, 33-byte key) are passed through in original order (already sorted by
//! keccak(addr) → sequential addr lookups). Storage ('o' entries, 65-byte key =
//! 'o'+keccak(addr)+keccak(slot)) are re-sorted by keccak(slot) (key[33..65]) via hash-partition
//! (bucket by keccak(slot)[0] → sort each bucket in RAM → concat). Bounded memory, sequential I/O.
//!
//! Usage: snapshot-resorter <in_snapshot.rlp> <out_snapshot.rlp> <tmp_dir>
//! Output gethdbdump: 0xc0 header (migrate skips it), then accounts (orig order) + storage
//! (keccak(slot) order), each entry = 0x80 op + RLP(key) + RLP(value), byte-faithful per entry.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

const NB: usize = 256;

/// Read one RLP byte-string from `r` into `dst` (mirrors migrate's read_rlp_bytes_into).
/// Returns false on clean EOF at the very start.
fn read_rlp(r: &mut impl Read, dst: &mut Vec<u8>) -> eyre::Result<bool> {
    let mut p = [0u8; 1];
    match r.read_exact(&mut p) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(false),
        Err(e) => return Err(e.into()),
    }
    let prefix = p[0];
    dst.clear();
    if prefix < 0x80 {
        dst.push(prefix);
    } else if prefix <= 0xb7 {
        let n = (prefix - 0x80) as usize;
        dst.resize(n, 0);
        r.read_exact(dst)?;
    } else if prefix <= 0xbf {
        let lol = (prefix - 0xb7) as usize;
        let mut lb = [0u8; 8];
        r.read_exact(&mut lb[..lol])?;
        let mut n = 0usize;
        for &b in &lb[..lol] { n = (n << 8) | b as usize; }
        dst.resize(n, 0);
        r.read_exact(dst)?;
    } else {
        eyre::bail!("unexpected RLP prefix {prefix:#x} (expected byte-string)");
    }
    Ok(true)
}

fn write_rlp(w: &mut impl Write, b: &[u8]) -> std::io::Result<()> {
    let n = b.len();
    if n == 1 && b[0] < 0x80 {
        w.write_all(b)
    } else if n <= 55 {
        w.write_all(&[0x80 + n as u8])?; w.write_all(b)
    } else {
        let mut lb = Vec::new();
        let mut x = n;
        while x > 0 { lb.push((x & 0xff) as u8); x >>= 8; }
        lb.reverse();
        w.write_all(&[0xb7 + lb.len() as u8])?; w.write_all(&lb)?; w.write_all(b)
    }
}

fn write_entry(w: &mut impl Write, key: &[u8], val: &[u8]) -> std::io::Result<()> {
    w.write_all(&[0x80])?;      // op = add
    write_rlp(w, key)?;
    write_rlp(w, val)
}

/// Skip the gethdbdump header (one RLP list) on `r`.
fn skip_header(r: &mut impl Read) -> eyre::Result<()> {
    let mut p = [0u8; 1];
    r.read_exact(&mut p)?;
    let len = match p[0] {
        b if b >= 0xf8 => {
            let lol = (b - 0xf7) as usize;
            let mut lb = [0u8; 8]; r.read_exact(&mut lb[..lol])?;
            let mut n = 0usize; for &x in &lb[..lol] { n = (n << 8) | x as usize; } n
        }
        b if b >= 0xc0 => (b - 0xc0) as usize,
        b => eyre::bail!("expected RLP list header, got {b:#x}"),
    };
    let mut skip = vec![0u8; len];
    r.read_exact(&mut skip)?;
    Ok(())
}

/// Read one full gethdbdump entry (op, key, val). Returns None on clean EOF.
fn read_entry(r: &mut impl Read, key: &mut Vec<u8>, val: &mut Vec<u8>) -> eyre::Result<Option<u8>> {
    let mut op = [0u8; 1];
    match r.read_exact(&mut op) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    if !read_rlp(r, key)? { return Ok(None); }
    read_rlp(r, val)?;
    Ok(Some(op[0]))
}

/// head: skip the first `skip` entries, then copy the next N complete entries of <in> to <out>
/// with a 0xc0 header (entry-aligned slice). `skip` lets the slice span into the storage section.
fn cmd_head(inp: &Path, outp: &Path, n: u64, skip: u64) -> eyre::Result<()> {
    let mut r = BufReader::with_capacity(1 << 23, File::open(inp)?);
    skip_header(&mut r)?;
    let mut out = BufWriter::with_capacity(1 << 23, File::create(outp)?);
    out.write_all(&[0xc0])?;
    let (mut key, mut val) = (Vec::new(), Vec::new());
    let mut s = 0u64;
    while s < skip {
        if read_entry(&mut r, &mut key, &mut val)?.is_none() { eyre::bail!("EOF during skip at {s}"); }
        s += 1;
    }
    let mut c = 0u64;
    while c < n {
        match read_entry(&mut r, &mut key, &mut val)? {
            Some(op) => { out.write_all(&[op])?; write_rlp(&mut out, &key)?; write_rlp(&mut out, &val)?; c += 1; }
            None => break,
        }
    }
    out.flush()?;
    eprintln!("head: wrote {c} entries to {}", outp.display());
    Ok(())
}

/// verify: load <orig> and <sorted> fully (small slices only), confirm identical (key,val) multiset
/// and that <sorted>'s storage section is non-decreasing by keccak(slot) with all accounts first.
fn cmd_verify(orig: &Path, sorted: &Path) -> eyre::Result<()> {
    fn load(p: &Path) -> eyre::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut r = BufReader::new(File::open(p)?);
        skip_header(&mut r)?;
        let (mut k, mut v) = (Vec::new(), Vec::new());
        let mut out = Vec::new();
        while let Some(op) = read_entry(&mut r, &mut k, &mut v)? {
            if op == 0x80 { out.push((k.clone(), v.clone())); }
        }
        Ok(out)
    }
    let a = load(orig)?;
    let b = load(sorted)?;
    eyre::ensure!(a.len() == b.len(), "entry count differs: orig {} vs sorted {}", a.len(), b.len());
    let mut sa: Vec<&(Vec<u8>, Vec<u8>)> = a.iter().collect();
    let mut sb: Vec<&(Vec<u8>, Vec<u8>)> = b.iter().collect();
    sa.sort(); sb.sort();
    eyre::ensure!(sa == sb, "(key,val) multiset differs between orig and sorted");
    // sortedness of sorted's storage + accounts-first
    let mut seen_storage = false;
    let mut prev: Option<&[u8]> = None;
    let (mut na, mut ns) = (0u64, 0u64);
    for (k, _) in &b {
        if k.len() == 33 && k[0] == b'a' {
            eyre::ensure!(!seen_storage, "account appears after storage began");
            na += 1;
        } else if k.len() == 65 && k[0] == b'o' {
            seen_storage = true;
            let slot = &k[33..65];
            if let Some(p) = prev { eyre::ensure!(p <= slot, "storage not sorted by keccak(slot)"); }
            prev = Some(slot);
            ns += 1;
        }
    }
    eprintln!("verify OK: {} entries, multiset identical, storage sorted by keccak(slot). {na} accounts, {ns} storage.", a.len());
    Ok(())
}

fn cmd_resort(inp: &Path, outp: &Path, tmp: &Path) -> eyre::Result<()> {
    fs::create_dir_all(tmp)?;
    let (inp, outp, tmp) = (inp.to_path_buf(), outp.to_path_buf(), tmp.to_path_buf());

    let mut r = BufReader::with_capacity(1 << 23, File::open(&inp)?);
    skip_header(&mut r)?;
    let mut out = BufWriter::with_capacity(1 << 23, File::create(&outp)?);
    out.write_all(&[0xc0])?; // empty-list header (migrate skips it)

    // bucket writers for storage; record = keylen-implied(65) key ++ u32 vlen ++ val
    let mut bw: Vec<BufWriter<File>> = (0..NB)
        .map(|b| BufWriter::with_capacity(1 << 20, File::create(tmp.join(format!("s{b:02x}.bin"))).unwrap()))
        .collect();

    let (mut n_acct, mut n_stor) = (0u64, 0u64);
    let (mut key, mut val) = (Vec::with_capacity(65), Vec::with_capacity(256));
    eprintln!("phase 1: stream snapshot; accounts -> out, storage -> 256 buckets by keccak(slot)[0]");
    loop {
        // op
        let mut op = [0u8; 1];
        match r.read_exact(&mut op) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
        if !read_rlp(&mut r, &mut key)? { break; }
        read_rlp(&mut r, &mut val)?;
        // migrate: op = if first==0x80 {0} else {first}; processes only op==0 (add), skips deletes.
        let decoded_op = if op[0] == 0x80 { 0u8 } else { op[0] };
        if decoded_op != 0 { continue; }
        if key.len() == 33 && key[0] == b'a' {
            write_entry(&mut out, &key, &val)?;          // account: pass through in order
            n_acct += 1;
        } else if key.len() == 65 && key[0] == b'o' {
            let b = key[33] as usize;                    // keccak(slot)[0]
            let w = &mut bw[b];
            w.write_all(&key)?;                          // 65 bytes
            w.write_all(&(val.len() as u32).to_le_bytes())?;
            w.write_all(&val)?;
            n_stor += 1;
        }
        if (n_acct + n_stor) % 100_000_000 == 0 {
            eprintln!("  scanned {} accounts + {} storage", n_acct, n_stor);
        }
    }
    for w in &mut bw { w.flush()?; }
    drop(bw);
    eprintln!("phase 1 done: {n_acct} accounts (emitted), {n_stor} storage (bucketed)");

    eprintln!("phase 2: sort each bucket by keccak(slot) and append to out");
    let mut emitted = 0u64;
    for b in 0..NB {
        let bp = tmp.join(format!("s{b:02x}.bin"));
        let mut buf = Vec::new();
        BufReader::new(File::open(&bp)?).read_to_end(&mut buf)?;
        // parse records: key[65] ++ vlen[u32 LE] ++ val
        let mut recs: Vec<(usize, usize)> = Vec::new(); // (key_off, val_off_after_len)
        let mut p = 0usize;
        while p < buf.len() {
            let key_off = p;
            let vlen = u32::from_le_bytes(buf[p + 65..p + 69].try_into().unwrap()) as usize;
            recs.push((key_off, vlen));
            p += 69 + vlen;
        }
        // sort by keccak(slot) = key[33..65]
        recs.sort_unstable_by(|x, y| buf[x.0 + 33..x.0 + 65].cmp(&buf[y.0 + 33..y.0 + 65]));
        for (ko, vlen) in &recs {
            let key = &buf[*ko..*ko + 65];
            let val = &buf[*ko + 69..*ko + 69 + *vlen];
            write_entry(&mut out, key, val)?;
            emitted += 1;
        }
        fs::remove_file(&bp).ok();
        if b % 16 == 0 { eprintln!("  through bucket {b:02x}: {emitted} storage emitted"); }
    }
    out.flush()?;
    eprintln!("done: {n_acct} accounts + {emitted} storage = {} entries -> {}", n_acct + emitted, outp.display());
    if emitted != n_stor { eyre::bail!("storage count mismatch: bucketed {n_stor} != emitted {emitted}"); }
    Ok(())
}

fn main() -> eyre::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage:\n  snapshot-resorter resort <in.rlp> <out.rlp> <tmp_dir>\n  snapshot-resorter head <in.rlp> <out.rlp> <n> [skip]\n  snapshot-resorter verify <orig.rlp> <sorted.rlp>";
    match args.first().map(String::as_str) {
        Some("resort") => cmd_resort(Path::new(&args[1]), Path::new(&args[2]), Path::new(&args[3])),
        Some("head") => cmd_head(Path::new(&args[1]), Path::new(&args[2]), args[3].parse()?,
            args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(0)),
        Some("verify") => cmd_verify(Path::new(&args[1]), Path::new(&args[2])),
        _ => { eprintln!("{usage}"); std::process::exit(2); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn we(out: &mut Vec<u8>, key: &[u8], val: &[u8]) {
        out.push(0x80);
        write_rlp(out, key).unwrap();
        write_rlp(out, val).unwrap();
    }

    // Build a synthetic gethdbdump with interleaved accounts + storage whose keccak(slot) bytes
    // are deliberately OUT of order, then resort and verify multiset-equality + sortedness.
    #[test]
    fn resort_roundtrip_and_sort() {
        let dir = std::env::temp_dir().join("srt_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let orig = dir.join("orig.rlp");
        let sorted = dir.join("sorted.rlp");
        let tmp = dir.join("tmp");

        let mut buf = Vec::new();
        buf.push(0xc0); // header
        // helper key builders
        let acct = |b: u8| { let mut k = vec![b'a']; k.extend_from_slice(&[b; 32]); k };
        let stor = |addr: u8, slot: u8| { let mut k = vec![b'o']; k.extend_from_slice(&[addr; 32]); k.extend_from_slice(&[slot; 32]); k };

        // interleaved: account, its storage (slot bytes in arbitrary order), next account...
        we(&mut buf, &acct(0x10), &[0x01, 0x02, 0x03]);           // account, multi-byte val
        we(&mut buf, &stor(0x10, 0xff), &[0xaa]);                 // slot 0xff
        we(&mut buf, &stor(0x10, 0x01), &[0xbb; 100]);            // slot 0x01, long val (>55B)
        we(&mut buf, &acct(0x20), &[]);                           // account, empty val
        we(&mut buf, &stor(0x20, 0x80), &[0x7f]);                 // slot 0x80
        we(&mut buf, &stor(0x05, 0x00), &[0x00]);                 // slot 0x00 (smallest)
        we(&mut buf, &stor(0x05, 0xab), &[0xcd, 0xef]);           // slot 0xab
        fs::write(&orig, &buf).unwrap();

        cmd_resort(&orig, &sorted, &tmp).unwrap();
        cmd_verify(&orig, &sorted).unwrap(); // asserts multiset identical + storage sorted by keccak(slot)

        // explicit ordering check on the sorted output
        let mut r = BufReader::new(File::open(&sorted).unwrap());
        skip_header(&mut r).unwrap();
        let (mut k, mut v) = (Vec::new(), Vec::new());
        let mut storage_slots = Vec::new();
        let mut accts = 0;
        while let Some(_) = read_entry(&mut r, &mut k, &mut v).unwrap() {
            if k[0] == b'a' { assert!(storage_slots.is_empty(), "account after storage"); accts += 1; }
            else { storage_slots.push(k[33]); }
        }
        assert_eq!(accts, 2);
        assert_eq!(storage_slots, vec![0x00, 0x01, 0x80, 0xab, 0xff]); // sorted by slot byte
        let _ = fs::remove_dir_all(&dir);
    }
}
