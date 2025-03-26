use anyhow::Context as _;
use ethers::types::U256;
use zkstack_cli_common::{forge::ForgeScript, wallets::Wallet};

use crate::{
    consts::MINIMUM_BALANCE_FOR_WALLET,
    messages::{msg_address_doesnt_have_enough_money_prompt, msg_wallet_private_key_not_set},
};

pub enum WalletOwner {
    Governor,
    Deployer,
}

pub fn fill_forge_private_key(
    mut forge: ForgeScript,
    wallet: Option<&Wallet>,
    wallet_owner: WalletOwner,
) -> anyhow::Result<ForgeScript> {
    if !forge.wallet_args_passed() {
        forge = forge.with_private_key(
            wallet
                .and_then(|w| w.private_key_h256())
                .context(msg_wallet_private_key_not_set(wallet_owner))?,
        );
    }
    Ok(forge)
}

pub async fn check_the_balance(forge: &ForgeScript) -> anyhow::Result<()> {
    const MSG_CONTINUE: &str = "Proceed with the deployment anyway";
    const MSG_CHECK_BALANCE: &str = "Check the balance again";
    const MSG_EXIT: &str = "Exit";

    let Some(address) = forge.address() else {
        return Ok(());
    };

    let expected_balance = U256::from(MINIMUM_BALANCE_FOR_WALLET);
    while let Some(balance) = forge.get_the_balance().await? {
        if balance >= expected_balance {
            return Ok(());
        }

        let prompt_msg =
            msg_address_doesnt_have_enough_money_prompt(&address, balance, expected_balance);
        match zkstack_cli_common::PromptSelect::new(
            &prompt_msg,
            [MSG_CONTINUE, MSG_CHECK_BALANCE, MSG_EXIT],
        )
        .ask()
        {
            MSG_CONTINUE => return Ok(()),
            MSG_CHECK_BALANCE => continue,
            MSG_EXIT => anyhow::bail!("Exiting the deployment process"),
            _ => unreachable!(),
        }
    }
    Ok(())
}
