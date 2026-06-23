#!/bin/bash
# Hash-partitioned distinct of Xatu storage slot keys (bounded memory, no spill reliance).
set -e
cd /home/0xalizk/sharded-pir/binary-node
R=/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_storage_diffs
TMP=/home/0xalizk/sharded-pir/binary-node/duckdb-tmp
PARTS=/home/0xalizk/sharded-pir/binary-node/xatu-preimages/parts_slots
OUT=/home/0xalizk/sharded-pir/binary-node/xatu-preimages/distinct
DB=./duckdb
rm -rf "$PARTS"; mkdir -p "$PARTS" "$OUT"; rm -f "$OUT"/slots_*.parquet

echo "### PASS 1 (partition raw slots -> 16 buckets) $(date -u +%H:%M)"
"$DB" -c "SET threads=2; SET memory_limit='12GB'; SET temp_directory='$TMP'; SET preserve_insertion_order=false;
COPY (SELECT unhex(substr(CAST(slot AS VARCHAR),3,64)) AS slot,
             substr(CAST(slot AS VARCHAR),3,1) AS b
      FROM read_parquet('$R/*.parquet') WHERE slot IS NOT NULL)
TO '$PARTS' (FORMAT parquet, PARTITION_BY (b), OVERWRITE_OR_IGNORE);"
echo "### PASS 1 done $(date -u +%H:%M); parts size: $(du -sh $PARTS|cut -f1)"

echo "### PASS 2 (distinct per bucket) $(date -u +%H:%M)"
for d in "$PARTS"/b=*; do
  b=$(basename "$d" | cut -d= -f2)
  "$DB" -c "SET threads=2; SET memory_limit='20GB'; SET temp_directory='$TMP'; SET preserve_insertion_order=false;
  COPY (SELECT DISTINCT slot FROM read_parquet('$d/*.parquet')) TO '$OUT/slots_$b.parquet' (FORMAT parquet);"
  echo "  bucket $b done $(date -u +%H:%M)"
done
echo "### ALL BUCKETS DONE $(date -u +%H:%M)"
rm -rf "$PARTS"   # reclaim the raw partitioned intermediate
echo "### cleaned parts; distinct outputs:"; ls -lah "$OUT"/slots_*.parquet
