import { utils } from 'zksync-web3';
import { ethers } from 'ethers';
import { getEthersProvider, getWalletKeys } from './utils';

// 10**12 ether
const AMOUNT_TO_DEPOSIT = ethers.utils.parseEther('1000000000000');

async function depositWithRichAccounts() {
    const ethProvider = getEthersProvider();
    const wallets = getWalletKeys().map((wk) => new ethers.Wallet(wk.privateKey, ethProvider));

    const handles: Promise<any>[] = [];

    if (!process.env.CONTRACTS_DIAMOND_PROXY_ADDR) {
        throw new Error('zkSync L1 Main contract address was not found');
    }

    for (const wallet of wallets) {
        const contract = new ethers.Contract(process.env.CONTRACTS_DIAMOND_PROXY_ADDR, utils.ZKSYNC_MAIN_ABI, wallet);

        const overrides = {
            value: AMOUNT_TO_DEPOSIT
        };

        const balance = await wallet.getBalance();
        console.log(`Wallet balance is ${ethers.utils.formatEther(balance)} ETH`);

        handles.push(
            // We have to implement the deposit manually because we run this script before running the server,
            // deposit method from wallet requires a running server
            contract.requestL2Transaction(
                wallet.address,
                AMOUNT_TO_DEPOSIT,
                '0x',
                utils.RECOMMENDED_DEPOSIT_L2_GAS_LIMIT,
                utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
                [],
                wallet.address,
                overrides
            )
        );
    }

    const depositHandles = (await Promise.all(handles)).map((h) => h.wait());
    await Promise.all(depositHandles);
}

async function main() {
    await depositWithRichAccounts();
}

main()
    .then(() => {
        console.log('Successfully deposited funds for the rich accounts!');
    })
    .catch((e) => {
        console.log(`Execution failed with error ${e}`);
    });
