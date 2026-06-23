SET threads=1;
SET memory_limit='20GB';
SET temp_directory='/home/0xalizk/sharded-pir/binary-node/duckdb-tmp';
SET preserve_insertion_order=false;
COPY (
  SELECT DISTINCT unhex(substr(CAST(slot AS VARCHAR),3,64)) AS slot
  FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_storage_diffs/*.parquet')
  WHERE slot IS NOT NULL
) TO '/home/0xalizk/sharded-pir/binary-node/xatu-preimages/distinct/slots.parquet' (FORMAT parquet);
