import fs from 'fs';
import path from 'path';
import { ethers } from 'ethers';
import { getEthersProvider, getWalletKeys } from './utils';
import assert from 'assert';

async function joinHyperchain() {
    const chainId = process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID!;
    assert(chainId != '270', `Your chain id is set to 270 - are you sure`);

    const ethProvider = getEthersProvider();
    const operatorWallet = ethers.Wallet.createRandom();
    console.log(`Operator: ${operatorWallet.address}`);
    const blobOperatorWallet = ethers.Wallet.createRandom();
    console.log(`Blob Operator: ${blobOperatorWallet.address}`);

    const allWalletKeys = getWalletKeys();

    const richWalletKey = allWalletKeys[allWalletKeys.length - 1];

    const richAccount = new ethers.Wallet(richWalletKey.privateKey, ethProvider);

    for (const addr of [operatorWallet.address, blobOperatorWallet.address]) {
        const tx = {
            to: addr,
            // Convert Ether to Wei
            value: ethers.utils.parseEther('100')
        };
        await richAccount.sendTransaction(tx);
    }
    console.log(`Eth sent to operator accounts`);

    const inputFilePath = process.env.MASTER_ENV_FILE!;
    const outputFilePath = path.resolve('/etc/env/l1-inits', '.init.env');
    const varsToKeep = [
        'CONTRACTS_BRIDGEHUB_PROXY_ADDR',
        'CONTRACTS_STATE_TRANSITION_PROXY_ADDR',
        'CONTRACTS_L1_SHARED_BRIDGE_PROXY_ADDR',
        'CONTRACTS_ADMIN_FACET_ADDR',
        'CONTRACTS_MAILBOX_FACET_ADDR',
        'CONTRACTS_EXECUTOR_FACET_ADDR',
        'CONTRACTS_GETTERS_FACET_ADDR',
        'CONTRACTS_DIAMOND_INIT_ADDR',
        'CONTRACTS_VERIFIER_ADDR',
        'CONTRACTS_L1_MULTICALL3_ADDR',
        'CONTRACTS_VALIDATOR_TIMELOCK_ADDR',
        'CONTRACTS_GOVERNANCE_ADDR'
    ];

    try {
        const data = fs.readFileSync(inputFilePath, 'utf8');

        const lines = data.split(/\r?\n/);
        // Filter and map lines to keep only specified variables
        const filteredEnvVars = lines.filter((line) => {
            const key = line.split('=')[0];
            return varsToKeep.includes(key);
        });

        filteredEnvVars.push(`ETH_SENDER_SENDER_OPERATOR_PRIVATE_KEY=${operatorWallet.privateKey}`);
        filteredEnvVars.push(`ETH_SENDER_SENDER_OPERATOR_COMMIT_ETH_ADDR=${operatorWallet.address}`);
        filteredEnvVars.push(`ETH_SENDER_SENDER_OPERATOR_BLOBS_PRIVATE_KEY=${blobOperatorWallet.privateKey}`);
        filteredEnvVars.push(`ETH_SENDER_SENDER_OPERATOR_BLOBS_ETH_ADDR=${blobOperatorWallet.address}`);

        // Prepare the content to write to the output file
        const outputContent = filteredEnvVars.join('\n');

        // Write the filtered environment variables to the output file
        fs.writeFileSync(outputFilePath, outputContent, 'utf8');
        console.log('Filtered environment variables have been written to the output file.');
    } catch (error) {
        console.error('Failed to process environment variables:', error);
    }
}

async function main() {
    await joinHyperchain();
}

main()
    .then(() => {
        console.log('Successfully joined hyperchain!');
    })
    .catch((e) => {
        console.log(`Execution failed with error ${e}`);
    });
