/**
 * Generic tests checking the deployed smart contract behavior.
 *
 * Note: if you are going to write multiple tests checking specific topic (e.g. `CREATE2` behavior or something like this),
 * consider creating a separate suite.
 * Let's try to keep only relatively simple and self-contained tests here.
 */

import { TestMaster } from '../src/index';
import { deployContract, getTestContract, waitForNewL1Batch } from '../src/helpers';
import { shouldOnlyTakeFee } from '../src/modifiers/balance-checker';

import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';
import { Provider } from 'zksync-ethers';
import * as elliptic from 'elliptic';
import { RetryProvider } from '../src/retry-provider';

// TODO: Leave only important ones.
const contracts = {
    counter: getTestContract('Counter'),
    constructor: getTestContract('SimpleConstructor'),
    expensive: getTestContract('Expensive'),
    infinite: getTestContract('InfiniteLoop'),
    create: {
        ...getTestContract('Import'),
        factoryDep: getTestContract('Foo').bytecode
    },
    context: getTestContract('Context'),
    error: getTestContract('SimpleRequire')
};

describe('Smart contract behavior checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;

    // Contracts shared in several tests.
    let counterContract: zksync.Contract;

    beforeAll(() => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
    });

    test('Should deploy & call a contract', async () => {
        counterContract = await deployContract(alice, contracts.counter, []);
        const feeCheck = await shouldOnlyTakeFee(alice);

        // Change the storage slot and ensure it actually changes.
        expect(counterContract.get()).resolves.bnToBeEq(0);
        await expect(counterContract.increment(42)).toBeAccepted([feeCheck]);
        expect(counterContract.get()).resolves.bnToBeEq(42);
    });

    test('Should deploy contract with a constructor', async () => {
        const contract1 = await deployContract(alice, contracts.constructor, [2, 3, false]);
        await expect(contract1.get()).resolves.bnToBeEq(2 * 3);

        const contract2 = await deployContract(alice, contracts.constructor, [5, 10, false]);
        await expect(contract2.get()).resolves.bnToBeEq(5 * 10);
    });

    test('Should deploy contract with create', async () => {
        const contractFactory = new zksync.ContractFactory(contracts.create.abi, contracts.create.bytecode, alice);
        const contract = await contractFactory.deploy({
            customData: {
                factoryDeps: [contracts.create.factoryDep]
            }
        });
        await contract.deployed();
        await expect(contract.getFooName()).resolves.toBe('Foo');
    });

    test('Should perform "expensive" contract calls', async () => {
        const expensiveContract = await deployContract(alice, contracts.expensive, []);

        // First, check that the transaction that is too expensive would be rejected by the API server.
        await expect(expensiveContract.expensive(3000)).toBeRejected();

        // Second, check that processable transaction may fail with "out of gas" error.
        // To do so, we estimate gas for arg "1" and supply it to arg "20".
        // This guarantees that transaction won't fail during verification.
        const lowGasLimit = await expensiveContract.estimateGas.expensive(1);
        await expect(
            expensiveContract.expensive(20, {
                gasLimit: lowGasLimit
            })
        ).toBeReverted();
    });

    test('Should fail an infinite loop transaction', async () => {
        if (testMaster.isFastMode()) {
            // TODO: This test currently doesn't work on stage (ZKD-552).
            console.log(`This test is disabled. If you see this line, please check if the issue is resolved`);
            return;
        }

        const infiniteLoop = await deployContract(alice, contracts.infinite, []);

        // Test eth_call first
        // TODO: provide a proper error for transactions that consume too much gas.
        // await expect(infiniteLoop.callStatic.infiniteLoop()).toBeRejected('cannot estimate transaction: out of gas');
        // ...and then an actual transaction
        await expect(infiniteLoop.infiniteLoop({ gasLimit: 1_000_000 })).toBeReverted([]);
    });

    test('Should test reverting storage logs', async () => {
        // In this test we check that if transaction reverts, it rolls back the storage slots.
        const prevValue = await counterContract.get();

        // We manually provide a constant, since otherwise the exception would be thrown
        // while estimating gas
        await expect(counterContract.incrementWithRevert(5, true, { gasLimit: 5000000 })).toBeReverted([]);

        // The tx has been reverted, so the value Should not have been changed:
        const newValue = await counterContract.get();
        expect(newValue).bnToBeEq(prevValue, 'The counter has changed despite the revert');
    });

    test('Should not allow invalid constructor calldata', async () => {
        const randomWrongArgs = [12, 12, true];
        await expect(deployContract(alice, contracts.counter, randomWrongArgs)).toBeRejected('too many arguments');
    });

    test('Should not allow invalid contract bytecode', async () => {
        // In this test we ensure that bytecode validity is checked by server.

        // Helpers to interact with the RPC API directly.
        const send = (tx: any) => alice.provider.send('eth_sendRawTransaction', [zksync.utils.serialize(tx)]);
        const call = (tx: any) => alice.provider.send('eth_call', [Provider.hexlifyTransaction(tx)]);
        const estimateGas = (tx: any) => alice.provider.send('eth_estimateGas', [Provider.hexlifyTransaction(tx)]);
        // Prepares an invalid serialized transaction with the bytecode of provided length.
        const invalidTx = (length: number) => invalidBytecodeTestTransaction(alice.provider, [new Uint8Array(length)]);

        const txWithUnchunkableBytecode = await invalidTx(17);
        const unchunkableError = 'Bytecode length is not divisible by 32';
        await expect(send(txWithUnchunkableBytecode)).toBeRejected(unchunkableError);
        await expect(call(txWithUnchunkableBytecode)).toBeRejected(unchunkableError);
        await expect(estimateGas(txWithUnchunkableBytecode)).toBeRejected(unchunkableError);

        const txWithBytecodeWithEvenChunks = await invalidTx(64);
        const evenChunksError = 'Bytecode has even number of 32-byte words';
        await expect(send(txWithBytecodeWithEvenChunks)).toBeRejected(evenChunksError);
        await expect(call(txWithBytecodeWithEvenChunks)).toBeRejected(evenChunksError);
        await expect(estimateGas(txWithBytecodeWithEvenChunks)).toBeRejected(evenChunksError);

        const longBytecodeLen = zksync.utils.MAX_BYTECODE_LEN_BYTES + 32;
        const txWithTooLongBytecode = await invalidTx(longBytecodeLen);
        const tooLongBytecodeError = `Bytecode too long: ${longBytecodeLen} bytes, while max ${zksync.utils.MAX_BYTECODE_LEN_BYTES} allowed`;
        await expect(send(txWithTooLongBytecode)).toBeRejected(tooLongBytecodeError);
        await expect(call(txWithTooLongBytecode)).toBeRejected(tooLongBytecodeError);
        await expect(estimateGas(txWithTooLongBytecode)).toBeRejected(tooLongBytecodeError);
    });

    test('Should interchangeably use ethers for eth calls', async () => {
        // In this test we make sure that we can use `ethers` `Contract` object and provider
        // to do an `eth_Call` and send transactions to ZKsync contract.
        // This check is important to ensure that external apps do not have to use our SDK and
        // can keep using `ethers` on their side.

        const rpcAddress = testMaster.environment().l2NodeUrl;
        const provider = new RetryProvider(rpcAddress);
        const wallet = new ethers.Wallet(alice.privateKey, provider);
        const ethersBasedContract = new ethers.Contract(counterContract.address, counterContract.interface, wallet);

        const oldValue = await ethersBasedContract.get();
        await expect(ethersBasedContract.increment(1)).toBeAccepted([]);
        expect(ethersBasedContract.get()).resolves.bnToBeEq(oldValue.add(1));
    });

    test('Should check that eth_call works with custom block tags', async () => {
        // Retrieve value normally.
        const counterValue = await counterContract.get();

        // Check current block tag.
        await expect(counterContract.callStatic.get({ blockTag: 'pending' })).resolves.bnToBeEq(counterValue);

        // Block from the future.
        await expect(counterContract.callStatic.get({ blockTag: 1000000000 })).toBeRejected(
            "Block with such an ID doesn't exist yet"
        );

        // Genesis block
        await expect(counterContract.callStatic.get({ blockTag: 0 })).toBeRejected('call revert exception');
    });

    test('Should correctly process msg.value inside constructor and in ethCall', async () => {
        const value = ethers.BigNumber.from(1);

        // Check that value provided to the constructor is processed.
        const contextContract = await deployContract(alice, contracts.context, [], undefined, { value });
        await expect(contextContract.valueOnCreate()).resolves.bnToBeEq(value);

        // Check that value provided to `eth_Call` is processed.
        // This call won't return anything, but will throw if it'll result in a revert.
        await contextContract.callStatic.requireMsgValue(value, {
            value
        });
    });

    test('Should return correct error during fee estimation', async () => {
        const errorContract = await deployContract(alice, contracts.error, []);

        await expect(errorContract.estimateGas.require_long()).toBeRevertedEstimateGas('longlonglong');
        await expect(errorContract.require_long()).toBeRevertedEthCall('longlonglong');
        await expect(errorContract.estimateGas.new_error()).toBeRevertedEstimateGas(
            undefined,
            '0x157bea60000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000046461746100000000000000000000000000000000000000000000000000000000'
        );
        await expect(errorContract.callStatic.new_error()).toBeRevertedEthCall(
            undefined,
            '0x157bea60000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000046461746100000000000000000000000000000000000000000000000000000000'
        );
    });

    test('Should check block properties for tx execution', async () => {
        if (testMaster.isFastMode()) {
            // This test requires a new L1 batch to be created, which may be very time consuming on stage.
            return;
        }

        // This test checks that block properties are correctly provided to the smart contracts.
        // Note that inside the VM we use l1 batches, not l2 blocks.
        // Also we have to use the `pending` block tag for eth calls, because by default it's "latest" and
        // will correspond to the last *sealed* batch (e.g. previous one).

        const contextContract = await deployContract(alice, contracts.context, []);
        const deploymentBlock = contextContract.deployTransaction.blockNumber!;
        const deploymentBlockInfo = await alice.provider.getBlock(deploymentBlock);
        // If batch was not sealed, its number may not be present in the receipt.
        const deploymentl1Batch = deploymentBlockInfo.l1BatchNumber ?? (await alice.provider.getL1BatchNumber()) + 1;

        // Check that block gas limit is correct.
        const blockGasLimit = await contextContract.getBlockGasLimit({ blockTag: 'pending' });
        expect(blockGasLimit).bnToBeGt(0);

        // Record values from the contract right after deployment to compare them with new ones later.
        const initialL1Batch = await contextContract.getBlockNumber({
            blockTag: 'pending'
        });
        const initialTimestamp = await contextContract.getBlockTimestamp({
            blockTag: 'pending'
        });
        // Soft check to verify that `tx.gasprice` doesn't revert.
        await contextContract.getTxGasPrice({ blockTag: 'pending' });

        // Check that current number of L1 batch on contract has sane value.
        // Here and below we use "gte"/"gt" instead of strict checks because tests are executed in parallel
        // and we can't guarantee a certain block commitment order.
        expect(initialL1Batch).bnToBeGte(deploymentl1Batch);

        // Wait till the new L1 batch is created.
        await waitForNewL1Batch(alice);

        // Now we're sure than a new L1 batch is created, we may check the new properties.
        const newL1Batch = await contextContract.getBlockNumber({
            blockTag: 'pending'
        });
        const newTimestamp = await contextContract.getBlockTimestamp({
            blockTag: 'pending'
        });

        expect(newL1Batch).bnToBeGt(initialL1Batch, 'New L1 batch number must be strictly greater');
        expect(newTimestamp).bnToBeGte(initialTimestamp, 'New timestamp must not be less than previous one');

        // And finally check block properties for the actual contract execution (not `eth_call`).
        const acceptedBlockLag = 20;
        const acceptedTimestampLag = 600;
        await expect(contextContract.checkBlockNumber(newL1Batch, newL1Batch.add(acceptedBlockLag))).toBeAccepted([]);
        // `newTimestamp` was received from the API, so actual timestamp in the state keeper may be lower.
        // This is why we use `initialTimestamp` here.
        await expect(
            contextContract.checkBlockTimestamp(initialTimestamp, initialTimestamp.add(acceptedTimestampLag))
        ).toBeAccepted([]);
    });

    test('Should successfully publish a large packable bytecode', async () => {
        // The rough length of the packed bytecode should be 350_000 / 4 = 87500,
        // which should fit into a batch
        const BYTECODE_LEN = 350_016 + 32; // +32 to ensure validity of the bytecode

        // Our current packing algorithm uses 8-byte chunks for dictionary and
        // so in order to make an effectively-packable bytecode, we need to have bytecode
        // consist of the same 2 types of 8-byte chunks.
        // Note, that instead of having 1 type of 8-byte chunks, we need 2 in order to have
        // a unique bytecode for each test run.
        const CHUNK_TYPE_1 = '00000000';
        const CHUNK_TYPE_2 = 'ffffffff';

        let bytecode = '0x';
        while (bytecode.length < BYTECODE_LEN * 2 + 2) {
            if (Math.random() < 0.5) {
                bytecode += CHUNK_TYPE_1;
            } else {
                bytecode += CHUNK_TYPE_2;
            }
        }

        await expect(
            alice.sendTransaction({
                to: alice.address,
                customData: {
                    factoryDeps: [bytecode]
                }
            })
        ).toBeAccepted([]);
    });

    test('Should reject tx with not enough gas for publishing bytecode', async () => {
        // Send a transaction with big unique factory dep and provide gas enough for validation but not for bytecode publishing.
        // Transaction should be rejected by API.

        const BYTECODE_LEN = 50016;
        const bytecode = ethers.utils.hexlify(ethers.utils.randomBytes(BYTECODE_LEN));

        // Estimate gas for "no-op". It's a good estimate for validation gas.
        const gasLimit = await alice.estimateGas({
            to: alice.address,
            data: '0x'
        });

        await expect(
            alice.sendTransaction({
                to: alice.address,
                gasLimit,
                customData: {
                    factoryDeps: [bytecode]
                }
            })
        ).toBeRejected('not enough gas to publish compressed bytecodes');
    });

    test('Should check secp256r1 precompile', async () => {
        const ec = new elliptic.ec('p256');

        const privateKeyHex = '519b423d715f8b581f4fa8ee59f4771a5b44c8130b4e3eacca54a56dda72b464';
        const privateKey = Buffer.from(privateKeyHex, 'hex');

        const message =
            '0x5905238877c77421f73e43ee3da6f2d9e2ccad5fc942dcec0cbd25482935faaf416983fe165b1a045ee2bcd2e6dca3bdf46c4310a7461f9a37960ca672d3feb5473e253605fb1ddfd28065b53cb5858a8ad28175bf9bd386a5e471ea7a65c17cc934a9d791e91491eb3754d03799790fe2d308d16146d5c9b0d0debd97d79ce8';
        const digest = ethers.utils.arrayify(ethers.utils.keccak256(message));
        const signature = ec.sign(digest, privateKey);

        const publicKeyHex =
            '0x1ccbe91c075fc7f4f033bfa248db8fccd3565de94bbfb12f3c59ff46c271bf83ce4014c68811f9a21a1fdb2c0e6113e06db7ca93b7404e78dc7ccd5ca89a4ca9';

        // Check that verification succeeds.
        const res = await alice.provider.call({
            to: '0x0000000000000000000000000000000000000100',
            data: ethers.utils.concat([
                digest,
                '0x' + signature.r.toString('hex'),
                '0x' + signature.s.toString('hex'),
                publicKeyHex
            ])
        });
        expect(res).toEqual('0x0000000000000000000000000000000000000000000000000000000000000001');

        // Send the transaction.
        const tx = await alice.sendTransaction({
            to: '0x0000000000000000000000000000000000000100',
            data: ethers.utils.concat([
                digest,
                '0x' + signature.r.toString('hex'),
                '0x' + signature.s.toString('hex'),
                publicKeyHex
            ])
        });
        const receipt = await tx.wait();
        expect(receipt.status).toEqual(1);
    });

    test('Should check transient storage', async () => {
        const artifact = require(`${
            testMaster.environment().pathToHome
        }/etc/contracts-test-data/artifacts-zk/contracts/storage/storage.sol/StorageTester.json`);
        const contractFactory = new zksync.ContractFactory(artifact.abi, artifact.bytecode, alice);
        const storageContract = await contractFactory.deploy();
        await storageContract.deployed();
        // Tests transient storage, see contract code for details.
        await expect(storageContract.testTransientStore()).toBeAccepted([]);
        // Checks that transient storage is cleaned up after each tx.
        await expect(storageContract.assertTValue(0)).toBeAccepted([]);
    });

    test('Should check code oracle works', async () => {
        // Deploy contract that calls CodeOracle.
        const artifact = require(`${
            testMaster.environment().pathToHome
        }/etc/contracts-test-data/artifacts-zk/contracts/precompiles/precompiles.sol/Precompiles.json`);
        const contractFactory = new zksync.ContractFactory(artifact.abi, artifact.bytecode, alice);
        const contract = await contractFactory.deploy();
        await contract.deployed();

        // Check that CodeOracle can decommit code of just deployed contract.
        const versionedHash = zksync.utils.hashBytecode(artifact.bytecode);
        const expectedBytecodeHash = ethers.utils.keccak256(artifact.bytecode);

        await expect(contract.callCodeOracle(versionedHash, expectedBytecodeHash)).toBeAccepted([]);
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});

async function invalidBytecodeTestTransaction(
    provider: zksync.Provider,
    factoryDeps: Uint8Array[]
): Promise<ethers.providers.TransactionRequest> {
    const chainId = (await provider.getNetwork()).chainId;

    const gasPrice = await provider.getGasPrice();
    const address = zksync.Wallet.createRandom().address;
    const tx: ethers.providers.TransactionRequest = {
        to: address,
        from: address,
        nonce: 0,

        gasLimit: ethers.BigNumber.from(300000),

        data: '0x',
        value: 0,
        chainId,

        type: 113,

        maxPriorityFeePerGas: gasPrice,
        maxFeePerGas: gasPrice,

        customData: {
            gasPerPubdata: zksync.utils.DEFAULT_GAS_PER_PUBDATA_LIMIT,
            factoryDeps,
            customSignature: new Uint8Array(17)
        }
    };

    return tx;
}
