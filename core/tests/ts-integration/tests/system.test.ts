/**
 * This suite contains tests checking the overall system behavior, e.g. not any particular topic,
 * but rather how do we handle certain relatively unique situations.
 *
 * Stuff related to the edge cases, bootloader and system contracts normally expected to go here.
 */

import { TestMaster } from '../src/index';
import { shouldChangeTokenBalances } from '../src/modifiers/balance-checker';
import { L2_DEFAULT_ETH_PER_ACCOUNT } from '../src/context-owner';

import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { BigNumberish, BytesLike } from 'ethers';
import { hashBytecode, serialize } from 'zksync-ethers/build/utils';
import { SYSTEM_CONTEXT_ADDRESS, getTestContract } from '../src/helpers';
import { DataAvailabityMode } from '../src/types';
import { isNetworkLocalL2 } from 'utils';

const contracts = {
    counter: getTestContract('Counter'),
    events: getTestContract('Emitter')
};

const BUILTIN_CREATE2_FACTORY_ADDRESS = '0x0000000000000000000000000000000000010000';

describe('System behavior checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;

    beforeAll(() => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
    });

    test('Network should be supporting Cancun+Deneb', async () => {
        if (isNetworkLocalL2(process.env.CHAIN_ETH_NETWORK!)) {
            // Skipping for L2 networks
            return;
        }
        const address_a = '0x000000000000000000000000000000000000000A';
        const address_b = '0x000000000000000000000000000000000000000b';

        const transaction_a = {
            to: address_a,
            data: '0x'
        };

        await expect(alice.providerL1!.call(transaction_a)).rejects.toThrow();

        const transaction_b = {
            to: address_b,
            data: '0x'
        };

        const result_b = await alice.providerL1!.call(transaction_b);
        expect(result_b).toEqual('0x');
    });

    test('Should check that system contracts and SDK create same CREATE/CREATE2 addresses', async () => {
        const deployerContract = new zksync.Contract(
            zksync.utils.CONTRACT_DEPLOYER_ADDRESS,
            zksync.utils.CONTRACT_DEPLOYER,
            alice.provider
        );

        const sender = zksync.Wallet.createRandom().address;
        const hash = ethers.utils.randomBytes(32);
        const salt = ethers.utils.randomBytes(32);
        const input = ethers.utils.randomBytes(128);
        const nonce = 5;

        const create2AddressBySDK = zksync.utils.create2Address(sender, hash, salt, input);
        const create2AddressByDeployer = await deployerContract.getNewAddressCreate2(sender, hash, salt, input);
        expect(create2AddressBySDK).toEqual(create2AddressByDeployer);

        const createAddressBySDK = zksync.utils.createAddress(sender, nonce);
        const createAddressByDeployer = await deployerContract.getNewAddressCreate(sender, nonce);
        expect(createAddressBySDK).toEqual(createAddressByDeployer);
    });

    test('Should accept transactions with small gasPerPubdataByte', async () => {
        const smallGasPerPubdata = 1;
        const senderNonce = await alice.getTransactionCount();

        // A safe low value to determine whether we can run this test.
        // It's higher than `smallGasPerPubdata` to not make the test flaky.
        const gasPerPubdataThreshold = 5;
        const response = await alice.provider.send('zks_estimateFee', [
            { from: alice.address, to: alice.address, value: '0x1' }
        ]);
        if (response.gas_per_pubdata_limit > gasPerPubdataThreshold) {
            // This tx should be accepted by the server, but would never be executed, so we don't wait for the receipt.
            await alice.sendTransaction({
                to: alice.address,
                customData: {
                    gasPerPubdata: smallGasPerPubdata
                }
            });
            // We don't wait for the transaction receipt because it never executed.
            // When another transaction with the same nonce is made, it overwrites the previous transaction and this one should be executed.
            await expect(
                alice.sendTransaction({
                    to: alice.address,
                    nonce: senderNonce
                })
            ).toBeAccepted([]);
        }
    });

    test('Should check that bootloader utils: Legacy tx hash', async () => {
        const bootloaderUtils = bootloaderUtilsContract();

        // Testing the correctness of calculating the legacy tx hashes
        const legacyTx = await alice.populateTransaction({
            type: 0,
            to: alice.address,
            from: alice.address,
            data: '0x',
            value: 0,
            gasLimit: 50000
        });
        const txBytes = await alice.signTransaction(legacyTx);
        const parsedTx = zksync.utils.parseTransaction(txBytes);
        const txData = signedTxToTransactionData(parsedTx)!;

        const expectedTxHash = parsedTx.hash;
        delete legacyTx.from;
        const expectedSignedHash = ethers.utils.keccak256(serialize(legacyTx));

        const proposedHashes = await bootloaderUtils.getTransactionHashes(txData);
        expect(proposedHashes.txHash).toEqual(expectedTxHash);
        expect(proposedHashes.signedTxHash).toEqual(expectedSignedHash);
    });

    test('Should check bootloader utils: EIP2930 tx hash', async () => {
        const bootloaderUtils = bootloaderUtilsContract();

        // Testing EIP2930 transactions
        const eip2930Tx = await alice.populateTransaction({
            type: 1,
            to: alice.address,
            from: alice.address,
            data: '0x',
            value: 0,
            gasLimit: 50000,
            gasPrice: 55000
        });
        const signedEip2930Tx = await alice.signTransaction(eip2930Tx);
        const parsedEIP2930tx = zksync.utils.parseTransaction(signedEip2930Tx);

        const EIP2930TxData = signedTxToTransactionData(parsedEIP2930tx)!;
        delete eip2930Tx.from;
        const expectedEIP2930TxHash = parsedEIP2930tx.hash;
        const expectedEIP2930SignedHash = ethers.utils.keccak256(serialize(eip2930Tx));

        const proposedEIP2930Hashes = await bootloaderUtils.getTransactionHashes(EIP2930TxData);
        expect(proposedEIP2930Hashes.txHash).toEqual(expectedEIP2930TxHash);
        expect(proposedEIP2930Hashes.signedTxHash).toEqual(expectedEIP2930SignedHash);
    });

    test('Should check bootloader utils: EIP1559 tx hash', async () => {
        const bootloaderUtils = bootloaderUtilsContract();

        // Testing EIP1559 transactions
        const eip1559Tx = await alice.populateTransaction({
            type: 2,
            to: alice.address,
            from: alice.address,
            data: '0x',
            value: 0,
            maxFeePerGas: 12000,
            maxPriorityFeePerGas: 100
        });
        const signedEip1559Tx = await alice.signTransaction(eip1559Tx);
        const parsedEIP1559tx = zksync.utils.parseTransaction(signedEip1559Tx);

        const EIP1559TxData = signedTxToTransactionData(parsedEIP1559tx)!;
        delete eip1559Tx.from;
        const expectedEIP1559TxHash = parsedEIP1559tx.hash;
        const expectedEIP1559SignedHash = ethers.utils.keccak256(serialize(eip1559Tx));

        const proposedEIP1559Hashes = await bootloaderUtils.getTransactionHashes(EIP1559TxData);
        expect(proposedEIP1559Hashes.txHash).toEqual(expectedEIP1559TxHash);
        expect(proposedEIP1559Hashes.signedTxHash).toEqual(expectedEIP1559SignedHash);
    });

    test('Should check bootloader utils: EIP712 tx hash', async () => {
        const bootloaderUtils = bootloaderUtilsContract();

        // EIP712 transaction hashes' test
        const eip712Tx = await alice.populateTransaction({
            type: 113,
            to: alice.address,
            from: alice.address,
            data: '0x',
            value: 0,
            customData: {
                gasPerPubdata: zksync.utils.DEFAULT_GAS_PER_PUBDATA_LIMIT
            }
        });
        const signedEip712Tx = await alice.signTransaction(eip712Tx);
        const parsedEIP712tx = zksync.utils.parseTransaction(signedEip712Tx);

        const eip712TxData = signedTxToTransactionData(parsedEIP712tx)!;
        const expectedEIP712TxHash = parsedEIP712tx.hash;
        const expectedEIP712SignedHash = zksync.EIP712Signer.getSignedDigest(eip712Tx);

        const proposedEIP712Hashes = await bootloaderUtils.getTransactionHashes(eip712TxData);

        expect(proposedEIP712Hashes.txHash).toEqual(expectedEIP712TxHash);
        expect(proposedEIP712Hashes.signedTxHash).toEqual(expectedEIP712SignedHash);
    });

    test.skip('Should execute withdrawals with different parameters in one block', async () => {
        // This test checks the SDK/system contracts (not even the server) behavior, and it's very time-consuming,
        // so it doesn't make sense to run it outside the localhost environment.
        if (testMaster.isFastMode()) {
            return;
        }
        const bob = testMaster.newEmptyAccount();

        const l2Token = testMaster.environment().erc20Token.l2Address;
        const l1Token = testMaster.environment().erc20Token.l1Address;
        const amount = 1;

        // Fund Bob's account.
        await alice.transfer({ amount, to: bob.address, token: l2Token }).then((tx) => tx.wait());
        testMaster.reporter.debug('Sent L2 token to Bob');
        await alice
            .transfer({ amount: L2_DEFAULT_ETH_PER_ACCOUNT.div(8), to: bob.address, token: zksync.utils.ETH_ADDRESS })
            .then((tx) => tx.wait());
        testMaster.reporter.debug('Sent ethereum on L2 to Bob');

        // Prepare matcher modifiers for L1 balance change.
        const aliceChange = await shouldChangeTokenBalances(l1Token, [{ wallet: alice, change: amount }], { l1: true });
        const bobChange = await shouldChangeTokenBalances(l1Token, [{ wallet: bob, change: amount }], { l1: true });

        // Maximize chances of including transactions into the same block by first creating both promises
        // and only then awaiting them. This is still probabalistic though: if this test becomes flaky,
        // most likely there exists a very big problem in the system.
        const aliceWithdrawalPromise = alice
            .withdraw({ token: l2Token, amount })
            .then((response) => response.waitFinalize());
        const bobWithdrawalPromise = bob
            .withdraw({ token: l2Token, amount })
            .then((response) => response.waitFinalize());

        const [aliceReceipt, bobReceipt] = await Promise.all([aliceWithdrawalPromise, bobWithdrawalPromise]);
        testMaster.reporter.debug(
            `Obtained withdrawal receipt for Alice: blockNumber=${aliceReceipt.blockNumber}, l1BatchNumber=${aliceReceipt.l1BatchNumber}, status=${aliceReceipt.status}`
        );
        testMaster.reporter.debug(
            `Obtained withdrawal receipt for Bob: blockNumber=${bobReceipt.blockNumber}, l1BatchNumber=${bobReceipt.l1BatchNumber}, status=${bobReceipt.status}`
        );
        await expect(alice.finalizeWithdrawal(aliceReceipt.transactionHash)).toBeAccepted([aliceChange]);
        testMaster.reporter.debug('Finalized withdrawal for Alice');
        await expect(alice.finalizeWithdrawal(bobReceipt.transactionHash)).toBeAccepted([bobChange]);
        testMaster.reporter.debug('Finalized withdrawal for Bob');
    });

    test.skip('Should execute a withdrawal with same parameters twice', async () => {
        // This test is a logical copy of the previous one, but in this one we send two withdrawals from the same account
        // It's skipped outside the localhost environment for the same reason.
        if (testMaster.isFastMode()) {
            return;
        }

        const l2Token = testMaster.environment().erc20Token.l2Address;
        const l1Token = testMaster.environment().erc20Token.l1Address;
        const amount = 1;

        // Prepare matcher modifiers. These modifiers would record the *current* Alice's balance, so after
        // the first finalization the diff would be (compared to now) `amount`, and after the second -- `amount*2`.
        const change1 = await shouldChangeTokenBalances(l1Token, [{ wallet: alice, change: amount }], { l1: true });
        const change2 = await shouldChangeTokenBalances(l1Token, [{ wallet: alice, change: amount * 2 }], { l1: true });
        testMaster.reporter.debug('Prepared token balance modifiers');

        // Maximize chances of including transactions into the same block by first creating both promises
        // and only then awaiting them. This is still probabilistic though: if this test becomes flaky,
        // most likely there exists a very big problem in the system.
        const nonce = await alice.getTransactionCount();
        testMaster.reporter.debug(`Obtained Alice's nonce: ${nonce}`);
        const withdrawal1 = alice
            .withdraw({ token: l2Token, amount, overrides: { nonce } })
            .then((response) => response.waitFinalize());
        const withdrawal2 = alice
            .withdraw({ token: l2Token, amount, overrides: { nonce: nonce + 1 } })
            .then((response) => response.waitFinalize());

        const [receipt1, receipt2] = await Promise.all([withdrawal1, withdrawal2]);
        testMaster.reporter.debug(
            `Obtained withdrawal receipt #1: blockNumber=${receipt1.blockNumber}, l1BatchNumber=${receipt1.l1BatchNumber}, status=${receipt1.status}`
        );
        testMaster.reporter.debug(
            `Obtained withdrawal receipt #2: blockNumber=${receipt2.blockNumber}, l1BatchNumber=${receipt2.l1BatchNumber}, status=${receipt2.status}`
        );
        await expect(alice.finalizeWithdrawal(receipt1.transactionHash)).toBeAccepted([change1]);
        testMaster.reporter.debug('Finalized withdrawal #1');
        await expect(alice.finalizeWithdrawal(receipt2.transactionHash)).toBeAccepted([change2]);
        testMaster.reporter.debug('Finalized withdrawal #2');
    });

    test('should accept transaction with duplicated factory dep', async () => {
        const bytecode = contracts.counter.bytecode;
        // We need some bytecodes that weren't deployed before to test behavior properly.
        const dep1 = ethers.utils.hexConcat([bytecode, ethers.utils.randomBytes(64)]);
        const dep2 = ethers.utils.hexConcat([bytecode, ethers.utils.randomBytes(64)]);
        const dep3 = ethers.utils.hexConcat([bytecode, ethers.utils.randomBytes(64)]);
        await expect(
            alice.sendTransaction({
                to: alice.address,
                customData: {
                    factoryDeps: [dep2, dep1, dep3, dep3, dep1, dep2]
                }
            })
        ).toBeAccepted();
    });

    test('Gas per pubdata byte getter should work', async () => {
        const systemContextArtifact = getTestContract('ISystemContext');
        const systemContext = new ethers.Contract(SYSTEM_CONTEXT_ADDRESS, systemContextArtifact.abi, alice.provider);
        const currentGasPerPubdata = await systemContext.gasPerPubdataByte();

        // The current gas per pubdata depends on a lot of factors, so it wouldn't be sustainable to check the exact value.
        // We'll just check that it is greater than zero.
        if (testMaster.environment().l1BatchCommitDataGeneratorMode === DataAvailabityMode.Rollup) {
            expect(currentGasPerPubdata.toNumber()).toBeGreaterThan(0);
        } else {
            expect(currentGasPerPubdata.toNumber()).toEqual(0);
        }
    });

    it('should reject transaction with huge gas limit', async () => {
        await expect(
            alice.sendTransaction({ to: alice.address, gasLimit: ethers.BigNumber.from(2).pow(51) })
        ).toBeRejected('exceeds block gas limit');
    });

    it('Create2Factory should work', async () => {
        // For simplicity, we'll just deploy a contract factory
        const salt = ethers.utils.randomBytes(32);

        const bytecode = await alice.provider.getCode(BUILTIN_CREATE2_FACTORY_ADDRESS);
        const abi = getTestContract('ICreate2Factory').abi;
        const hash = hashBytecode(bytecode);

        const contractFactory = new ethers.Contract(BUILTIN_CREATE2_FACTORY_ADDRESS, abi, alice);

        const deploymentTx = await (await contractFactory.create2(salt, hash, [])).wait();

        const deployedAddresses = zksync.utils.getDeployedContracts(deploymentTx);
        expect(deployedAddresses.length).toEqual(1);
        const deployedAddress = deployedAddresses[0];
        const correctCreate2Address = zksync.utils.create2Address(contractFactory.address, hash, salt, []);

        expect(deployedAddress.deployedAddress.toLocaleLowerCase()).toEqual(correctCreate2Address.toLocaleLowerCase());
        expect(await alice.provider.getCode(deployedAddress.deployedAddress)).toEqual(bytecode);
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });

    function bootloaderUtilsContract() {
        const BOOTLOADER_UTILS_ADDRESS = '0x000000000000000000000000000000000000800c';
        const BOOTLOADER_UTILS = new ethers.utils.Interface(
            require(`${
                testMaster.environment().pathToHome
            }/contracts/system-contracts/artifacts-zk/contracts-preprocessed/BootloaderUtilities.sol/BootloaderUtilities.json`).abi
        );

        return new ethers.Contract(BOOTLOADER_UTILS_ADDRESS, BOOTLOADER_UTILS, alice);
    }
});

// Interface encoding the transaction struct used for AA protocol
export interface TransactionData {
    txType: BigNumberish;
    from: BigNumberish;
    to: BigNumberish;
    gasLimit: BigNumberish;
    gasPerPubdataByteLimit: BigNumberish;
    maxFeePerGas: BigNumberish;
    maxPriorityFeePerGas: BigNumberish;
    paymaster: BigNumberish;
    nonce: BigNumberish;
    value: BigNumberish;
    // In the future, we might want to add some
    // new fields to the struct. The `txData` struct
    // is to be passed to account and any changes to its structure
    // would mean a breaking change to these accounts. In order to prevent this,
    // we should keep some fields as "reserved".
    // It is also recommended that their length is fixed, since
    // it would allow easier proof integration (in case we will need
    // some special circuit for preprocessing transactions).
    reserved: BigNumberish[];
    data: BytesLike;
    signature: BytesLike;
    factoryDeps: BytesLike[];
    paymasterInput: BytesLike;
    // Reserved dynamic type for the future use-case. Using it should be avoided,
    // But it is still here, just in case we want to enable some additional functionality.
    reservedDynamic: BytesLike;
}

function signedTxToTransactionData(tx: ethers.Transaction) {
    // Transform legacy transaction's `v` part of the signature
    // to a single byte used in the packed eth signature
    function unpackV(v: number) {
        if (v >= 35) {
            const chainId = Math.floor((v - 35) / 2);
            return v - chainId * 2 - 8;
        } else if (v <= 1) {
            return 27 + v;
        }

        throw new Error('Invalid `v`');
    }

    function legacyTxToTransactionData(tx: any): TransactionData {
        return {
            txType: 0,
            from: tx.from!,
            to: tx.to!,
            gasLimit: tx.gasLimit!,
            gasPerPubdataByteLimit: zksync.utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
            maxFeePerGas: tx.gasPrice!,
            maxPriorityFeePerGas: tx.gasPrice!,
            paymaster: 0,
            nonce: tx.nonce,
            value: tx.value || 0,
            reserved: [tx.chainId || 0, 0, 0, 0],
            data: tx.data!,
            signature: ethers.utils.hexConcat([tx.r, tx.s, new Uint8Array([unpackV(tx.v)])]),
            factoryDeps: [],
            paymasterInput: '0x',
            reservedDynamic: '0x'
        };
    }

    function eip2930TxToTransactionData(tx: any): TransactionData {
        return {
            txType: 1,
            from: tx.from!,
            to: tx.to!,
            gasLimit: tx.gasLimit!,
            gasPerPubdataByteLimit: zksync.utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
            maxFeePerGas: tx.gasPrice!,
            maxPriorityFeePerGas: tx.gasPrice!,
            paymaster: 0,
            nonce: tx.nonce,
            value: tx.value || 0,
            reserved: [0, 0, 0, 0],
            data: tx.data!,
            signature: ethers.utils.hexConcat([tx.r, tx.s, unpackV(tx.v)]),
            factoryDeps: [],
            paymasterInput: '0x',
            reservedDynamic: '0x'
        };
    }

    function eip1559TxToTransactionData(tx: any): TransactionData {
        return {
            txType: 2,
            from: tx.from!,
            to: tx.to!,
            gasLimit: tx.gasLimit!,
            gasPerPubdataByteLimit: zksync.utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
            maxFeePerGas: tx.maxFeePerGas,
            maxPriorityFeePerGas: tx.maxPriorityFeePerGas,
            paymaster: 0,
            nonce: tx.nonce,
            value: tx.value || 0,
            reserved: [0, 0, 0, 0],
            data: tx.data!,
            signature: ethers.utils.hexConcat([tx.r, tx.s, unpackV(tx.v)]),
            factoryDeps: [],
            paymasterInput: '0x',
            reservedDynamic: '0x'
        };
    }

    function eip712TxToTransactionData(tx: any): TransactionData {
        return {
            txType: 113,
            from: tx.from!,
            to: tx.to!,
            gasLimit: tx.gasLimit!,
            gasPerPubdataByteLimit: tx.customData.gasPerPubdata || zksync.utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
            maxFeePerGas: tx.maxFeePerGas,
            maxPriorityFeePerGas: tx.maxPriorityFeePerGas,
            paymaster: tx.customData.paymasterParams?.paymaster || 0,
            nonce: tx.nonce,
            value: tx.value || 0,
            reserved: [0, 0, 0, 0],
            data: tx.data!,
            signature: tx.customData.customSignature,
            factoryDeps: tx.customData.factoryDeps.map(hashBytecode),
            paymasterInput: tx.customData.paymasterParams?.paymasterInput || '0x',
            reservedDynamic: '0x'
        };
    }

    const txType = tx.type ?? 0;

    switch (txType) {
        case 0:
            return legacyTxToTransactionData(tx);
        case 1:
            return eip2930TxToTransactionData(tx);
        case 2:
            return eip1559TxToTransactionData(tx);
        case 113:
            return eip712TxToTransactionData(tx);
        default:
            throw new Error('Unsupported tx type');
    }
}
