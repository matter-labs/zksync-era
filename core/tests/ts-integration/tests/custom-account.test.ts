/**
 * This suite contains tests checking the behavior of custom accounts (accounts represented by smart contracts).
 */

import { TestMaster } from '../src';

import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { deployContract, getTestContract } from '../src/helpers';
import { ERC20_PER_ACCOUNT, L2_DEFAULT_ETH_PER_ACCOUNT } from '../src/context-owner';
import { shouldChangeETHBalances, shouldChangeTokenBalances } from '../src/modifiers/balance-checker';

// We create multiple custom accounts and we need to fund them with ETH to pay for fees.
const ETH_PER_CUSTOM_ACCOUNT = L2_DEFAULT_ETH_PER_ACCOUNT / 8n;
const TRANSFER_AMOUNT = 1n;
const DEFAULT_TIMESTAMP_ASSERTER_RANGE_START = 0;
// 2555971200 is a number of seconds up to 30/12/2050
const DEFAULT_TIMESTAMP_ASSERTER_RANGE_END = 2555971200;

describe.skip('Tests for the custom account behavior', () => {
    let contracts: any;

    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let customAccount: zksync.Contract;
    let erc20Address: string;
    let erc20: zksync.Contract;
    let timestampAsserterAddress: string;

    beforeAll(() => {
        contracts = {
            customAccount: getTestContract('CustomAccount'),
            context: getTestContract('Context')
        };
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        erc20Address = testMaster.environment().erc20Token.l2Address;
        timestampAsserterAddress = testMaster.environment().timestampAsserterAddress;
        erc20 = new zksync.Contract(
            erc20Address,
            zksync.utils.IERC20,
            // Signer doesn't matter for custom account transactions, as signature would be replaced with custom one.
            alice
        );
    });

    test('Should deploy custom account', async () => {
        const violateRules = false;
        customAccount = await deployContract(
            alice,
            contracts.customAccount,
            [
                violateRules,
                timestampAsserterAddress,
                DEFAULT_TIMESTAMP_ASSERTER_RANGE_START,
                DEFAULT_TIMESTAMP_ASSERTER_RANGE_END
            ],
            'createAccount'
        );

        // Now we need to check that it was correctly marked as an account:
        const contractAccountInfo = await alice.provider.getContractAccountInfo(await customAccount.getAddress());

        // Checking that the version of the account abstraction is correct
        expect(contractAccountInfo.supportedAAVersion).toEqual(zksync.types.AccountAbstractionVersion.Version1);

        // Checking that the nonce ordering is correct
        expect(contractAccountInfo.nonceOrdering).toEqual(zksync.types.AccountNonceOrdering.Sequential);

        return customAccount;
    });

    test('Should fund the custom account', async () => {
        await alice
            .transfer({ to: await customAccount.getAddress(), amount: ETH_PER_CUSTOM_ACCOUNT })
            .then((tx) => tx.wait());
        await alice
            .transfer({
                to: await customAccount.getAddress(),
                token: erc20Address,
                amount: ERC20_PER_ACCOUNT / 8n
            })
            .then((tx) => tx.wait());
    });

    test('Should execute contract by custom account', async () => {
        const tx = await erc20.transfer.populateTransaction(alice.address, TRANSFER_AMOUNT);
        const customAccountAddress = await customAccount.getAddress();

        const erc20BalanceChange = await shouldChangeTokenBalances(erc20Address, [
            // Custom account change (sender)
            {
                addressToCheck: customAccountAddress,
                wallet: alice,
                change: -TRANSFER_AMOUNT
            },
            // Alice change (recipient)
            { wallet: alice, change: TRANSFER_AMOUNT }
        ]);
        const feeCheck = await shouldChangeETHBalances([
            // 0 change would only check for fees.
            { addressToCheck: customAccountAddress, wallet: alice, change: 0n }
        ]);

        // Check that transaction succeeds.
        await expect(
            sendCustomAccountTransaction(
                tx as zksync.types.Transaction,
                alice.provider,
                customAccountAddress,
                testMaster.environment().l2ChainId
            )
        ).toBeAccepted([erc20BalanceChange, feeCheck]);
    });

    test('Should fail transaction validation due to timestamp assertion in the validation tracer - close to the range end', async () => {
        const now = Math.floor(Date.now() / 1000);
        const minTimeTillEnd = testMaster.environment().timestampAsserterMinTimeTillEndSec;
        const rangeStart = now - 10;
        const rangeEnd = now + minTimeTillEnd / 2;

        const customAccount = await deployAndFundCustomAccount(
            alice,
            erc20Address,
            timestampAsserterAddress,
            rangeStart,
            rangeEnd,
            contracts.customAccount
        );

        const tx = await erc20.transfer.populateTransaction(alice.address, TRANSFER_AMOUNT);

        await expect(
            sendCustomAccountTransaction(
                tx as zksync.types.Transaction,
                alice.provider,
                await customAccount.getAddress(),
                testMaster.environment().l2ChainId
            )
        ).toBeRejected(
            'failed to validate the transaction. reason: Violated validation rules: block.timestamp is too close to the range end'
        );
    });

    test('Should execute contract by custom account when timestamp asserter range end overflows', async () => {
        // This test ensures that a custom account transaction completes successfully
        // even when the timestamp asserter's range end exceeds `u64::MAX`. In such cases,
        // the range is capped at `u64::MAX` and processed as expected.
        const customAccount = await deployAndFundCustomAccount(
            alice,
            erc20Address,
            timestampAsserterAddress,
            0,
            BigInt('3402823669209384634633746074317682'), // u128::MAX
            contracts.customAccount
        );

        const tx = await erc20.transfer.populateTransaction(alice.address, TRANSFER_AMOUNT);
        const customAccountAddress = await customAccount.getAddress();
        const erc20BalanceChange = await shouldChangeTokenBalances(erc20Address, [
            {
                addressToCheck: customAccountAddress,
                wallet: alice,
                change: -TRANSFER_AMOUNT
            },
            { wallet: alice, change: TRANSFER_AMOUNT }
        ]);
        const feeCheck = await shouldChangeETHBalances([
            { addressToCheck: customAccountAddress, wallet: alice, change: 0n }
        ]);

        await expect(
            sendCustomAccountTransaction(
                tx as zksync.types.Transaction,
                alice.provider,
                await customAccount.getAddress(),
                testMaster.environment().l2ChainId
            )
        ).toBeAccepted([erc20BalanceChange, feeCheck]);
    });

    test('Should fail to estimate fee due to block.timestamp assertion in the smart contract', async () => {
        const now = Math.floor(Date.now() / 1000);
        const rangeStart = now + 300;
        const rangeEnd = now + 1000;

        const customAccount = await deployAndFundCustomAccount(
            alice,
            erc20Address,
            timestampAsserterAddress,
            rangeStart,
            rangeEnd,
            contracts.customAccount
        );
        const customAccountAddress = await customAccount.getAddress();

        const tx = await erc20.transfer.populateTransaction(alice.address, TRANSFER_AMOUNT);

        try {
            await sendCustomAccountTransaction(
                tx as zksync.types.Transaction,
                alice.provider,
                customAccountAddress,
                testMaster.environment().l2ChainId,
                undefined,
                undefined,
                false
            );
            expect(null).fail('The transaction was expected to fail');
        } catch (e) {
            const err = e as Error;
            expect(err.message).toContain(
                'failed to validate the transaction. reason: Validation revert: Account validation error'
            );
            const functionSelectorMatch = err.message.match(/function_selector\s=\s(0x[0-9a-fA-F]{8})/);
            const calldataMatch = err.message.match(/data\s=\s(0x[0-9a-fA-F]+)/);

            expect(functionSelectorMatch && calldataMatch).toBeTruthy();

            const functionSelector = functionSelectorMatch![1];
            expect(functionSelector).toBe('0x3d5740d9');

            const calldata = calldataMatch![1];

            const startHex = calldata.slice(74, 138);
            const endHex = calldata.slice(138);
            const start = BigInt(`0x${startHex}`);
            const end = BigInt(`0x${endHex}`);

            expect(start).toBe(BigInt(rangeStart));
            expect(end).toBe(BigInt(rangeEnd));
        }
    });

    test('Should fail the validation with incorrect signature', async () => {
        const tx = await erc20.transfer.populateTransaction(alice.address, TRANSFER_AMOUNT);
        const fakeSignature = new Uint8Array(12);
        await expect(
            sendCustomAccountTransaction(
                tx as zksync.types.Transaction,
                alice.provider,
                await customAccount.getAddress(),
                testMaster.environment().l2ChainId,
                fakeSignature
            )
        ).toBeRejected('failed to validate the transaction.');
    });

    test('Should not allow violating validation rules', async () => {
        // We configure account to violate storage access rules during tx validation.
        const violateRules = true;
        const badCustomAccount = await deployContract(
            alice,
            contracts.customAccount,
            [
                violateRules,
                timestampAsserterAddress,
                DEFAULT_TIMESTAMP_ASSERTER_RANGE_START,
                DEFAULT_TIMESTAMP_ASSERTER_RANGE_END
            ],
            'createAccount'
        );
        const badCustomAccountAddress = await badCustomAccount.getAddress();

        // Fund the account.
        await alice
            .transfer({
                to: badCustomAccountAddress,
                amount: ETH_PER_CUSTOM_ACCOUNT
            })
            .then((tx) => tx.wait());
        await alice
            .transfer({
                to: badCustomAccountAddress,
                token: erc20Address,
                amount: TRANSFER_AMOUNT
            })
            .then((tx) => tx.wait());

        let tx = await erc20.transfer.populateTransaction(alice.address, TRANSFER_AMOUNT);
        await expect(
            sendCustomAccountTransaction(
                tx as zksync.types.Transaction,
                alice.provider,
                badCustomAccountAddress,
                testMaster.environment().l2ChainId
            )
        ).toBeRejected('Violated validation rules');
    });

    test('Should not execute from non-account', async () => {
        // Note that we supply "create" instead of "createAccount" here -- the code is the same, but it'll
        // be treated as a common contract.
        const violateRules = false;
        const nonAccount = await deployContract(
            alice,
            contracts.customAccount,
            [
                violateRules,
                timestampAsserterAddress,
                DEFAULT_TIMESTAMP_ASSERTER_RANGE_START,
                DEFAULT_TIMESTAMP_ASSERTER_RANGE_END
            ],
            'create'
        );
        const nonAccountAddress = await nonAccount.getAddress();

        // Fund the account.
        await alice.transfer({ to: nonAccountAddress, amount: ETH_PER_CUSTOM_ACCOUNT }).then((tx) => tx.wait());
        await alice
            .transfer({
                to: nonAccountAddress,
                token: erc20Address,
                amount: TRANSFER_AMOUNT
            })
            .then((tx) => tx.wait());

        let tx = await erc20.transfer.populateTransaction(alice.address, TRANSFER_AMOUNT);
        /*
        Ethers v6 error handling is not capable of handling this format of messages.
        See: https://github.com/ethers-io/ethers.js/blob/main/src.ts/providers/provider-jsonrpc.ts#L976
        {
            "code": 3,
            "message": "invalid sender. can't start a transaction from a non-account",
            "data": "0x"
         }
         */
        await expect(
            sendCustomAccountTransaction(
                tx as zksync.types.Transaction,
                alice.provider,
                nonAccountAddress,
                testMaster.environment().l2ChainId
            )
        ).toBeRejected(/*"invalid sender. can't start a transaction from a non-account"*/);
    });

    test('Should provide correct tx.origin for EOA and custom accounts', async () => {
        const contextContract = await deployContract(alice, contracts.context, []);

        // For EOA, the tx.origin should be the EOA
        await expect(contextContract.checkTxOrigin(alice.address)).toBeAccepted([]);

        // For custom accounts, the tx.origin should be the bootloader address
        const customAATx = await contextContract.checkTxOrigin.populateTransaction(
            zksync.utils.BOOTLOADER_FORMAL_ADDRESS
        );
        await expect(
            sendCustomAccountTransaction(
                customAATx as zksync.types.Transaction,
                alice.provider,
                await customAccount.getAddress(),
                testMaster.environment().l2ChainId
            )
        ).toBeAccepted([]);
    });

    test('API should reject validation that takes too many computational ergs', async () => {
        const violateStorageRules = false;
        const badCustomAccount = await deployContract(
            alice,
            contracts.customAccount,
            [
                violateStorageRules,
                timestampAsserterAddress,
                DEFAULT_TIMESTAMP_ASSERTER_RANGE_START,
                DEFAULT_TIMESTAMP_ASSERTER_RANGE_END
            ],
            'createAccount'
        );
        const badCustomAccountAddress = await badCustomAccount.getAddress();
        badCustomAccount.connect(alice);

        // Fund the account.
        await alice
            .transfer({
                to: badCustomAccountAddress,
                amount: ETH_PER_CUSTOM_ACCOUNT
            })
            .then((tx) => tx.wait());
        await alice
            .transfer({
                to: badCustomAccountAddress,
                token: erc20Address,
                amount: TRANSFER_AMOUNT
            })
            .then((tx) => tx.wait());

        // Set flag to do many calculations during validation.
        const validationGasLimit = testMaster.environment().validationComputationalGasLimit;
        await badCustomAccount.setGasToSpent(validationGasLimit).then((tx: any) => tx.wait());

        let tx = await erc20.transfer.populateTransaction(alice.address, TRANSFER_AMOUNT);
        await expect(
            sendCustomAccountTransaction(
                tx as zksync.types.Transaction,
                alice.provider,
                badCustomAccountAddress,
                testMaster.environment().l2ChainId
            )
        ).toBeRejected('Violated validation rules: Took too many computational gas');
    });

    test('State keeper should reject validation that takes too many computational ergs', async () => {
        const violateStorageRules = false;
        const badCustomAccount = await deployContract(
            alice,
            contracts.customAccount,
            [
                violateStorageRules,
                timestampAsserterAddress,
                DEFAULT_TIMESTAMP_ASSERTER_RANGE_START,
                DEFAULT_TIMESTAMP_ASSERTER_RANGE_END
            ],
            'createAccount'
        );
        const badCustomAccountAddress = await badCustomAccount.getAddress();
        badCustomAccount.connect(alice);

        // Fund the account.
        await alice
            .transfer({
                to: badCustomAccountAddress,
                amount: ETH_PER_CUSTOM_ACCOUNT
            })
            .then((tx) => tx.wait());
        await alice
            .transfer({
                to: badCustomAccountAddress,
                token: erc20Address,
                amount: TRANSFER_AMOUNT
            })
            .then((tx) => tx.wait());

        const transfer = await erc20.transfer.populateTransaction(alice.address, TRANSFER_AMOUNT);
        const nonce = await alice.provider.getTransactionCount(badCustomAccountAddress);

        // delayedTx should pass API checks (if not then error will be thrown on the next lime)
        // but should be rejected by the state-keeper (checked later).
        const delayedTx = await sendCustomAccountTransaction(
            transfer as zksync.types.Transaction,
            alice.provider,
            badCustomAccountAddress,
            testMaster.environment().l2ChainId,
            undefined,
            nonce + 1
        );

        // Increase nonce and set flag to do many calculations during validation.
        const validationGasLimit = testMaster.environment().validationComputationalGasLimit;
        const tx = await badCustomAccount.setGasToSpent.populateTransaction(validationGasLimit);
        await expect(
            sendCustomAccountTransaction(
                tx as zksync.types.Transaction,
                alice.provider,
                badCustomAccountAddress,
                testMaster.environment().l2ChainId,
                undefined,
                nonce
            )
        ).toBeAccepted();

        // We don't have a good check that tx was indeed rejected.
        // Most that we can do is to ensure that tx wasn't mined for some time.
        const attempts = 5;
        for (let i = 0; i < attempts; ++i) {
            const receipt = await alice.provider.getTransactionReceipt(delayedTx.hash);
            expect(receipt).toBeNull();
            await zksync.utils.sleep(1000);
        }
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});

// Accepts the tx request with filled transaction's data and
// sends the transaction that should be accepted by the `custom-aa.sol` test contract.
async function sendCustomAccountTransaction(
    tx: zksync.types.Transaction,
    browserProvider: zksync.Provider,
    accountAddress: string,
    chainId: bigint,
    customSignature?: Uint8Array,
    nonce?: number,
    estimateGas: boolean = true
) {
    const gasLimit = estimateGas ? await browserProvider.estimateGas({ ...tx, from: accountAddress }) : BigInt(100_000); // Enough gas to invoke AA contract

    const gasPrice = await browserProvider.getGasPrice();

    tx.gasLimit = gasLimit;
    tx.gasPrice = gasPrice;
    tx.chainId = chainId;
    tx.value = 0n;
    tx.nonce = nonce ?? (await browserProvider.getTransactionCount(accountAddress));
    tx.type = 113;
    tx.from = accountAddress;

    tx.customData = {
        gasPerPubdata: zksync.utils.DEFAULT_GAS_PER_PUBDATA_LIMIT
    };

    const signedTxHash = zksync.EIP712Signer.getSignedDigest(tx);
    tx.customData = {
        ...tx.customData,
        customSignature: customSignature ?? ethers.concat([signedTxHash, accountAddress])
    };
    const serializedTx = zksync.utils.serializeEip712({ ...tx });

    return await browserProvider.broadcastTransaction(serializedTx);
}

async function deployAndFundCustomAccount(
    richAccount: zksync.Wallet,
    erc20Address: string,
    timestampAsserterAddress: string,
    rangeStart: any,
    rangeEnd: any,
    customAccount_: any
): Promise<zksync.Contract> {
    const customAccount = await deployContract(
        richAccount,
        customAccount_,
        [false, timestampAsserterAddress, rangeStart, rangeEnd],
        'createAccount'
    );

    await richAccount
        .transfer({ to: await customAccount.getAddress(), amount: ETH_PER_CUSTOM_ACCOUNT })
        .then((tx) => tx.wait());
    await richAccount
        .transfer({
            to: await customAccount.getAddress(),
            token: erc20Address,
            amount: ERC20_PER_ACCOUNT / 8n
        })
        .then((tx) => tx.wait());
    return customAccount;
}
