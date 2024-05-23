use crate::consts::MINIMUM_BALANCE_FOR_WALLET;
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

pub async fn check_the_balance(forge: &ForgeScript) -> anyhow::Result<()> {
    let Some(address) = forge.address() else {
        return Ok(());
    };

    while !forge
        .check_the_balance(MINIMUM_BALANCE_FOR_WALLET.into())
        .await?
    {
        if common::PromptConfirm::new(format!("Address {address:?} doesn't have enough money to deploy contracts do you want to continue?")).ask() {
                break;
            }
    }
    Ok(())
}
