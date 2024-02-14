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

    // During the preparation for the local node, the L2 server is not available, so
    // it is not possible to estimate the exact number of gas that is required for the transaction
    const DEPOSIT_L2_GAS_LIMIT = 10_000_000;
    const gasPrice = await ethProvider.getGasPrice();
    const contract = new ethers.Contract(process.env.CONTRACTS_DIAMOND_PROXY_ADDR, utils.ZKSYNC_MAIN_ABI, ethProvider);

    const expectedCost = await contract.l2TransactionBaseCost(
        gasPrice,
        DEPOSIT_L2_GAS_LIMIT,
        utils.DEFAULT_GAS_PER_PUBDATA_LIMIT
    );

    for (const wallet of wallets) {
        const contract = new ethers.Contract(process.env.CONTRACTS_DIAMOND_PROXY_ADDR, utils.ZKSYNC_MAIN_ABI, wallet);

        const overrides = {
            value: AMOUNT_TO_DEPOSIT.add(expectedCost)
        };

        const balance = await wallet.getBalance();
        console.log(`Wallet ${wallet.address} balance is ${ethers.utils.formatEther(balance)} ETH`);

        // TODO: Currently we're providing zero as an operator fee, which works right now,
        // but will be changed in the future.
        handles.push(
            // We have to implement the deposit manually because we run this script before running the server,
            // deposit method from wallet requires a running server
            contract.requestL2Transaction(
                wallet.address,
                AMOUNT_TO_DEPOSIT,
                overrides.value,
                '0x',
                DEPOSIT_L2_GAS_LIMIT,
                utils.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT,
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
