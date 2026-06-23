#!/usr/bin/env python3
"""Robustly download Xatu canonical_execution Parquet for mainnet (0 -> head).

Per-file curl: --retry handles transient SSL/connection drops (the httpfs streaming
approach died on those); -f skips 404 gaps; skip-existing makes it resumable (re-run to
mop up misses). Modest parallelism to share bandwidth with the concurrent geth download.

Tables: storage_diffs (slots + addrs), balance_diffs / nonce_diffs / contracts (addrs).
Output: xatu-raw/<table>/<blockfloor>.parquet
"""
import os, subprocess, concurrent.futures as cf

BASE = "https://data.ethpandaops.io/xatu/mainnet/databases/default"
TABLES = [
    "canonical_execution_storage_diffs",
    "canonical_execution_balance_diffs",
    "canonical_execution_nonce_diffs",
    "canonical_execution_contracts",
]
MAX_BLK = 25_340_000
STEP = 1000
OUT = "xatu-raw"
WORKERS = 4

def fetch(table, blk):
    d = os.path.join(OUT, table)
    os.makedirs(d, exist_ok=True)
    p = os.path.join(d, f"{blk}.parquet")
    if os.path.exists(p) and os.path.getsize(p) > 0:
        return "skip", 0
    url = f"{BASE}/{table}/1000/{blk}.parquet"
    r = subprocess.run(
        ["curl", "-fsS", "--http1.1", "--retry", "6", "--retry-delay", "3",
         "-C", "-", "-o", p, url],
        capture_output=True,
    )
    if r.returncode == 0:
        return "ok", (os.path.getsize(p) if os.path.exists(p) else 0)
    # 404 (no activity in range) or failed: drop empty partial, count as miss
    if os.path.exists(p) and os.path.getsize(p) == 0:
        os.remove(p)
    return "miss", 0

def main():
    tasks = [(t, b) for t in TABLES for b in range(0, MAX_BLK + 1, STEP)]
    done = ok = miss = skip = 0
    nbytes = 0
    with cf.ThreadPoolExecutor(max_workers=WORKERS) as ex:
        for status, n in ex.map(lambda tb: fetch(*tb), tasks):
            done += 1
            if status == "ok": ok += 1; nbytes += n
            elif status == "miss": miss += 1
            else: skip += 1
            if done % 2000 == 0:
                print(f"{done}/{len(tasks)}  ok={ok} skip={skip} miss(404/fail)={miss}  {nbytes/1e9:.1f} GB",
                      flush=True)
    print(f"DONE {done}/{len(tasks)}  ok={ok} skip={skip} miss={miss}  {nbytes/1e9:.1f} GB")

if __name__ == "__main__":
    main()
