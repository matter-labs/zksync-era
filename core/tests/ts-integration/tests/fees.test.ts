/**
 * This suite contains tests displaying prices for some of the most common operations under various L1 gas prices.
 *
 * IMPORTANT: this test affects the internal state of the server and so
 * it should never be run in parallel with other tests.
 *
 * Locally, for maximal representation the test should be run with batches of size 1.
 * However, we do not want to overload the CI for such purposes and so the job of the CI would be to make
 * sure that the test is maintained does not get broken.
 *
 */
import * as utils from 'zk/build/utils';
import { TestMaster } from '../src/index';

import * as zksync from 'zksync-web3';
import { BigNumber, ethers } from 'ethers';
import { Token } from '../src/types';

// Unless `RUN_FEE_TEST` is provided, skip the test suit
const testFees = process.env.RUN_FEE_TEST ? describe : describe.skip;

// The L1 gas prices under which the test will be conducted.
// For CI we use only 2 gas prices to not slow it down too much.
const L1_GAS_PRICES_TO_TEST = process.env.CI
    ? [
          5_000_000_000, // 5 gwei
          10_000_000_000 // 10 gwei
      ]
    : [
          1_000_000_000, // 1 gwei
          5_000_000_000, // 5 gwei
          10_000_000_000, // 10 gwei
          25_000_000_000, // 25 gwei
          50_000_000_000, // 50 gwei
          100_000_000_000, // 100 gwei
          200_000_000_000, // 200 gwei
          400_000_000_000, // 400 gwei
          800_000_000_000, // 800 gwei
          1_000_000_000_000, // 1000 gwei
          2_000_000_000_000 // 2000 gwei
      ];

testFees('Test fees', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;

    let tokenDetails: Token;
    let aliceErc20: zksync.Contract;

    beforeAll(() => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();

        tokenDetails = testMaster.environment().erc20Token;
        aliceErc20 = new ethers.Contract(tokenDetails.l1Address, zksync.utils.IERC20, alice.ethWallet());
    });

    test('Test fees', async () => {
        const receiver = ethers.Wallet.createRandom().address;

        // Getting ETH price in gas.
        const feeTestL1Receipt = await (
            await alice.ethWallet().sendTransaction({
                to: receiver,
                value: BigNumber.from(1)
            })
        ).wait();

        const feeTestL1ReceiptERC20 = await (
            await alice.ethWallet().sendTransaction({
                to: aliceErc20.address,
                data: aliceErc20.interface.encodeFunctionData('transfer', [receiver, BigNumber.from(1)])
            })
        ).wait();

        let reports = ['ETH transfer:\n\n', 'ERC20 transfer:\n\n'];
        for (const gasPrice of L1_GAS_PRICES_TO_TEST) {
            reports = await appendResults(
                alice,
                [feeTestL1Receipt, feeTestL1ReceiptERC20],
                // We always regenerate new addresses for transaction requests in order to estimate the cost for a new account
                [
                    {
                        to: ethers.Wallet.createRandom().address,
                        value: BigNumber.from(1)
                    },
                    {
                        data: aliceErc20.interface.encodeFunctionData('transfer', [
                            ethers.Wallet.createRandom().address,
                            BigNumber.from(1)
                        ]),
                        to: tokenDetails.l2Address
                    }
                ],
                gasPrice,
                reports
            );
        }

        await setInternalL1GasPrice(alice._providerL2(), undefined, true);

        console.log(`Full report: \n\n${reports.join('\n\n')}`);
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});

async function appendResults(
    sender: zksync.Wallet,
    originalL1Receipts: ethers.providers.TransactionReceipt[],
    transactionRequests: ethers.providers.TransactionRequest[],
    newL1GasPrice: number,
    reports: string[]
): Promise<string[]> {
    await setInternalL1GasPrice(sender._providerL2(), newL1GasPrice.toString());

    if (originalL1Receipts.length !== reports.length && originalL1Receipts.length !== transactionRequests.length) {
        throw new Error('The array of receipts and reports have different length');
    }

    const results = [];

    for (let i = 0; i < originalL1Receipts.length; i++) {
        const receipt = originalL1Receipts[i];
        const request = transactionRequests[i];
        const oldReport = reports[i];

        results.push(await updateReport(sender, receipt, request, newL1GasPrice, oldReport));
    }

    return results;
}

async function updateReport(
    sender: zksync.Wallet,
    l1Receipt: ethers.providers.TransactionReceipt,
    transactionRequest: ethers.providers.TransactionRequest,
    newL1GasPrice: number,
    oldReport: string
): Promise<string> {
    const expectedL1Price = +ethers.utils.formatEther(l1Receipt.gasUsed.mul(newL1GasPrice));

    const estimatedL2GasPrice = await sender.getGasPrice();
    const estimatedL2GasLimit = await sender.estimateGas(transactionRequest);
    const estimatedPrice = estimatedL2GasPrice.mul(estimatedL2GasLimit);

    const balanceBefore = await sender.getBalance();
    await (await sender.sendTransaction(transactionRequest)).wait();
    const balanceAfter = await sender.getBalance();
    const balanceDiff = balanceBefore.sub(balanceAfter);

    const l2PriceAsNumber = +ethers.utils.formatEther(balanceDiff);
    const l2EstimatedPriceAsNumber = +ethers.utils.formatEther(estimatedPrice);

    const gasReport = `Gas price ${newL1GasPrice / 1000000000} gwei:
    L1 cost ${expectedL1Price}, 
    L2 estimated cost: ${l2EstimatedPriceAsNumber}
    Estimated Gain: ${expectedL1Price / l2EstimatedPriceAsNumber}
    L2 cost: ${l2PriceAsNumber}, 
    Gain: ${expectedL1Price / l2PriceAsNumber}\n`;
    console.log(gasReport);

    return oldReport + gasReport;
}

async function killServerAndWaitForShutdown(provider: zksync.Provider) {
    await utils.exec('pkill zksync_server');
    // Wait until it's really stopped.
    let iter = 0;
    while (iter < 30) {
        try {
            await provider.getBlockNumber();
            await utils.sleep(5);
            iter += 1;
        } catch (_) {
            // When exception happens, we assume that server died.
            return;
        }
    }
    // It's going to panic anyway, since the server is a singleton entity, so better to exit early.
    throw new Error("Server didn't stop after a kill request");
}

async function setInternalL1GasPrice(provider: zksync.Provider, newPrice?: string, disconnect?: boolean) {
    // Make sure server isn't running.
    try {
        await killServerAndWaitForShutdown(provider);
    } catch (_) {}

    // Run server in background.
    let command = 'zk server --components api,tree,eth,data_fetcher,state_keeper';
    command = `DATABASE_MERKLE_TREE_MODE=full ${command}`;
    if (newPrice) {
        command = `ETH_SENDER_GAS_ADJUSTER_INTERNAL_ENFORCED_L1_GAS_PRICE=${newPrice} ${command}`;
    }
    const zkSyncServer = utils.background(command, 'ignore');

    if (disconnect) {
        zkSyncServer.unref();
    }

    // Server may need some time to recompile if it's a cold run, so wait for it.
    let iter = 0;
    let mainContract;
    while (iter < 30 && !mainContract) {
        try {
            mainContract = await provider.getMainContractAddress();
        } catch (_) {
            await utils.sleep(5);
            iter += 1;
        }
    }
    if (!mainContract) {
        throw new Error('Server did not start');
    }
}
