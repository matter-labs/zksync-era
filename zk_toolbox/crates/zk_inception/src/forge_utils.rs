use anyhow::anyhow;
use common::forge::ForgeScript;
use ethers::types::H256;

pub fn fill_forge_private_key(
    mut forge: ForgeScript,
    private_key: Option<H256>,
) -> anyhow::Result<ForgeScript> {
    if !forge.wallet_args_passed() {
        forge =
            forge.with_private_key(private_key.ok_or(anyhow!("Deployer private key is not set"))?);
    }
    Ok(forge)
}
