SET threads=2;
SET memory_limit='16GB';
SET temp_directory='/home/0xalizk/sharded-pir/binary-node/duckdb-tmp';
SET preserve_insertion_order=false;
-- distinct storage slot keys
COPY (
  SELECT DISTINCT slot
  FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_storage_diffs/*.parquet')
  WHERE slot IS NOT NULL
) TO '/home/0xalizk/sharded-pir/binary-node/xatu-preimages/distinct/slots.parquet' (FORMAT parquet);
-- distinct account addresses (union of every address-bearing column; superset is fine)
COPY (
  SELECT address AS a FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_storage_diffs/*.parquet') WHERE address IS NOT NULL
  UNION SELECT address FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_balance_diffs/*.parquet') WHERE address IS NOT NULL
  UNION SELECT address FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_nonce_diffs/*.parquet') WHERE address IS NOT NULL
  UNION SELECT contract_address FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_contracts/*.parquet') WHERE contract_address IS NOT NULL
  UNION SELECT deployer FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_contracts/*.parquet') WHERE deployer IS NOT NULL
  UNION SELECT factory FROM read_parquet('/home/0xalizk/sharded-pir/binary-node/xatu-preimages/xatu-raw/canonical_execution_contracts/*.parquet') WHERE factory IS NOT NULL
) TO '/home/0xalizk/sharded-pir/binary-node/xatu-preimages/distinct/addrs.parquet' (FORMAT parquet);
