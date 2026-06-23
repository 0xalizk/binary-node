#!/usr/bin/env python3
"""Find truncated/corrupt Xatu parquet files (missing trailing PAR1 magic).
Reads only the last 4 bytes of each file — tiny data, but many opens; run niced+ionice."""
import os

ROOT = "xatu-preimages/xatu-raw"
bad = []
n = 0
for tbl in sorted(os.listdir(ROOT)):
    d = os.path.join(ROOT, tbl)
    if not os.path.isdir(d):
        continue
    for e in os.scandir(d):
        if not e.name.endswith(".parquet"):
            continue
        n += 1
        try:
            sz = e.stat().st_size
            if sz < 8:
                bad.append((e.path, sz, "tiny"))
                continue
            with open(e.path, "rb") as fh:
                fh.seek(-4, 2)
                if fh.read(4) != b"PAR1":
                    bad.append((e.path, sz, "no-magic"))
        except Exception as ex:
            bad.append((e.path, -1, repr(ex)))
print(f"checked {n} parquet files; {len(bad)} bad")
for p, sz, why in bad:
    print(f"  {why}\t{sz}\t{p}")
# also write just the bad paths for re-download
with open("xatu-preimages/bad_parquet.txt", "w") as w:
    for p, _, _ in bad:
        w.write(p + "\n")
