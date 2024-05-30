use crate::{
    consts::MINIMUM_BALANCE_FOR_WALLET,
    messages::{msg_address_doesnt_have_enough_money_prompt, MSG_DEPLOYER_PK_NOT_SET_ERR},
};
use anyhow::anyhow;
use common::forge::ForgeScript;
use ethers::types::H256;

pub fn fill_forge_private_key(
    mut forge: ForgeScript,
    private_key: Option<H256>,
) -> anyhow::Result<ForgeScript> {
    if !forge.wallet_args_passed() {
        forge = forge.with_private_key(private_key.ok_or(anyhow!(MSG_DEPLOYER_PK_NOT_SET_ERR))?);
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
        if common::PromptConfirm::new(msg_address_doesnt_have_enough_money_prompt(&address)).ask() {
            break;
        }
    }
    Ok(())
}
