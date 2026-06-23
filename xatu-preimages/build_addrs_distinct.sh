#!/bin/bash
# Hash-partitioned distinct of Xatu account addresses (bounded memory).
# Sources: balance_diffs + nonce_diffs + contracts (storage_diffs.address omitted:
# every storage-bearing addr is a contract => has a nonce change => in nonce_diffs).
set -e
cd /home/0xalizk/sharded-pir/binary-node
R=/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw
TMP=/home/0xalizk/sharded-pir/binary-node/duckdb-tmp
PARTS=/home/0xalizk/sharded-pir/binary-node/xatu-preimages/parts_addrs
OUT=/home/0xalizk/sharded-pir/binary-node/xatu-preimages/distinct
DB=./duckdb
rm -rf "$PARTS"; mkdir -p "$PARTS" "$OUT"; rm -f "$OUT"/addrs_*.parquet

echo "### PASS 1 (partition addresses -> 16 buckets) $(date -u +%H:%M)"
"$DB" -c "SET threads=4; SET memory_limit='12GB'; SET temp_directory='$TMP'; SET preserve_insertion_order=false;
COPY (
  SELECT unhex(substr(CAST(a AS VARCHAR),3,40)) AS addr, substr(CAST(a AS VARCHAR),3,1) AS bk
  FROM (
    SELECT address AS a FROM read_parquet('$R/canonical_execution_balance_diffs/*.parquet') WHERE address IS NOT NULL
    UNION ALL SELECT address FROM read_parquet('$R/canonical_execution_nonce_diffs/*.parquet') WHERE address IS NOT NULL
    UNION ALL SELECT contract_address FROM read_parquet('$R/canonical_execution_contracts/*.parquet') WHERE contract_address IS NOT NULL
    UNION ALL SELECT deployer FROM read_parquet('$R/canonical_execution_contracts/*.parquet') WHERE deployer IS NOT NULL
    UNION ALL SELECT factory FROM read_parquet('$R/canonical_execution_contracts/*.parquet') WHERE factory IS NOT NULL
  )
) TO '$PARTS' (FORMAT parquet, PARTITION_BY (bk), OVERWRITE_OR_IGNORE);"
echo "### PASS 1 done $(date -u +%H:%M); parts: $(du -sh $PARTS|cut -f1)"

echo "### PASS 2 (distinct per bucket) $(date -u +%H:%M)"
for d in "$PARTS"/bk=*; do
  b=$(basename "$d" | cut -d= -f2)
  "$DB" -c "SET threads=2; SET memory_limit='20GB'; SET temp_directory='$TMP'; SET preserve_insertion_order=false;
  COPY (SELECT DISTINCT addr FROM read_parquet('$d/*.parquet')) TO '$OUT/addrs_$b.parquet' (FORMAT parquet);"
  echo "  bucket $b done $(date -u +%H:%M)"
done
echo "### ALL BUCKETS DONE $(date -u +%H:%M)"
rm -rf "$PARTS"
echo "### distinct account outputs:"; ls -lah "$OUT"/addrs_*.parquet
