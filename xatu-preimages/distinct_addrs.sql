SET threads=4;
SET memory_limit='24GB';
SET temp_directory='/home/0xalizk/sharded-pir/binary-node/duckdb-tmp';
SET preserve_insertion_order=false;
COPY (
  SELECT DISTINCT unhex(substr(CAST(a AS VARCHAR),3,40)) AS addr FROM (
    SELECT address AS a FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_storage_diffs/*.parquet') WHERE address IS NOT NULL
    UNION ALL SELECT address FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_balance_diffs/*.parquet') WHERE address IS NOT NULL
    UNION ALL SELECT address FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_nonce_diffs/*.parquet') WHERE address IS NOT NULL
    UNION ALL SELECT contract_address FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_contracts/*.parquet') WHERE contract_address IS NOT NULL
    UNION ALL SELECT deployer FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_contracts/*.parquet') WHERE deployer IS NOT NULL
    UNION ALL SELECT factory FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_contracts/*.parquet') WHERE factory IS NOT NULL
  )
) TO '/home/0xalizk/sharded-pir/binary-node/xatu-preimages/distinct/addrs.parquet' (FORMAT parquet);
