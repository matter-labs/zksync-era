use std::path::{Path, PathBuf};

use ethers::core::rand::thread_rng;
use xshell::Shell;

use crate::{
    configs::{ReadConfig, SaveConfig, WalletsConfig},
    consts::{CONFIGS_PATH, WALLETS_FILE},
    wallets::WalletCreation,
};

pub fn create_wallets(
    shell: &Shell,
    dst_wallet_path: &Path,
    link_to_code: &Path,
    wallet_creation: WalletCreation,
    initial_wallet_path: Option<PathBuf>,
) -> anyhow::Result<()> {
    let wallets = match wallet_creation {
        WalletCreation::Random => {
            let rng = &mut thread_rng();
            WalletsConfig::random(rng)
        }
        WalletCreation::Empty => WalletsConfig::empty(),
        WalletCreation::Localhost => {
            let path = link_to_code.join(CONFIGS_PATH).join(WALLETS_FILE);
            WalletsConfig::read(path)?
        }
        WalletCreation::InFile => {
            let path = initial_wallet_path.ok_or(anyhow::anyhow!(
                "Wallet path for in file option is required"
            ))?;
            WalletsConfig::read(path)?
        }
    };

    wallets.save(shell, dst_wallet_path)?;
    Ok(())
}
