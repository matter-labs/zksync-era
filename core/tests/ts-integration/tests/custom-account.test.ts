/**
 * This suite contains tests checking the behavior of custom accounts (accounts represented by smart contracts).
 */

import { TestMaster } from '../src';

import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { deployContract, getTestContract } from '../src/helpers';
import { ERC20_PER_ACCOUNT, L2_DEFAULT_ETH_PER_ACCOUNT } from '../src/context-owner';
import { shouldChangeETHBalances, shouldChangeTokenBalances } from '../src/modifiers/balance-checker';

const contracts = {
    customAccount: getTestContract('CustomAccount'),
    context: getTestContract('Context')
};

// We create multiple custom accounts and we need to fund them with ETH to pay for fees.
const ETH_PER_CUSTOM_ACCOUNT = L2_DEFAULT_ETH_PER_ACCOUNT / 8n;
const TRANSFER_AMOUNT = 1n;

describe('Tests for the custom account behavior', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let customAccount: zksync.Contract;
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

    test('Should deploy custom account', async () => {
        const violateRules = false;
        customAccount = await deployContract(alice, contracts.customAccount, [violateRules], 'createAccount');

        // Now we need to check that it was correctly marked as an account:
        const contractAccountInfo = await alice.provider.getContractAccountInfo(await customAccount.getAddress());

        // Checking that the version of the account abstraction is correct
        expect(contractAccountInfo.supportedAAVersion).toEqual(zksync.types.AccountAbstractionVersion.Version1);

        // Checking that the nonce ordering is correct
        expect(contractAccountInfo.nonceOrdering).toEqual(zksync.types.AccountNonceOrdering.Sequential);
    });

    test('Should fund the custom account', async () => {
        await alice
            .transfer({ to: await customAccount.getAddress(), amount: ETH_PER_CUSTOM_ACCOUNT })
            .then((tx) => tx.wait());
        await alice
            .transfer({
                to: await customAccount.getAddress(),
                token: erc20Address,
                amount: ERC20_PER_ACCOUNT / 4n
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
        const badCustomAccount = await deployContract(alice, contracts.customAccount, [violateRules], 'createAccount');
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
        const nonAccount = await deployContract(alice, contracts.customAccount, [violateRules], 'create');
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
            [violateStorageRules],
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
            [violateStorageRules],
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
    nonce?: number
) {
    const gasLimit = await browserProvider.estimateGas({
        ...tx,
        from: accountAddress
    });
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
