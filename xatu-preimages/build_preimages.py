#!/usr/bin/env python3
"""Build a gethdbdump preimage file from Xatu plain addresses + slot keys.

Input: two text files, one hex value per line (0x-prefixed):
  addrs.txt  -- distinct 20-byte addresses  (DISTINCT address from balance/nonce/storage diffs + contracts)
  slots.txt  -- distinct 32-byte slot keys  (DISTINCT slot from storage_diffs)
Output: preimages.rlp in the exact gethdbdump format `ethrex migrate` consumes
  (see docs: header 0xc0; per entry op=0x80 + RLP(key=keccak32) + RLP(value=plain)).
  migrate distinguishes address vs slot preimage by VALUE length (20 vs 32).
  Entries are sorted by keccak hash so mmap (binary-search) mode works.
"""
import sys
from Cryptodome.Hash import keccak

def k256(b: bytes) -> bytes:
    h = keccak.new(digest_bits=256); h.update(b); return h.digest()

def rlp_bytes(b: bytes) -> bytes:
    n = len(b)
    if n == 1 and b[0] < 0x80:
        return b
    if n <= 55:
        return bytes([0x80 + n]) + b
    ln = n.to_bytes((n.bit_length() + 7) // 8, "big")
    return bytes([0xb7 + len(ln)]) + ln + b

def load_hex(path):
    out = []
    with open(path) as f:
        for line in f:
            s = line.strip().strip('"')
            if not s:
                continue
            if s.startswith("0x") or s.startswith("0X"):
                s = s[2:]
            out.append(bytes.fromhex(s))
    return out

def main():
    addrs_path, slots_path, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
    addrs = load_hex(addrs_path)
    slots = load_hex(slots_path)
    # (keccak(x), x) -- value length (20|32) is how migrate tags addr vs slot.
    entries = []
    for a in addrs:
        assert len(a) == 20, f"bad address length {len(a)}"
        entries.append((k256(a), a))
    for s in slots:
        assert len(s) == 32, f"bad slot length {len(s)}"
        entries.append((k256(s), s))
    entries.sort(key=lambda e: e[0])          # sort by hash (mmap mode needs it)
    with open(out_path, "wb") as w:
        w.write(b"\xc0")                       # empty-list header (migrate skips it)
        for h, v in entries:
            w.write(b"\x80")                   # op = add
            w.write(rlp_bytes(h))              # key = 32-byte keccak hash
            w.write(rlp_bytes(v))              # value = plain addr(20) | slot(32)
    print(f"addrs={len(addrs)} slots={len(slots)} total_preimages={len(entries)}")
    if entries:
        h0, v0 = entries[0]
        print(f"first entry  keccak={h0.hex()}  ->  preimage(len {len(v0)})={v0.hex()}")

if __name__ == "__main__":
    main()
