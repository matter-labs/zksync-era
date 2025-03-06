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
import fs from 'node:fs/promises';
import { TestContextOwner, TestMaster } from '../src';

// import * as zksync from 'zksync-ethers';
import * as zksync from 'zksync-ethers-interop-support';
import * as ethers from 'ethers';
import { DataAvailabityMode, Token } from '../src/types';
import { SYSTEM_CONTEXT_ADDRESS, getTestContract, waitForNewL1Batch, anyTransaction } from '../src/helpers';
import { loadConfig, shouldLoadConfigFromFile } from 'utils/build/file-configs';
import { logsTestPath } from 'utils/build/logs';
import { sleep } from 'utils/build';
import { killPidWithAllChilds } from 'utils/build/kill';
import path from 'path';
import { NodeSpawner } from 'utils/src/node-spawner';
import { sendTransfers } from '../src/context-owner';
import { Reporter } from '../src/reporter';

declare global {
    var __ZKSYNC_TEST_CONTEXT_OWNER__: TestContextOwner;
}

const UINT32_MAX = 2n ** 32n - 1n;
const MAX_GAS_PER_PUBDATA = 50_000n;

// The L1 gas prices under which the test will be conducted.
// For CI we use only 2 gas prices to not slow it down too much.
const L1_GAS_PRICES_TO_TEST = process.env.CI
    ? [
          5_000_000_000n, // 5 gwei
          10_000_000_000n // 10 gwei
      ]
    : [
          1_000_000_000n, // 1 gwei
          5_000_000_000n, // 5 gwei
          10_000_000_000n, // 10 gwei
          25_000_000_000n, // 25 gwei
          50_000_000_000n, // 50 gwei
          100_000_000_000n, // 100 gwei
          200_000_000_000n, // 200 gwei
          400_000_000_000n, // 400 gwei
          800_000_000_000n, // 800 gwei
          1_000_000_000_000n, // 1000 gwei
          2_000_000_000_000n // 2000 gwei
      ];

// Unless `RUN_FEE_TEST` is provided, skip the test suit
const testFees = process.env.RUN_FEE_TEST ? describe : describe.skip;

testFees('Test fees', function () {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;

    let tokenDetails: Token;
    let aliceErc20: zksync.Contract;
    let isETHBasedChain: boolean;

    let mainLogs: fs.FileHandle;
    let baseTokenAddress: string;
    let ethClientWeb3Url: string;
    let apiWeb3JsonRpcHttpUrl: string;
    let mainNodeSpawner: NodeSpawner;

    const fileConfig = shouldLoadConfigFromFile();
    const pathToHome = path.join(__dirname, '../../../..');
    const enableConsensus = process.env.ENABLE_CONSENSUS == 'true';

    async function logsPath(chain: string | undefined, name: string): Promise<string> {
        chain = chain ? chain : 'default';
        return await logsTestPath(chain, 'logs/server/fees', name);
    }

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        let l2Node = testMaster.environment().l2NodePid;
        if (l2Node !== undefined) {
            await killPidWithAllChilds(l2Node, 9);
        }

        if (!fileConfig.loadFromFile) {
            ethClientWeb3Url = process.env.ETH_CLIENT_WEB3_URL!;
            apiWeb3JsonRpcHttpUrl = process.env.API_WEB3_JSON_RPC_HTTP_URL!;
            baseTokenAddress = process.env.CONTRACTS_BASE_TOKEN_ADDR!;
        } else {
            const generalConfig = loadConfig({
                pathToHome,
                chain: fileConfig.chain,
                config: 'general.yaml'
            });
            const secretsConfig = loadConfig({
                pathToHome,
                chain: fileConfig.chain,
                config: 'secrets.yaml'
            });
            const contractsConfig = loadConfig({
                pathToHome,
                chain: fileConfig.chain,
                config: 'contracts.yaml'
            });

            ethClientWeb3Url = secretsConfig.l1.l1_rpc_url;
            apiWeb3JsonRpcHttpUrl = generalConfig.api.web3_json_rpc.http_url;
            baseTokenAddress = contractsConfig.l1.base_token_addr;
        }

        const pathToMainLogs = await logsPath(fileConfig.chain, 'server.log');
        mainLogs = await fs.open(pathToMainLogs, 'a');
        console.log(`Writing server logs to ${pathToMainLogs}`);

        mainNodeSpawner = new NodeSpawner(pathToHome, mainLogs, fileConfig, {
            enableConsensus,
            ethClientWeb3Url,
            apiWeb3JsonRpcHttpUrl,
            baseTokenAddress
        });

        await mainNodeSpawner.killAndSpawnMainNode();

        alice = testMaster.mainAccount();
        tokenDetails = testMaster.environment().erc20Token;
        aliceErc20 = new ethers.Contract(tokenDetails.l1Address, zksync.utils.IERC20, alice.ethWallet());

        const mainWallet = new zksync.Wallet(
            testMaster.environment().mainWalletPK,
            alice._providerL2(),
            alice._providerL1()
        );

        isETHBasedChain = baseTokenAddress == zksync.utils.ETH_ADDRESS_IN_CONTRACTS;

        // On non ETH based chains the standard deposit is not enough to run all this tests
        if (!isETHBasedChain) {
            const depositTx = await mainWallet.deposit({
                token: baseTokenAddress,
                amount: ethers.parseEther('100'),
                approveERC20: true,
                approveBaseERC20: true
            });
            await depositTx.wait();
            await Promise.all(
                await sendTransfers(
                    zksync.utils.ETH_ADDRESS,
                    mainWallet,
                    { alice: alice.privateKey },
                    ethers.parseEther('100'),
                    undefined,
                    undefined,
                    new Reporter()
                )
            );
        }
    });

    test('Test all fees', async () => {
        const receiver = ethers.Wallet.createRandom().address;

        // Getting ETH price in gas.
        const feeTestL1Receipt = await (
            await alice.ethWallet().sendTransaction({
                to: receiver,
                value: 1n
            })
        ).wait();

        if (feeTestL1Receipt === null) {
            throw new Error('Failed to send ETH transaction');
        }

        const feeTestL1ReceiptERC20 = await (
            await alice.ethWallet().sendTransaction({
                to: aliceErc20.getAddress(),
                data: aliceErc20.interface.encodeFunctionData('transfer', [receiver, 1n])
            })
        ).wait();

        if (feeTestL1ReceiptERC20 === null) {
            throw new Error('Failed to send ERC20 transaction');
        }

        // Warming up slots for the receiver
        await (
            await alice.sendTransaction({
                to: receiver,
                value: BigInt(1),
                type: 2
            })
        ).wait();

        await (
            await alice.sendTransaction({
                data: aliceErc20.interface.encodeFunctionData('transfer', [receiver, 1n]),
                to: tokenDetails.l2Address,
                type: 2
            })
        ).wait();

        let reports = [
            'ETH transfer (to new):\n\n',
            'ETH transfer (to old):\n\n',
            'ERC20 transfer (to new):\n\n',
            'ERC20 transfer (to old):\n\n'
        ];
        for (const gasPrice of L1_GAS_PRICES_TO_TEST) {
            // For the sake of simplicity, we'll use the same pubdata price as the L1 gas price.
            await mainNodeSpawner.killAndSpawnMainNode({
                newL1GasPrice: gasPrice,
                newPubdataPrice: gasPrice
            });

            reports = await appendResults(
                alice,
                [feeTestL1Receipt, feeTestL1Receipt, feeTestL1ReceiptERC20, feeTestL1ReceiptERC20],
                // We always regenerate new addresses for transaction requests in order to estimate the cost for a new account
                [
                    {
                        to: ethers.Wallet.createRandom().address,
                        value: 1n,
                        type: 2
                    },
                    {
                        to: receiver,
                        value: 1n,
                        type: 2
                    },
                    {
                        data: aliceErc20.interface.encodeFunctionData('transfer', [
                            ethers.Wallet.createRandom().address,
                            1n
                        ]),
                        to: tokenDetails.l2Address,
                        type: 2
                    },
                    {
                        data: aliceErc20.interface.encodeFunctionData('transfer', [receiver, 1n]),
                        to: tokenDetails.l2Address,
                        type: 2
                    }
                ],
                gasPrice,
                reports
            );
        }

        console.log(`Full report: \n\n${reports.join('\n\n')}`);
    });

    test('Test gas price expected value', async () => {
        const l1GasPrice = 2_000_000_000n; /// set to 2 gwei
        await mainNodeSpawner.killAndSpawnMainNode({
            newL1GasPrice: l1GasPrice,
            newPubdataPrice: l1GasPrice
        });

        // wait for new batch so gas price is updated with new config set above
        await waitForNewL1Batch(alice);

        const receipt = await anyTransaction(alice);

        const feeParams = await alice._providerL2().getFeeParams();
        const feeConfig = feeParams.V2.config;
        // type is missing conversion_ratio field
        const conversionRatio: { numerator: bigint; denominator: bigint } = (feeParams.V2 as any)['conversion_ratio'];
        if (isETHBasedChain) {
            expect(conversionRatio.numerator).toBe(1); //number not bigint for some reason
            expect(conversionRatio.denominator).toBe(1);
        } else {
            expect(conversionRatio.numerator).toBeGreaterThan(1n);
        }

        // the minimum + compute overhead of 0.01gwei in validium mode
        const expectedETHGasPrice =
            feeConfig.minimal_l2_gas_price +
            (feeConfig.compute_overhead_part * feeParams.V2.l1_gas_price * feeConfig.batch_overhead_l1_gas) /
                feeConfig.max_gas_per_batch;
        const expectedConvertedGasPrice =
            (expectedETHGasPrice * conversionRatio.numerator) / conversionRatio.denominator;

        expect(receipt.gasPrice).toBe(BigInt(expectedConvertedGasPrice));
    });

    test('Test base token ratio fluctuations', async () => {
        const l1GasPrice = 2_000_000_000n; /// set to 2 gwei

        if (isETHBasedChain) return;

        await mainNodeSpawner.killAndSpawnMainNode({
            newL1GasPrice: l1GasPrice,
            newPubdataPrice: l1GasPrice,
            externalPriceApiClientForcedNumerator: 300,
            externalPriceApiClientForcedDenominator: 100,
            externalPriceApiClientForcedFluctuation: 20,
            baseTokenPricePollingIntervalMs: 1000,
            baseTokenAdjusterL1UpdateDeviationPercentage: 0
        });

        const beginFeeParams = await alice._providerL2().getFeeParams();
        const mainContract = await alice.getMainContract();
        const beginL1Nominator = await mainContract.baseTokenGasPriceMultiplierNominator();
        let changedL2 = false;
        let changedL1 = false;
        for (let i = 0; i < 20; i++) {
            await sleep(0.5);
            const newFeeParams = await alice._providerL2().getFeeParams();
            // we need any as FeeParams is missing existing conversion_ratio field

            if (
                ((newFeeParams.V2 as any)['conversion_ratio'].numerator as number) !=
                ((beginFeeParams.V2 as any)['conversion_ratio'].numerator as number)
            ) {
                // @ts-ignore
                const diff =
                    (newFeeParams.V2 as any)['conversion_ratio'].numerator -
                    (beginFeeParams.V2 as any)['conversion_ratio'].numerator;
                // Deviation is 20%, Adding 5% extra for any arithmetic precision issues, 25%*300 = 75
                expect(diff).toBeLessThan(75);
                expect(diff).toBeGreaterThan(-75);
                changedL2 = true;
                break;
            }
        }
        expect(changedL2).toBeTruthy();
        for (let i = 0; i < 10; i++) {
            const newL1Nominator = await mainContract.baseTokenGasPriceMultiplierNominator();
            if (newL1Nominator != beginL1Nominator) {
                const diff = newL1Nominator - beginL1Nominator;
                expect(diff).toBeLessThan(75); // as above
                expect(diff).toBeGreaterThan(-75);
                changedL1 = true;
                break;
            }
            await sleep(0.5);
        }

        expect(changedL1).toBeTruthy();
    });

    test('Test gas consumption under large L1 gas price', async () => {
        if (testMaster.environment().l1BatchCommitDataGeneratorMode === DataAvailabityMode.Validium) {
            // We skip this test for Validium mode, since L1 gas price has little impact on the gasLimit in this mode.
            return;
        }

        // In this test we check that the server works fine when the required gasLimit is over u32::MAX.
        // Under normal server behavior, the maximal gas spent on pubdata is around 120kb * 2^20 gas/byte = ~120 * 10^9 gas.

        // In this test we will set gas per pubdata byte to its maximum value, while publishing a large L1->L2 message.

        const minimalL2GasPrice = testMaster.environment().minimalL2GasPrice;

        // We want the total gas limit to be over u32::MAX, so we need the gas per pubdata to be 50k.
        //
        // Note, that in case, any sort of overhead is present in the l2 fair gas price calculation, the final
        // gas per pubdata may be lower than 50_000. Here we assume that it is not the case, but we'll double check
        // that the gasLimit is indeed over u32::MAX, which is the most important tested property.
        const requiredPubdataPrice = minimalL2GasPrice * 100_000n;

        await mainNodeSpawner.killAndSpawnMainNode({
            newL1GasPrice: requiredPubdataPrice,
            newPubdataPrice: requiredPubdataPrice
        });

        // Wait for current batch to close so gas price is updated with the new config set above
        await waitForNewL1Batch(alice);

        const l1Messenger = new ethers.Contract(zksync.utils.L1_MESSENGER_ADDRESS, zksync.utils.L1_MESSENGER, alice);

        // Firstly, let's test a successful transaction.
        const largeData = ethers.randomBytes(90_000);
        const tx = await l1Messenger.sendToL1(largeData, { type: 0 });
        expect(tx.gasLimit > UINT32_MAX).toBeTruthy();
        const receipt = await tx.wait();
        console.log(`Gas used ${receipt.gasUsed}`);
        expect(receipt.gasUsed > UINT32_MAX).toBeTruthy();

        // Let's also check that the same transaction would work as eth_call
        const systemContextArtifact = getTestContract('ISystemContext');
        const systemContext = new ethers.Contract(SYSTEM_CONTEXT_ADDRESS, systemContextArtifact.abi, alice.provider);
        const systemContextGasPerPubdataByte = await systemContext.gasPerPubdataByte();
        expect(systemContextGasPerPubdataByte).toEqual(MAX_GAS_PER_PUBDATA);

        const dataHash = await l1Messenger.sendToL1.staticCall(largeData, { type: 0 });
        expect(dataHash).toEqual(ethers.keccak256(largeData));

        // Secondly, let's test an unsuccessful transaction with large refund.

        // The size of the data has increased, so the previous gas limit is not enough.
        const largerData = ethers.randomBytes(91_000);
        const gasToPass = receipt.gasUsed;
        const unsuccessfulTx = await l1Messenger.sendToL1(largerData, {
            gasLimit: gasToPass,
            type: 0
        });

        try {
            await unsuccessfulTx.wait();
            throw new Error('The transaction should have reverted');
        } catch {
            const receipt = await alice.provider.getTransactionReceipt(unsuccessfulTx.hash);
            expect(gasToPass - receipt!.gasUsed > UINT32_MAX).toBeTruthy();
        }
    });

    afterAll(async () => {
        // Returning the pubdata price to the default one
        // Spawning with no options restores defaults.
        await mainNodeSpawner.killAndSpawnMainNode();

        // Wait for current batch to close so gas price returns to normal.
        await waitForNewL1Batch(alice);

        await testMaster.deinitialize();
        __ZKSYNC_TEST_CONTEXT_OWNER__.setL2NodePid(mainNodeSpawner.mainNode!.proc.pid!);
    });
});

async function appendResults(
    sender: zksync.Wallet,
    originalL1Receipts: ethers.TransactionReceipt[],
    transactionRequests: ethers.TransactionRequest[],
    newL1GasPrice: bigint,
    reports: string[]
): Promise<string[]> {
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
    l1Receipt: ethers.TransactionReceipt,
    transactionRequest: ethers.TransactionRequest,
    newL1GasPrice: bigint,
    oldReport: string
): Promise<string> {
    const expectedL1Price = +ethers.formatEther(l1Receipt.gasUsed * newL1GasPrice);
    // This is flaky without multiplying by 3.
    const estimatedL2GasPrice = ethers.getBigInt(await sender.provider.send('eth_gasPrice', [])) * 3n;
    transactionRequest.maxFeePerGas = estimatedL2GasPrice;
    const estimatedL2GasLimit = await sender.estimateGas(transactionRequest);
    const estimatedPrice = estimatedL2GasPrice * estimatedL2GasLimit;

    const balanceBefore = await sender.getBalance();
    const transaction = await sender.sendTransaction(transactionRequest);
    console.log(`Sending transaction: ${transaction.hash}`);
    await transaction.wait();
    const balanceAfter = await sender.getBalance();
    const balanceDiff = balanceBefore - balanceAfter;

    const l2PriceAsNumber = +ethers.formatEther(balanceDiff);
    const l2EstimatedPriceAsNumber = +ethers.formatEther(estimatedPrice);

    const gasReport = `Gas price ${newL1GasPrice / 1000000000n} gwei:
    L1 cost ${expectedL1Price},
    L2 estimated cost: ${l2EstimatedPriceAsNumber}
    Estimated Gain: ${expectedL1Price / l2EstimatedPriceAsNumber}
    L2 cost: ${l2PriceAsNumber},
    Gain: ${expectedL1Price / l2PriceAsNumber}\n`;
    console.log(gasReport);

    return oldReport + gasReport;
}
