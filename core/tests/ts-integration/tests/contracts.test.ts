/**
 * Generic tests checking the deployed smart contract behavior.
 *
 * Note: if you are going to write multiple tests checking specific topic (e.g. `CREATE2` behavior or something like this),
 * consider creating a separate suite.
 * Let's try to keep only relatively simple and self-contained tests here.
 */

import { TestMaster } from '../src';
import { deployContract, getDeploymentNonce, getAccountNonce, getTestContract, scaledGasPrice } from '../src/helpers';
import { shouldOnlyTakeFee } from '../src/modifiers/balance-checker';

import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';
import * as elliptic from 'elliptic';
import { RetryProvider } from '../src/retry-provider';
import { waitForNewL1Batch } from 'utils';

const SECONDS = 1000;
jest.setTimeout(400 * SECONDS);

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
    let expensiveContract: zksync.Contract;

    beforeAll(() => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
    });

    test('Should deploy & call a contract', async () => {
        counterContract = await deployContract(alice, contracts.counter, []);
        const feeCheck = await shouldOnlyTakeFee(alice);

        // Change the storage slot and ensure it actually changes.
        await expect(counterContract.get()).resolves.toEqual(0n);
        await expect(counterContract.increment(42)).toBeAccepted([feeCheck]);
        await expect(counterContract.get()).resolves.toEqual(42n);
    });

    test('Should deploy contract with a constructor', async () => {
        const contract1 = await deployContract(alice, contracts.constructor, [2, 3, false]);
        await expect(contract1.get()).resolves.toEqual(2n * 3n);

        const contract2 = await deployContract(alice, contracts.constructor, [5, 10, false]);
        await expect(contract2.get()).resolves.toEqual(5n * 10n);
    });

    test('Should deploy contract with create', async () => {
        const contractFactory = new zksync.ContractFactory(contracts.create.abi, contracts.create.bytecode, alice);
        const nonce = await alice.getDeploymentNonce();
        const accountNonce = await alice.getNonce();
        // Not all Alice's transactions are deployments, but all deployments require a separate transaction (we don't use `Multicall3` etc.
        // to batch deployments).
        expect(accountNonce).toBeGreaterThanOrEqual(nonce);
        const blockNumber = await alice.provider.getBlockNumber();

        testMaster.reporter?.debug(`Nonces before deployment: deployment=${nonce}, account=${accountNonce}`);
        const contract = (await contractFactory.deploy({
            customData: {
                factoryDeps: [contracts.create.factoryDep]
            }
        })) as zksync.Contract;
        await contract.waitForDeployment();
        await expect(contract.getFooName()).resolves.toBe('Foo');

        const newNonce = await alice.getDeploymentNonce();
        expect(newNonce).toEqual(nonce + 1n);
        const contractAddress = await contract.getAddress();
        expect(contractAddress).toEqual(zksync.utils.createAddress(alice.address, nonce));

        // Check `getTransactionCount` for contracts
        let contractNonce = await alice.provider.getTransactionCount(contractAddress);
        expect(contractNonce).toEqual(1); // `Foo` is deployed in the constructor
        // Should also work using `NonceHolder.getDeploymentNonce()`
        const contractDeploymentNonce = await getDeploymentNonce(alice.provider, contractAddress);
        expect(contractDeploymentNonce).toEqual(1n);
        // The account nonce for contracts should always stay 0.
        expect(await getAccountNonce(alice.provider, contractAddress)).toEqual(0n);

        // Check retrospective `getTransactionCount` values
        expect(await alice.getNonce(blockNumber)).toEqual(accountNonce);
        const oldContractNonce = await alice.provider.getTransactionCount(contractAddress, blockNumber);
        expect(oldContractNonce).toEqual(0);

        // Deploy a salted contract from the factory.
        const salt = ethers.getBytes('0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef');
        await expect(contract.deploySalted(salt)).resolves.toBeAccepted();
        const deployedCodeHash = zksync.utils.hashBytecode(contracts.create.factoryDep);
        const expectedSaltedAddress = zksync.utils.create2Address(contractAddress, deployedCodeHash, salt, '0x');
        expect(await contract.saltedContracts(salt)).toEqual(expectedSaltedAddress);

        contractNonce = await alice.provider.getTransactionCount(contractAddress);
        expect(contractNonce).toEqual(2);
        expect(await getAccountNonce(alice.provider, contractAddress)).toEqual(0n);
    });

    test('Should perform "expensive" contract calls', async () => {
        expensiveContract = await deployContract(alice, contracts.expensive, []);
        //  Check that the transaction that is too expensive would be rejected by the API server.
        await expect(expensiveContract.expensive(15000)).toBeRejected();
    });

    test('Should perform underpriced "expensive" contract calls', async () => {
        //  Check that processable transaction may fail with "out of gas" error.
        // To do so, we estimate gas for arg "1" and supply it to arg "20".
        // This guarantees that transaction won't fail during verification.
        const lowGasLimit = await expensiveContract.expensive.estimateGas(1);
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

        const gasPrice = await scaledGasPrice(alice);
        const infiniteLoop = await deployContract(alice, contracts.infinite, []);

        // Test eth_call first
        // TODO: provide a proper error for transactions that consume too much gas.
        // await expect(infiniteLoop.callStatic.infiniteLoop()).toBeRejected('cannot estimate transaction: out of gas');
        // ...and then an actual transaction
        await expect(infiniteLoop.infiniteLoop({ gasLimit: 1_000_000, gasPrice })).toBeReverted([]);
    });

    test('Should test reverting storage logs', async () => {
        // In this test we check that if transaction reverts, it rolls back the storage slots.
        const prevValue = await counterContract.get();
        const gasPrice = await scaledGasPrice(alice);

        // We manually provide a gas limit and gas price, since otherwise the exception would be thrown
        // while querying zks_estimateFee.
        await expect(
            counterContract.incrementWithRevert(5, true, {
                gasLimit: 5000000,
                gasPrice
            })
        ).toBeReverted();

        // The tx has been reverted, so the value Should not have been changed:
        const newValue = await counterContract.get();
        expect(newValue).toEqual(prevValue); // The counter has changed despite the revert
    });

    test('Should not allow invalid constructor calldata', async () => {
        const randomWrongArgs = [12, 12, true];
        await expect(deployContract(alice, contracts.counter, randomWrongArgs)).toBeRejected(
            'incorrect number of arguments to constructor'
        );
    });

    test('Should not allow invalid contract bytecode', async () => {
        // In this test we ensure that bytecode validity is checked by server.

        // Helpers to interact with the RPC API directly.
        const send = (tx: any) => alice.provider.send('eth_sendRawTransaction', [zksync.utils.serializeEip712(tx)]);
        const call = (tx: any) => alice.provider.send('eth_call', [alice.provider.getRpcTransaction(tx)]);
        const estimateGas = (tx: any) => alice.provider.send('eth_estimateGas', [alice.provider.getRpcTransaction(tx)]);
        // Prepares an invalid serialized transaction with the bytecode of provided length.
        const invalidTx = (length: number) => invalidBytecodeTestTransaction(alice.provider, [new Uint8Array(length)]);

        const txWithUnchunkableBytecode = await invalidTx(17);
        const unchunkableError = 'Bytecode length is not divisible by 32';
        await expect(send(txWithUnchunkableBytecode)).toBeRejected(unchunkableError);

        /*
        Ethers v6 error handling is not capable of handling this format of messages.
        See: https://github.com/ethers-io/ethers.js/blob/main/src.ts/providers/provider-jsonrpc.ts#L976
        {
          code: 3,
          message: 'Failed to serialize transaction: factory dependency #0 is invalid: Bytecode length is not divisible by 32'
        }
         */
        await expect(call(txWithUnchunkableBytecode)).toBeRejected(/*unchunkableError*/);
        await expect(estimateGas(txWithUnchunkableBytecode)).toBeRejected(/*unchunkableError*/);

        const txWithBytecodeWithEvenChunks = await invalidTx(64);
        const evenChunksError = 'Bytecode has even number of 32-byte words';
        await expect(send(txWithBytecodeWithEvenChunks)).toBeRejected(evenChunksError);

        /*
        {
          code: 3,
          message: 'Failed to serialize transaction: factory dependency #0 is invalid: Bytecode has even number of 32-byte words'
        }
         */
        await expect(call(txWithBytecodeWithEvenChunks)).toBeRejected(/*evenChunksError*/);
        await expect(estimateGas(txWithBytecodeWithEvenChunks)).toBeRejected(/*evenChunksError*/);

        const longBytecodeLen = zksync.utils.MAX_BYTECODE_LEN_BYTES + 32;
        const txWithTooLongBytecode = await invalidTx(longBytecodeLen);
        const tooLongBytecodeError = `Bytecode too long: ${longBytecodeLen} bytes, while max ${zksync.utils.MAX_BYTECODE_LEN_BYTES} allowed`;
        await expect(send(txWithTooLongBytecode)).toBeRejected(tooLongBytecodeError);
        /*
        {
          code: 3,
          message: 'Failed to serialize transaction: factory dependency #0 is invalid: Bytecode too long: 2097152 bytes, while max 2097120 allowed'
        }
         */
        await expect(call(txWithTooLongBytecode)).toBeRejected(/*tooLongBytecodeError*/);
        await expect(estimateGas(txWithTooLongBytecode)).toBeRejected(/*tooLongBytecodeError*/);
    });

    test('Should interchangeably use ethers for eth calls', async () => {
        // In this test we make sure that we can use `ethers` `Contract` object and provider
        // to do an `eth_Call` and send transactions to ZKsync contract.
        // This check is important to ensure that external apps do not have to use our SDK and
        // can keep using `ethers` on their side.

        const rpcAddress = testMaster.environment().l2NodeUrl;
        const provider = new RetryProvider(rpcAddress);
        const wallet = new ethers.Wallet(alice.privateKey, provider);
        const ethersBasedContract = new ethers.Contract(
            await counterContract.getAddress(),
            counterContract.interface,
            wallet
        );

        const oldValue = await ethersBasedContract.get();
        await expect(ethersBasedContract.increment(1)).toBeAccepted([]);
        await expect(ethersBasedContract.get()).resolves.toEqual(oldValue + 1n);
    });

    test('Should check that eth_call works with custom block tags', async () => {
        // Retrieve value normally.
        counterContract = await deployContract(alice, contracts.counter, []);
        const counterValue = await counterContract.get();

        // Check current block tag.
        await expect(counterContract.get.staticCall({ blockTag: 'pending' })).resolves.toEqual(counterValue);

        /*
        Ethers v6 error handling is not capable of handling this format of messages.
        See: https://github.com/ethers-io/ethers.js/blob/main/src.ts/providers/provider-jsonrpc.ts#L976
        {
          "code": -32602,
          "message": "Block with such an ID doesn't exist yet"
        }
         */
        // Block from the future.
        await expect(counterContract.get.staticCall({ blockTag: 1000000000 }))
            .toBeRejected
            //"Block with such an ID doesn't exist yet"
            ();

        // Genesis block
        await expect(counterContract.get.staticCall({ blockTag: 0 })).toBeRejected('could not decode result data');
    });

    test('Should correctly process msg.value inside constructor and in ethCall', async () => {
        const value = 1n;

        // Check that value provided to the constructor is processed.
        const contextContract = await deployContract(alice, contracts.context, [], undefined, { value });
        await expect(contextContract.valueOnCreate()).resolves.toEqual(value);

        // Check that value provided to `eth_Call` is processed.
        // This call won't return anything, but will throw if it'll result in a revert.
        await contextContract.requireMsgValue.staticCall(value, {
            value
        });
    });

    test('Should return correct error during fee estimation', async () => {
        const errorContract = await deployContract(alice, contracts.error, []);

        /*
         {
           "code": 3,
           "message": "execution reverted: longlonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglong",
           "data": "0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000c86c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e676c6f6e67000000000000000000000000000000000000000000000000"
         }
         */
        await expect(errorContract.require_long.estimateGas()).toBeRevertedEstimateGas(/*'longlonglong'*/);
        await expect(errorContract.require_long()).toBeRevertedEthCall(/*'longlonglong'*/);
        await expect(errorContract.new_error.estimateGas()).toBeRevertedEstimateGas(
            undefined,
            '0x157bea60000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000046461746100000000000000000000000000000000000000000000000000000000'
        );
        // execution reverted: TestError(uint256,uint256,uint256,string)
        await expect(errorContract.new_error.staticCall())
            .toBeRevertedEthCall
            /*
            undefined,
            '0x157bea60000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000046461746100000000000000000000000000000000000000000000000000000000'
            */
            ();
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
        const deploymentBlock = await contextContract.deploymentTransaction()!.blockNumber!;
        const deploymentBlockInfo = await alice.provider.getBlock(deploymentBlock);
        // If batch was not sealed, its number may not be present in the receipt.
        const deploymentl1Batch = deploymentBlockInfo.l1BatchNumber ?? (await alice.provider.getL1BatchNumber()) + 1;

        // Check that block gas limit is correct.
        const blockGasLimit = await contextContract.getBlockGasLimit({ blockTag: 'pending' });
        expect(blockGasLimit).toBeGreaterThan(0n);

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
        expect(initialL1Batch).toBeGreaterThanOrEqual(deploymentl1Batch);

        // Wait till the new L1 batch is created.
        await waitForNewL1Batch(alice);

        // Now we're sure that a new L1 batch is created, we may check the new properties.
        const newL1Batch = await contextContract.getBlockNumber({
            blockTag: 'pending'
        });
        const newTimestamp = await contextContract.getBlockTimestamp({
            blockTag: 'pending'
        });

        expect(newL1Batch).toBeGreaterThan(initialL1Batch); // New L1 batch number must be strictly greater
        expect(newTimestamp).toBeGreaterThanOrEqual(initialTimestamp); // New timestamp must not be less than previous one

        // And finally check block properties for the actual contract execution (not `eth_call`).
        const acceptedBlockLag = 20n;
        const acceptedTimestampLag = 600n;
        await expect(contextContract.checkBlockNumber(newL1Batch, newL1Batch + acceptedBlockLag)).toBeAccepted([]);
        // `newTimestamp` was received from the API, so actual timestamp in the state keeper may be lower.
        // This is why we use `initialTimestamp` here.
        await expect(
            contextContract.checkBlockTimestamp(initialTimestamp, initialTimestamp + acceptedTimestampLag)
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
        const bytecode = ethers.hexlify(ethers.randomBytes(BYTECODE_LEN));

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
        const digest = ethers.getBytes(ethers.keccak256(message));
        const signature = ec.sign(digest, privateKey);

        const publicKeyHex =
            '0x1ccbe91c075fc7f4f033bfa248db8fccd3565de94bbfb12f3c59ff46c271bf83ce4014c68811f9a21a1fdb2c0e6113e06db7ca93b7404e78dc7ccd5ca89a4ca9';

        // Check that verification succeeds.
        const res = await alice.provider.call({
            to: '0x0000000000000000000000000000000000000100',
            data: ethers.concat([
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
            data: ethers.concat([
                digest,
                '0x' + signature.r.toString('hex'),
                '0x' + signature.s.toString('hex'),
                publicKeyHex
            ])
        });
        const receipt = await tx.wait();
        expect(receipt.status).toEqual(1);
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});

async function invalidBytecodeTestTransaction(
    provider: zksync.Provider,
    factoryDeps: Uint8Array[]
): Promise<ethers.TransactionRequest> {
    const chainId = (await provider.getNetwork()).chainId;

    const gasPrice = await provider.getGasPrice();
    const address = zksync.Wallet.createRandom().address;
    return {
        to: address,
        from: address,
        nonce: 0,
        gasLimit: 300000n,
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
}
