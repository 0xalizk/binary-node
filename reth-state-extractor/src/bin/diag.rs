//! Diagnostic: open a reth datadir read-only and print row counts for the
//! candidate state tables, to find where plain account/storage state lives.
//! Usage: diag <reth_datadir>

use reth_db::{open_db_read_only, tables, Database};
use reth_db_api::transaction::DbTx;
use std::path::PathBuf;

macro_rules! count {
    ($tx:expr, $t:ty) => {{
        match $tx.entries::<$t>() {
            Ok(n) => println!("{:<28} {}", stringify!($t), n),
            Err(e) => println!("{:<28} ERR: {e}", stringify!($t)),
        }
    }};
}

fn main() -> eyre::Result<()> {
    let datadir = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or_else(|| eyre::eyre!("usage: diag <reth_datadir>"))?,
    );
    let db = open_db_read_only(&datadir.join("db"), Default::default())
        .map_err(|e| eyre::eyre!("open: {e}"))?;
    let tx = db.tx().map_err(|e| eyre::eyre!("tx: {e}"))?;

    println!("{:<28} {}", "TABLE", "ENTRIES");
    count!(tx, tables::PlainAccountState);
    count!(tx, tables::PlainStorageState);
    count!(tx, tables::Bytecodes);
    count!(tx, tables::HashedAccounts);
    count!(tx, tables::HashedStorages);
    count!(tx, tables::AccountsHistory);
    count!(tx, tables::StoragesHistory);
    count!(tx, tables::AccountChangeSets);
    count!(tx, tables::StorageChangeSets);
    count!(tx, tables::CanonicalHeaders);
    count!(tx, tables::Headers);
    Ok(())
}
