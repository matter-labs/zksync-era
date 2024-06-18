use anyhow::anyhow;
use common::forge::ForgeScript;
use ethers::types::{H256, U256};

use crate::{
    consts::MINIMUM_BALANCE_FOR_WALLET,
    messages::{msg_address_doesnt_have_enough_money_prompt, MSG_DEPLOYER_PK_NOT_SET_ERR},
};

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

    let expected_balance = U256::from(MINIMUM_BALANCE_FOR_WALLET);
    while let Some(balance) = forge.get_the_balance().await? {
        if balance >= expected_balance {
            return Ok(());
        }
        if !common::PromptConfirm::new(msg_address_doesnt_have_enough_money_prompt(
            &address,
            balance,
            expected_balance,
        ))
        .ask()
        {
            break;
        }
    }
    Ok(())
}
