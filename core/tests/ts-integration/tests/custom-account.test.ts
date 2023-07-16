/**
 * This suite contains tests checking the behavior of custom accounts (accounts represented by smart contracts).
 */

import { TestMaster } from '../src/index';

import * as zksync from 'zksync-web3';
import { utils, types } from 'zksync-web3';
import * as ethers from 'ethers';
import { deployContract, getTestContract } from '../src/helpers';
import { ERC20_PER_ACCOUNT, L2_ETH_PER_ACCOUNT } from '../src/context-owner';
import { shouldChangeETHBalances, shouldChangeTokenBalances } from '../src/modifiers/balance-checker';

const contracts = {
    customAccount: getTestContract('CustomAccount'),
    context: getTestContract('Context')
};

// We create multiple custom accounts and we need to fund them with ETH to pay for fees.
const ETH_PER_CUSTOM_ACCOUNT = L2_ETH_PER_ACCOUNT.div(8);
const TRANSFER_AMOUNT = 1;

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
            utils.IERC20,
            // Signer doesn't matter for custom account transactions, as signature would be replaced with custom one.
            alice
        );
    });

    test('Should deploy custom account', async () => {
        const violateRules = false;
        customAccount = await deployContract(alice, contracts.customAccount, [violateRules], 'createAccount');

        // Now we need to check that it was correctly marked as an account:
        const contractAccountInfo = await alice.provider.getContractAccountInfo(customAccount.address);

        // Checking that the version of the account abstraction is correct
        expect(contractAccountInfo.supportedAAVersion).toEqual(types.AccountAbstractionVersion.Version1);

        // Checking that the nonce ordering is correct
        expect(contractAccountInfo.nonceOrdering).toEqual(types.AccountNonceOrdering.Sequential);
    });

    test('Should fund the custom account', async () => {
        await alice.transfer({ to: customAccount.address, amount: ETH_PER_CUSTOM_ACCOUNT }).then((tx) => tx.wait());
        await alice
            .transfer({
                to: customAccount.address,
                token: erc20Address,
                amount: ERC20_PER_ACCOUNT.div(4)
            })
            .then((tx) => tx.wait());
    });

    test('Should execute contract by custom account', async () => {
        const tx = await erc20.populateTransaction.transfer(alice.address, TRANSFER_AMOUNT);

        const erc20BalanceChange = await shouldChangeTokenBalances(erc20Address, [
            // Custom account change (sender)
            {
                addressToCheck: customAccount.address,
                wallet: alice,
                change: -TRANSFER_AMOUNT
            },
            // Alice change (recipient)
            { wallet: alice, change: TRANSFER_AMOUNT }
        ]);
        const feeCheck = await shouldChangeETHBalances([
            // 0 change would only check for fees.
            { addressToCheck: customAccount.address, wallet: alice, change: 0 }
        ]);

        // Check that transaction succeeds.
        await expect(sendCustomAccountTransaction(tx, alice.provider, customAccount.address)).toBeAccepted([
            erc20BalanceChange,
            feeCheck
        ]);
    });

    test('Should fail the validation with incorrect signature', async () => {
        const tx = await erc20.populateTransaction.transfer(alice.address, TRANSFER_AMOUNT);
        const fakeSignature = new Uint8Array(12);
        await expect(
            sendCustomAccountTransaction(tx, alice.provider, customAccount.address, fakeSignature)
        ).toBeRejected('failed to validate the transaction.');
    });

    test('Should not allow violating validation rules', async () => {
        // We configure account to violate storage access rules during tx validation.
        const violateRules = true;
        const badCustomAccount = await deployContract(alice, contracts.customAccount, [violateRules], 'createAccount');

        // Fund the account.
        await alice
            .transfer({
                to: badCustomAccount.address,
                amount: ETH_PER_CUSTOM_ACCOUNT
            })
            .then((tx) => tx.wait());
        await alice
            .transfer({
                to: badCustomAccount.address,
                token: erc20Address,
                amount: TRANSFER_AMOUNT
            })
            .then((tx) => tx.wait());

        let tx = await erc20.populateTransaction.transfer(alice.address, TRANSFER_AMOUNT);
        await expect(sendCustomAccountTransaction(tx, alice.provider, badCustomAccount.address)).toBeRejected(
            'Violated validation rules'
        );
    });

    test('Should not execute from non-account', async () => {
        // Note that we supply "create" instead of "createAccount" here -- the code is the same, but it'll
        // be treated as a common contract.
        const violateRules = false;
        const nonAccount = await deployContract(alice, contracts.customAccount, [violateRules], 'create');

        // Fund the account.
        await alice.transfer({ to: nonAccount.address, amount: ETH_PER_CUSTOM_ACCOUNT }).then((tx) => tx.wait());
        await alice
            .transfer({
                to: nonAccount.address,
                token: erc20Address,
                amount: TRANSFER_AMOUNT
            })
            .then((tx) => tx.wait());

        let tx = await erc20.populateTransaction.transfer(alice.address, TRANSFER_AMOUNT);
        await expect(sendCustomAccountTransaction(tx, alice.provider, nonAccount.address)).toBeRejected(
            "invalid sender. can't start a transaction from a non-account"
        );
    });

    test('Should provide correct tx.origin for EOA and custom accounts', async () => {
        const contextContract = await deployContract(alice, contracts.context, []);

        // For EOA, the tx.origin should be the EOA
        await expect(contextContract.checkTxOrigin(alice.address)).toBeAccepted([]);

        // For custom accounts, the tx.origin should be the bootloader address
        const customAATx = await contextContract.populateTransaction.checkTxOrigin(utils.BOOTLOADER_FORMAL_ADDRESS);
        await expect(sendCustomAccountTransaction(customAATx, alice.provider, customAccount.address)).toBeAccepted([]);
    });

    test('API should reject validation that takes too many computational ergs', async () => {
        const violateStorageRules = false;
        const badCustomAccount = await deployContract(
            alice,
            contracts.customAccount,
            [violateStorageRules],
            'createAccount'
        );
        badCustomAccount.connect(alice);

        // Fund the account.
        await alice
            .transfer({
                to: badCustomAccount.address,
                amount: ETH_PER_CUSTOM_ACCOUNT
            })
            .then((tx) => tx.wait());
        await alice
            .transfer({
                to: badCustomAccount.address,
                token: erc20Address,
                amount: TRANSFER_AMOUNT
            })
            .then((tx) => tx.wait());

        // Set flag to do many calculations during validation.
        const validationGasLimit = +process.env.CHAIN_STATE_KEEPER_VALIDATION_COMPUTATIONAL_GAS_LIMIT!;
        await badCustomAccount.setGasToSpent(validationGasLimit).then((tx: any) => tx.wait());

        let tx = await erc20.populateTransaction.transfer(alice.address, TRANSFER_AMOUNT);
        await expect(sendCustomAccountTransaction(tx, alice.provider, badCustomAccount.address)).toBeRejected(
            'Violated validation rules: Took too many computational gas'
        );
    });

    test('State keeper should reject validation that takes too many computational ergs', async () => {
        const violateStorageRules = false;
        const badCustomAccount = await deployContract(
            alice,
            contracts.customAccount,
            [violateStorageRules],
            'createAccount'
        );
        badCustomAccount.connect(alice);

        // Fund the account.
        await alice
            .transfer({
                to: badCustomAccount.address,
                amount: ETH_PER_CUSTOM_ACCOUNT
            })
            .then((tx) => tx.wait());
        await alice
            .transfer({
                to: badCustomAccount.address,
                token: erc20Address,
                amount: TRANSFER_AMOUNT
            })
            .then((tx) => tx.wait());

        const transfer = await erc20.populateTransaction.transfer(alice.address, TRANSFER_AMOUNT);
        const nonce = await alice.provider.getTransactionCount(badCustomAccount.address);

        const delayedTx = await sendCustomAccountTransaction(
            transfer,
            alice.provider,
            badCustomAccount.address,
            undefined,
            nonce + 1
        );
        // delayedTx passed API checks (since we got the response) but should be rejected by the state-keeper.
        const rejection = expect(delayedTx).toBeReverted();

        // Increase nonce and set flag to do many calculations during validation.
        const validationGasLimit = +process.env.CHAIN_STATE_KEEPER_VALIDATION_COMPUTATIONAL_GAS_LIMIT!;
        const tx = await badCustomAccount.populateTransaction.setGasToSpent(validationGasLimit);
        await expect(
            sendCustomAccountTransaction(tx, alice.provider, badCustomAccount.address, undefined, nonce)
        ).toBeAccepted();
        await rejection;
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});

// Accepts the tx request with filled transaction's data and
// sends the transaction that should be accepted by the `custom-aa.sol` test contract.
async function sendCustomAccountTransaction(
    tx: ethers.PopulatedTransaction,
    web3Provider: zksync.Provider,
    accountAddress: string,
    customSignature?: Uint8Array,
    nonce?: number
) {
    const gasLimit = await web3Provider.estimateGas({
        ...tx,
        from: accountAddress
    });
    const gasPrice = await web3Provider.getGasPrice();

    tx.gasLimit = gasLimit;
    tx.gasPrice = gasPrice;
    tx.chainId = parseInt(process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID!, 10);
    tx.value = ethers.BigNumber.from(0);
    tx.nonce = nonce ?? (await web3Provider.getTransactionCount(accountAddress));
    tx.type = 113;
    tx.from = accountAddress;

    tx.customData = {
        gasPerPubdata: utils.DEFAULT_GAS_PER_PUBDATA_LIMIT
    };

    const signedTxHash = zksync.EIP712Signer.getSignedDigest(tx);
    tx.customData = {
        ...tx.customData,
        from: accountAddress,
        customSignature: customSignature ?? ethers.utils.concat([signedTxHash, accountAddress])
    };
    const serializedTx = utils.serialize({ ...tx });

    return await web3Provider.sendTransaction(serializedTx);
}
