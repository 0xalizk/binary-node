//! preimage-builder
//!
//! Reads the distinct plain account-addresses + storage-slot keys (Parquet, one binary
//! column each), keccak256-hashes every value, and writes a Geth `gethdbdump` preimage
//! stream **globally sorted by hash** — which is what `ethrex migrate`'s mmap binary-search
//! requires (it does NOT sort internally).
//!
//! Sort method (1.77B entries won't fit in RAM): partition by hash[0] into 256 bucket files
//! (keccak output is uniform → even buckets), sort each bucket in memory by full hash, then
//! concatenate buckets 0x00..0xff → globally sorted.
//!
//! Usage: preimage-builder <distinct_dir> <out.rlp> <tmp_bucket_dir>
//!   distinct_dir: contains slots_*.parquet (32-byte values) and addrs_*.parquet (20-byte)
//!
//! gethdbdump output: 0xc0 header, then per entry: 0x80 (op=add) + RLP(hash 32B) + RLP(plain).
//! migrate tags addr vs slot by value length (20 vs 32).

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use arrow_array::{Array, BinaryArray, FixedSizeBinaryArray, LargeBinaryArray};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use tiny_keccak::{Hasher, Keccak};

const NBUCKETS: usize = 256;

fn keccak(data: &[u8]) -> [u8; 32] {
    let mut k = Keccak::v256();
    k.update(data);
    let mut o = [0u8; 32];
    k.finalize(&mut o);
    o
}

/// Write one RLP byte-string (all our values are 20 or 32 bytes → short-string form).
fn write_rlp<W: Write>(w: &mut W, b: &[u8]) -> std::io::Result<()> {
    debug_assert!(b.len() < 56 && !b.is_empty());
    w.write_all(&[0x80 + b.len() as u8])?;
    w.write_all(b)
}

/// For each parquet file in `dir` matching `prefix`, call `f(value_bytes)`.
fn for_each_value(dir: &Path, prefix: &str, mut f: impl FnMut(&[u8])) -> eyre::Result<u64> {
    let mut n = 0u64;
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.starts_with(prefix) && s.ends_with(".parquet"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    for path in files {
        let file = File::open(&path)?;
        let reader = ParquetRecordBatchReaderBuilder::try_new(file)?
            .with_batch_size(65536)
            .build()?;
        for batch in reader {
            let batch = batch?;
            let col = batch.column(0);
            if let Some(a) = col.as_any().downcast_ref::<BinaryArray>() {
                for i in 0..a.len() {
                    if a.is_valid(i) { f(a.value(i)); n += 1; }
                }
            } else if let Some(a) = col.as_any().downcast_ref::<LargeBinaryArray>() {
                for i in 0..a.len() {
                    if a.is_valid(i) { f(a.value(i)); n += 1; }
                }
            } else if let Some(a) = col.as_any().downcast_ref::<FixedSizeBinaryArray>() {
                for i in 0..a.len() {
                    if a.is_valid(i) { f(a.value(i)); n += 1; }
                }
            } else {
                eyre::bail!("unexpected column type in {:?}: {:?}", path, col.data_type());
            }
        }
    }
    Ok(n)
}

fn main() -> eyre::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().ok_or_else(|| eyre::eyre!("usage: <distinct_dir> <out.rlp> <tmp_dir>"))?);
    let out_path = PathBuf::from(args.next().ok_or_else(|| eyre::eyre!("missing out.rlp"))?);
    let tmp = PathBuf::from(args.next().ok_or_else(|| eyre::eyre!("missing tmp_dir"))?);
    fs::create_dir_all(&tmp)?;

    // ---- Phase 1: keccak every value, route to 256 hash-buckets ----
    // bucket record: hash[32] ++ vlen[1] ++ plain[vlen]
    eprintln!("phase 1: keccak + partition into {NBUCKETS} buckets");
    let mut bw: Vec<BufWriter<File>> = (0..NBUCKETS)
        .map(|b| BufWriter::with_capacity(1 << 20, File::create(tmp.join(format!("b{b:02x}.bin"))).unwrap()))
        .collect();
    let mut route = |v: &[u8]| -> std::io::Result<()> {
        let h = keccak(v);
        let w = &mut bw[h[0] as usize];
        w.write_all(&h)?;
        w.write_all(&[v.len() as u8])?;
        w.write_all(v)
    };
    let mut err: Option<std::io::Error> = None;
    let n_slots = for_each_value(&dir, "slots_", |v| { if err.is_none() { if let Err(e) = route(v) { err = Some(e); } } })?;
    let n_addrs = for_each_value(&dir, "addrs_", |v| { if err.is_none() { if let Err(e) = route(v) { err = Some(e); } } })?;
    if let Some(e) = err { return Err(e.into()); }
    for w in &mut bw { w.flush()?; }
    drop(bw);
    eprintln!("phase 1 done: {n_slots} slots + {n_addrs} addrs = {} values", n_slots + n_addrs);

    // ---- Phase 2: sort each bucket by hash, emit gethdbdump in bucket order ----
    eprintln!("phase 2: sort buckets + emit {}", out_path.display());
    let mut out = BufWriter::with_capacity(1 << 22, File::create(&out_path)?);
    out.write_all(&[0xc0])?; // empty-list header (migrate skips it)
    let mut total: u64 = 0;
    for b in 0..NBUCKETS {
        let bp = tmp.join(format!("b{b:02x}.bin"));
        let mut buf = Vec::new();
        BufReader::new(File::open(&bp)?).read_to_end(&mut buf)?;
        // parse records: 32 + 1 + vlen
        let mut recs: Vec<([u8; 32], u8, [u8; 32])> = Vec::new();
        let mut p = 0usize;
        while p < buf.len() {
            let mut h = [0u8; 32];
            h.copy_from_slice(&buf[p..p + 32]);
            let vlen = buf[p + 32] as usize;
            let mut v = [0u8; 32];
            v[..vlen].copy_from_slice(&buf[p + 33..p + 33 + vlen]);
            recs.push((h, vlen as u8, v));
            p += 33 + vlen;
        }
        recs.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        for (h, vlen, v) in &recs {
            out.write_all(&[0x80])?;            // op = add
            write_rlp(&mut out, h)?;            // key = 32-byte keccak hash
            write_rlp(&mut out, &v[..*vlen as usize])?; // value = plain (20 or 32)
            total += 1;
        }
        fs::remove_file(&bp).ok();
        if b % 16 == 0 { eprintln!("  emitted through bucket {b:02x}: {total} entries"); }
    }
    out.flush()?;
    eprintln!("done: {total} preimage entries -> {}", out_path.display());
    Ok(())
}
