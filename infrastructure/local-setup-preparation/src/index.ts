import { utils } from 'zksync-ethers';
import { ethers } from 'ethers';
import { getEthersProvider, getWalletKeys, isOperator } from './utils';

// 10**12 ether
const AMOUNT_TO_DEPOSIT = ethers.utils.parseEther('1000000000000');
const CALLDATA = '0x';

async function depositWithRichAccounts() {
    const ethProvider = getEthersProvider();

    const chainId = process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID!;
    const wallets = getWalletKeys().map((wk) => new ethers.Wallet(wk.privateKey, ethProvider));
    const isEthBasedChain = process.env.CONTRACTS_BASE_TOKEN_ADDR === utils.ETH_ADDRESS_IN_CONTRACTS;

    const handles: Promise<any>[] = [];

    if (!process.env.CONTRACTS_BRIDGEHUB_PROXY_ADDR) {
        throw new Error('ZKsync L1 Main contract address was not found');
    }

    // During the preparation for the local node, the L2 server is not available, so
    // it is not possible to estimate the exact number of gas that is required for the transaction
    const DEPOSIT_L2_GAS_LIMIT = 10_000_000;
    const gasPrice = await ethProvider.getGasPrice();
    const contract = new ethers.Contract(process.env.CONTRACTS_BRIDGEHUB_PROXY_ADDR, utils.BRIDGEHUB_ABI, ethProvider);

    const expectedCost =
        (await contract.l2TransactionBaseCost(
            chainId,
            gasPrice,
            DEPOSIT_L2_GAS_LIMIT,
            utils.DEFAULT_GAS_PER_PUBDATA_LIMIT
        )) * 1.5;
    // We multiply the expected cost with 1.5 (in case gas price changes a little between calls - especially when we send 10 in a row).

    for (const wallet of wallets) {
        if (!(await isOperator(chainId, wallet.address))) {
            const contract = new ethers.Contract(
                process.env.CONTRACTS_BRIDGEHUB_PROXY_ADDR,
                utils.BRIDGEHUB_ABI,
                wallet
            );

            const overrides = {
                value: AMOUNT_TO_DEPOSIT.add(expectedCost)
            };

            if (!isEthBasedChain) {
                const baseTokenContract = new ethers.Contract(
                    process.env.CONTRACTS_BASE_TOKEN_ADDR,
                    utils.IERC20,
                    wallet
                );
                const sharedBridge = await contract.assetRouter();
                await (await baseTokenContract.approve(sharedBridge, ethers.constants.MaxUint256)).wait();
                const l1Erc20ABI = ['function mint(address to, uint256 amount)'];
                const l1Erc20Contract = new ethers.Contract(baseTokenContract.address, l1Erc20ABI, wallet);
                await (await l1Erc20Contract.mint(wallet.address, AMOUNT_TO_DEPOSIT.mul(2))).wait();
            }

            handles.push(
                // We have to implement the deposit manually because we run this script before running the server,
                // deposit method from wallet requires a running server
                contract.requestL2TransactionDirect(
                    {
                        chainId,
                        mintValue: overrides.value,
                        l2Contract: wallet.address,
                        l2Value: AMOUNT_TO_DEPOSIT,
                        l2Calldata: CALLDATA,
                        l2GasLimit: DEPOSIT_L2_GAS_LIMIT,
                        l2GasPerPubdataByteLimit: utils.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT,
                        factoryDeps: [],
                        refundRecipient: wallet.address
                    },
                    isEthBasedChain ? overrides : {}
                )
            );
        }
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
