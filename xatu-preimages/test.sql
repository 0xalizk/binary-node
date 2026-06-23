SET threads=2;
SET memory_limit='6GB';
SET temp_directory='/home/0xalizk/sharded-pir/binary-node/duckdb-tmp';
SELECT count(DISTINCT slot) AS distinct_slots
FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_storage_diffs/24*.parquet');
