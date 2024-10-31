import { Command } from 'commander';
import * as utils from 'utils';
// import * as env from './env';
// import fs from 'fs';

import { getDeployAccounts, getTestAccounts } from './run';

import { ethers } from 'ethers';
import { Wallet, Provider, utils as zkUtils } from 'zksync-ethers';
// import { spawn } from 'child_process';

export const command = new Command('dev2').description('Management of an L2 network on top of another L2');

// Deposits from rich wallets to the current chain
async function supplyRichWallets() {
    // Note that we explicitly do not use `isCurrentNetworkLocal` function here, since this method is
    // intended to be used only on L1 base chain.
    if (!utils.isNetworkLocalL1(process.env.CHAIN_ETH_NETWORK!)) {
        throw new Error('This command is only available for localhost');
    }

    console.log('Depositing funds from rich accounts to the network!');

    const l1Provider = new ethers.providers.JsonRpcProvider(process.env.ETH_CLIENT_WEB3_URL);
    const l2Provider = new Provider(process.env.API_WEB3_JSON_RPC_HTTP_URL);

    const richAccounts = [...(await getTestAccounts()), ...(await getDeployAccounts())];
    for (const account of richAccounts) {
        const { privateKey } = account;
        const wallet = new Wallet(privateKey, l2Provider, l1Provider);

        if (
            privateKey == process.env.ETH_SENDER_SENDER_OPERATOR_PRIVATE_KEY ||
            privateKey == process.env.ETH_SENDER_SENDER_OPERATOR_BLOBS_PRIVATE_KEY
        ) {
            console.log(`Skipping rich wallet ${wallet.address} as it is an operator wallet`);
            continue;
        }

        console.log('Depositing to wallet ', wallet.address);

        // For now, we only deposit ETH and only deal with ETH-based chains.
        await (
            await wallet.deposit({
                token: zkUtils.ETH_ADDRESS_IN_CONTRACTS,
                amount: ethers.utils.parseEther('100000')
            })
        ).wait();
        console.log('Done');
    }

    console.log('Deposits completed!');
}

command
    .command('prepare-env')
    .allowUnknownOption(true)
    .description('switch to and compile the dev2 config')
    .action(async () => {
        await utils.spawn('zk config compile dev2 --diff 1');
    });

command
    .command('supply-rich-wallets')
    .allowUnknownOption(true)
    .description('deposit from rich wallets to the current active chain')
    .action(supplyRichWallets);

// command
//     .command('prepare-to-be-sync-layer')
//     .allowUnknownOption(true)
//     .description('deposit from rich wallets to the current active chain')
//     .action(async () => {
//         const currentRpc = process.env.API_WEB3_JSON_RPC_HTTP_URL;
//         // for this script, we will use the l2 rpc
//         // process.env.ETH_CLIENT_WEB3_URL = currentRpc;
//         // process.env.BASE
//         await utils.spawn('yarn l1-contracts prepare-sync-layer');
//     });;
