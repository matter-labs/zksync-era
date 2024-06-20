use std::path::Path;

use config::RocksDbs;
use xshell::Shell;

use crate::defaults::{
    EN_ROCKS_DB_PREFIX, MAIN_ROCKS_DB_PREFIX, ROCKS_DB_STATE_KEEPER, ROCKS_DB_TREE,
};

pub enum RocksDBDirOption {
    Main,
    ExternalNode,
}

impl RocksDBDirOption {
    pub fn prefix(&self) -> &str {
        match self {
            RocksDBDirOption::Main => MAIN_ROCKS_DB_PREFIX,
            RocksDBDirOption::ExternalNode => EN_ROCKS_DB_PREFIX,
        }
    }
}

pub fn recreate_rocksdb_dirs(
    shell: &Shell,
    rocks_db_path: &Path,
    option: RocksDBDirOption,
) -> anyhow::Result<RocksDbs> {
    let state_keeper = rocks_db_path
        .join(option.prefix())
        .join(ROCKS_DB_STATE_KEEPER);
    shell.remove_path(&state_keeper)?;
    let merkle_tree = rocks_db_path.join(option.prefix()).join(ROCKS_DB_TREE);
    shell.remove_path(&merkle_tree)?;
    Ok(RocksDbs {
        state_keeper: shell.create_dir(state_keeper)?,
        merkle_tree: shell.create_dir(merkle_tree)?,
    })
}
