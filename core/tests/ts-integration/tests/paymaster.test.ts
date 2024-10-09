/**
 * This suite contains tests checking the behavior of paymasters -- entities that can cover fees for users.
 */
import { TestMaster } from '../src';
// import * as zksync from 'zksync-ethers';
import * as zksync from 'zksync-ethers-interop-support';
// import { Provider, Wallet, utils, Contract } from 'zksync-ethers';
import { Provider, Wallet, utils, Contract } from 'zksync-ethers-interop-support';
import * as ethers from 'ethers';
import { deployContract, getTestContract } from '../src/helpers';
import { L2_DEFAULT_ETH_PER_ACCOUNT } from '../src/context-owner';
import { checkReceipt } from '../src/modifiers/receipt-check';
import { extractFee } from '../src/modifiers/balance-checker';
import { TestMessage } from '../src/matchers/matcher-helpers';
import { Address } from 'zksync-ethers/build/types';
import * as hre from 'hardhat';
import { Deployer } from '@matterlabs/hardhat-zksync-deploy';
import { ZkSyncArtifact } from '@matterlabs/hardhat-zksync-deploy/dist/types';

const contracts = {
    customPaymaster: getTestContract('CustomPaymaster')
};

// The amount of tokens to transfer (in wei).
const AMOUNT = 1n;

// Exchange ratios for each 1 ETH wei
const CUSTOM_PAYMASTER_RATE_NUMERATOR = 5n;
const TESTNET_PAYMASTER_RATE_NUMERATOR = 1n;
const PAYMASTER_RATE_DENOMINATOR = 1n;

describe('Paymaster tests', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let paymaster: zksync.Contract;
    let erc20Address: string;
    let erc20: zksync.Contract;

    beforeAll(() => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        erc20Address = testMaster.environment().erc20Token.l2Address;
        erc20 = new zksync.Contract(
            erc20Address,
            zksync.utils.IERC20,
            // Signer doesn't matter for custom account transactions, as signature would be replaced with custom one.
            alice
        );
    });

    test('Should deploy a paymaster', async () => {
        paymaster = await deployContract(alice, contracts.customPaymaster, []);
        // Supplying paymaster with ETH it would need to cover the fees for the user
        await alice
            .transfer({ to: await paymaster.getAddress(), amount: L2_DEFAULT_ETH_PER_ACCOUNT / 4n })
            .then((tx) => tx.wait());
    });

    test('Should pay fee with paymaster', async () => {
        paymaster = await deployContract(alice, contracts.customPaymaster, []);
        const paymasterAddress = await paymaster.getAddress();
        // Supplying paymaster with ETH it would need to cover the fees for the user
        await alice.transfer({ to: paymasterAddress, amount: L2_DEFAULT_ETH_PER_ACCOUNT / 4n }).then((tx) => tx.wait());

        const correctSignature = new Uint8Array(46);

        const paymasterParamsForEstimation = await getTestPaymasterParamsForFeeEstimation(
            erc20,
            alice.address,
            paymasterAddress
        );
        const tx = await erc20.transfer.populateTransaction(alice.address, AMOUNT, {
            customData: {
                gasPerPubdata: utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
                paymasterParams: paymasterParamsForEstimation
            }
        });
        tx.gasLimit = await erc20.transfer.estimateGas(alice.address, AMOUNT, {
            customData: {
                gasPerPubdata: utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
                paymasterParams: paymasterParamsForEstimation
            }
        });

        const txPromise = sendTxWithTestPaymasterParams(
            tx as zksync.types.Transaction,
            alice.provider,
            alice,
            paymasterAddress,
            erc20Address,
            correctSignature,
            testMaster.environment().l2ChainId
        );
        await expect(txPromise).toBeAccepted([
            checkReceipt(
                (receipt) => paidFeeWithPaymaster(receipt, CUSTOM_PAYMASTER_RATE_NUMERATOR, paymasterAddress),
                'Fee was not paid (or paid incorrectly)'
            )
        ]);
    });

    test('Should call postOp of the paymaster', async () => {
        const correctSignature = new Uint8Array(46);
        const paymasterAddress = await paymaster.getAddress();

        const paymasterParamsForEstimation = await getTestPaymasterParamsForFeeEstimation(
            erc20,
            alice.address,
            paymasterAddress
        );
        const tx = await erc20.transfer.populateTransaction(alice.address, AMOUNT, {
            customData: {
                gasPerPubdata: utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
                paymasterParams: paymasterParamsForEstimation
            }
        });
        tx.gasLimit = await erc20.transfer.estimateGas(alice.address, AMOUNT, {
            customData: {
                gasPerPubdata: utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
                paymasterParams: paymasterParamsForEstimation
            }
        });
        // We add 300k gas to make sure that the postOp is successfully called
        // Note, that the successful call of the postOp is not guaranteed by the protocol &
        // should not be required from the users. We still do it here for the purpose of the test.
        tx.gasLimit = tx.gasLimit! + 300000n;

        testMaster.environment().l2ChainId;
        const txPromise = sendTxWithTestPaymasterParams(
            tx as zksync.types.Transaction,
            alice.provider,
            alice,
            paymasterAddress,
            erc20Address,
            correctSignature,
            testMaster.environment().l2ChainId
        );
        await expect(txPromise).toBeAccepted([
            checkReceipt(
                (receipt) => paidFeeWithPaymaster(receipt, CUSTOM_PAYMASTER_RATE_NUMERATOR, paymasterAddress),
                'Fee was not paid (or paid incorrectly)'
            )
        ]);

        const afterCounter = await paymaster.txCounter();
        const calledContextWithCounter = await paymaster.calledContext(afterCounter);
        expect(calledContextWithCounter).toEqual(true);
    });

    test('Should pay fees with testnet paymaster', async () => {
        // The testnet paymaster is not available on mainnet
        if (testMaster.environment().network == 'mainnet') {
            return;
        }

        const testnetPaymaster = (await alice.provider.getTestnetPaymasterAddress())!;
        expect(testnetPaymaster).toBeTruthy();

        // Supplying paymaster with ETH it would need to cover the fees for the user
        await alice.transfer({ to: testnetPaymaster, amount: L2_DEFAULT_ETH_PER_ACCOUNT / 4n }).then((tx) => tx.wait());

        const tx = await erc20.transfer.populateTransaction(alice.address, AMOUNT);
        const gasPrice = await alice.provider.getGasPrice();

        const aliceERC20Balance = await erc20.balanceOf(alice.address);
        const paramsForFeeEstimation = zksync.utils.getPaymasterParams(testnetPaymaster, {
            type: 'ApprovalBased',
            // For transaction estimation we provide the paymasterInput with large
            // minimalAllowance. It is safe for the end users, since the transaction is never
            // actually signed.
            minimalAllowance: aliceERC20Balance - AMOUNT,
            token: erc20Address,
            // While the "correct" paymaster signature may not be available in the true mainnet
            // paymasters, it is accessible in this test to make the test paymaster simpler.
            // The amount that is passed does not matter, since the testnet paymaster does not enforce it
            // to cover the fee for him.
            innerInput: new Uint8Array()
        });
        const gasLimit = await erc20.transfer.estimateGas(alice.address, AMOUNT, {
            customData: {
                gasPerPubdata: utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
                paymasterParams: paramsForFeeEstimation
            }
        });
        const fee = gasPrice * gasLimit;

        const paymasterParams = utils.getPaymasterParams(testnetPaymaster, {
            type: 'ApprovalBased',
            token: erc20Address,
            minimalAllowance: fee,
            innerInput: new Uint8Array()
        });
        const txPromise = alice.sendTransaction({
            ...tx,
            gasLimit,
            customData: {
                gasPerPubdata: utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
                paymasterParams
            }
        });

        await expect(txPromise).toBeAccepted([
            checkReceipt(
                (receipt) => paidFeeWithPaymaster(receipt, TESTNET_PAYMASTER_RATE_NUMERATOR, testnetPaymaster),
                'Fee was not paid (or paid incorrectly)'
            )
        ]);
    });

    test('Should reject tx with invalid paymaster input', async () => {
        const paymasterAddress = await paymaster.getAddress();
        const paymasterParamsForEstimation = await getTestPaymasterParamsForFeeEstimation(
            erc20,
            alice.address,
            paymasterAddress
        );
        const tx = await erc20.transfer.populateTransaction(alice.address, AMOUNT, {
            customData: {
                gasPerPubdata: utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
                paymasterParams: paymasterParamsForEstimation
            }
        });
        tx.gasLimit = await erc20.transfer.estimateGas(alice.address, AMOUNT, {
            customData: {
                gasPerPubdata: utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
                paymasterParams: paymasterParamsForEstimation
            }
        });

        const incorrectSignature = new Uint8Array(45);
        await expect(
            sendTxWithTestPaymasterParams(
                tx as zksync.types.Transaction,
                alice.provider,
                alice,
                paymasterAddress,
                erc20Address,
                incorrectSignature,
                testMaster.environment().l2ChainId
            )
        ).toBeRejected('Paymaster validation error');
    });

    test('Should deploy nonce-check paymaster and not fail validation', async function () {
        const deployer = new Deployer(hre as any, alice as any);
        const paymaster = await deployPaymaster(deployer);
        const paymasterAddress = await paymaster.getAddress();
        const token = testMaster.environment().erc20Token;

        await (
            await deployer.zkWallet.sendTransaction({
                to: paymasterAddress,
                value: ethers.parseEther('0.01')
            })
        ).wait();

        const paymasterParams = utils.getPaymasterParams(paymasterAddress, {
            type: 'ApprovalBased',
            token: token.l2Address,
            minimalAllowance: 1n,
            innerInput: new Uint8Array()
        });

        let bob = testMaster.newEmptyAccount();

        let aliceTx = await alice.transfer({
            to: bob.address,
            amount: 100,
            token: token.l2Address
        });

        await aliceTx.wait();

        let bobTx = bob.transfer({
            to: alice.address,
            amount: 1,
            token: token.l2Address,
            overrides: {
                customData: {
                    gasPerPubdata: utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
                    paymasterParams
                }
            }
        });

        /*
        Ethers v6 error handling is not capable of handling this format of messages.
        See: https://github.com/ethers-io/ethers.js/blob/main/src.ts/providers/provider-jsonrpc.ts#L976
        {
          "code": 3,
          "message": "failed paymaster validation. error message: Nonce is zerooo",
          "data": "0x"
        }
         */
        await expect(bobTx).toBeRejected(/*'Nonce is zerooo'*/);

        const aliceTx2 = alice.transfer({
            to: alice.address,
            amount: 1,
            token: token.l2Address,
            overrides: {
                customData: {
                    gasPerPubdata: utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
                    paymasterParams
                }
            }
        });

        await expect(aliceTx2).toBeAccepted();
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});

/**
 * Matcher modifier that checks if the fee was paid with the paymaster.
 * It only checks the receipt logs and assumes that logs are correct (e.g. if event is present, tokens were moved).
 * Assumption is that other tests ensure this invariant.
 */
function paidFeeWithPaymaster(
    receipt: zksync.types.TransactionReceipt,
    ratioNumerator: bigint,
    paymaster: string
): boolean {
    const errorMessage = (line: string) => {
        return new TestMessage()
            .matcherHint('.shouldBeAccepted.paidFeeWithPaymaster')
            .line(line)
            .line(`Transaction initiator:`)
            .expected(receipt.from)
            .line(`Paymaster address:`)
            .expected(paymaster)
            .line('Receipt')
            .received(receipt)
            .build();
    };

    // So, if the fees were paid, there should be the following logs:
    // 1. paymaster -> bootloader (fee amount in ETH)
    // 2. initiator -> paymaster (converted fee amount in ERC20)
    // Below we're looking for the 1st log, then convert it to the ERC20 log and look for it as well.
    let fee;
    try {
        fee = extractFee(receipt, paymaster);
    } catch (e) {
        // No fee was paid by paymaster, test is failed.
        expect(null).fail(errorMessage('Transaction did not have the ETH fee log'));
        throw e; // Unreachable, needed to make ts happy.
    }
    const expectedErc20Fee = getTestPaymasterFeeInToken(fee.feeBeforeRefund, ratioNumerator);

    // Find the log showing that the fee in ERC20 was taken from the user.
    // We need to pad values to represent 256-bit value.
    const fromAccountAddress = ethers.zeroPadValue(receipt.from, 32);
    const paddedAmount = ethers.toBeHex(expectedErc20Fee, 32);
    const paddedPaymaster = ethers.zeroPadValue(paymaster, 32);
    // ERC20 fee log is one that sends money to the paymaster.
    const erc20TransferTopic = ethers.id('Transfer(address,address,uint256)');
    const erc20FeeLog = receipt.logs.find((log) => {
        return (
            log.topics.length == 3 &&
            log.topics[0] == erc20TransferTopic &&
            log.topics[1] == fromAccountAddress &&
            log.topics[2] == paddedPaymaster &&
            log.data == paddedAmount
        );
    });
    if (!erc20FeeLog) {
        // ERC20 token was not taken (or taken incorrectly) from the account.
        expect(null).fail(errorMessage('Transaction did not have the ERC20 fee log (or the amount was incorrect)'));
        throw new Error(); // Unreachable, needed to make ts happy.
    }

    return true;
}

function getTestPaymasterFeeInToken(feeInEth: bigint, numerator: bigint) {
    // The number of ETH that the paymaster agrees to swap is equal to
    // tokenAmount * exchangeRateNumerator / exchangeRateDenominator
    //
    // tokenAmount * exchangeRateNumerator / exchangeRateDenominator >= feeInEth
    // tokenAmount >= feeInEth * exchangeRateDenominator / exchangeRateNumerator
    // tokenAmount = ceil(feeInEth * exchangeRateDenominator / exchangeRateNumerator)
    // for easier ceiling we do the following:
    // tokenAmount = (ethNeeded * exchangeRateDenominator + exchangeRateNumerator - 1) / exchangeRateNumerator
    return (feeInEth * PAYMASTER_RATE_DENOMINATOR + numerator - 1n) / numerator;
}

function getTestPaymasterInnerInput(signature: ethers.BytesLike, tokenAmount: bigint) {
    const abiEncoder = new ethers.AbiCoder();
    return abiEncoder.encode(
        ['bytes', 'uint256', 'uint256', 'uint256'],
        [signature, CUSTOM_PAYMASTER_RATE_NUMERATOR, PAYMASTER_RATE_DENOMINATOR, tokenAmount]
    );
}

async function getTestPaymasterParamsForFeeEstimation(
    erc20: ethers.Contract,
    senderAddress: Address,
    paymasterAddress: Address
): Promise<zksync.types.PaymasterParams> {
    // While the "correct" paymaster signature may not be available in the true mainnet
    // paymasters, it is accessible in this test to make the test paymaster simpler.
    const correctSignature = new Uint8Array(46);

    const aliceERC20Balance = await erc20.balanceOf(senderAddress);
    const paramsForFeeEstimation = zksync.utils.getPaymasterParams(paymasterAddress, {
        type: 'ApprovalBased',
        // For transaction estimation we provide the paymasterInput with large
        // minimalAllowance. It is safe for the end users, since the transaction is never
        // actually signed.
        minimalAllowance: aliceERC20Balance,
        token: await erc20.getAddress(),
        // The amount that is passed does not matter, since the testnet paymaster does not enforce it
        // to cover the fee for him.
        innerInput: getTestPaymasterInnerInput(correctSignature, 1n)
    });

    return paramsForFeeEstimation;
}

function getTestPaymasterParams(paymaster: string, token: string, ethNeeded: bigint, signature: ethers.BytesLike) {
    const tokenAmount = getTestPaymasterFeeInToken(ethNeeded, CUSTOM_PAYMASTER_RATE_NUMERATOR);
    // The input to the tester paymaster
    const innerInput = getTestPaymasterInnerInput(signature, tokenAmount);

    return utils.getPaymasterParams(paymaster, {
        type: 'ApprovalBased',
        token,
        minimalAllowance: tokenAmount,
        innerInput
    });
}

async function sendTxWithTestPaymasterParams(
    tx: zksync.types.Transaction,
    browserProvider: Provider,
    sender: Wallet,
    paymasterAddress: string,
    token: string,
    paymasterSignature: ethers.BytesLike,
    l2ChainId: bigint
) {
    const gasPrice = await browserProvider.getGasPrice();

    tx.gasPrice = gasPrice;
    tx.chainId = l2ChainId;
    tx.value = 0n;
    tx.nonce = await browserProvider.getTransactionCount(sender.address);
    tx.type = 113;

    const ethNeeded = tx.gasLimit! * gasPrice;
    const paymasterParams = getTestPaymasterParams(paymasterAddress, token, ethNeeded, paymasterSignature);

    tx.customData = {
        ...tx.customData,
        gasPerPubdata: utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
        paymasterParams
    };
    const signedTx = await sender.signTransaction(tx);
    return await browserProvider.broadcastTransaction(signedTx);
}

async function deployPaymaster(deployer: Deployer): Promise<Contract> {
    const artifactPay = getTestContract('Paymaster');
    return await deployer.deploy(artifactPay as ZkSyncArtifact);
}
