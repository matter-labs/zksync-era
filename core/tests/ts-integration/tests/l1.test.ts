/**
 * This suite contains tests checking the interaction with L1.
 *
 * !WARN! Tests that interact with L1 may be very time consuming on stage.
 * Please only do the minimal amount of actions to test the behavior (e.g. no unnecessary deposits/withdrawals
 * and waiting for the block finalization).
 */
import { TestMaster } from '../src/index';
import * as zksync from 'zksync-web3';
import * as ethers from 'ethers';
import { deployContract, getTestContract, scaledGasPrice, waitForNewL1Batch } from '../src/helpers';
import { getHashedL2ToL1Msg, L1_MESSENGER, L1_MESSENGER_ADDRESS } from 'zksync-web3/build/src/utils';

const SYSTEM_CONFIG = require(`${process.env.ZKSYNC_HOME}/contracts/SystemConfig.json`);

const contracts = {
    counter: getTestContract('Counter'),
    errors: getTestContract('SimpleRequire'),
    context: getTestContract('Context'),
    writesAndMessages: getTestContract('WritesAndMessages')
};

// Sane amount of L2 gas enough to process a transaction.
const DEFAULT_L2_GAS_LIMIT = 5000000;

describe('Tests for L1 behavior', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;

    let counterContract: zksync.Contract;
    let contextContract: zksync.Contract;
    let errorContract: zksync.Contract;

    beforeAll(() => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
    });

    test('Should deploy required contracts', async () => {
        // We will need to call several different contracts, so it's better to deploy all them
        // as a separate step.
        counterContract = await deployContract(alice, contracts.counter, []);
        contextContract = await deployContract(alice, contracts.context, []);
        errorContract = await deployContract(alice, contracts.errors, []);
    });

    test('Should request L1 execute', async () => {
        const calldata = counterContract.interface.encodeFunctionData('increment', ['1']);
        const gasPrice = scaledGasPrice(alice);

        await expect(
            alice.requestExecute({
                contractAddress: counterContract.address,
                calldata,
                overrides: {
                    gasPrice
                }
            })
        ).toBeAccepted([]);
    });

    test('Should request L1 execute with msg.value', async () => {
        const l2Value = 10;
        const calldata = contextContract.interface.encodeFunctionData('requireMsgValue', [l2Value]);
        const gasPrice = scaledGasPrice(alice);

        await expect(
            alice.requestExecute({
                contractAddress: contextContract.address,
                calldata,
                l2Value,
                overrides: {
                    gasPrice
                }
            })
        ).toBeAccepted([]);
    });

    test('Should fail requested L1 execute', async () => {
        const calldata = errorContract.interface.encodeFunctionData('require_short', []);
        const gasPrice = scaledGasPrice(alice);

        await expect(
            alice.requestExecute({
                contractAddress: errorContract.address,
                calldata,
                l2GasLimit: DEFAULT_L2_GAS_LIMIT,
                overrides: {
                    gasPrice
                }
            })
        ).toBeReverted([]);
    });

    test('Should send L2->L1 messages', async () => {
        if (testMaster.isFastMode()) {
            return;
        }
        const contract = new zksync.Contract(L1_MESSENGER_ADDRESS, L1_MESSENGER, alice);

        // Send message to L1 and wait until it gets there.
        const message = ethers.utils.toUtf8Bytes('Some L2->L1 message');
        const tx = await contract.sendToL1(message);
        const receipt = await tx.waitFinalize();

        // Get the proof for the sent message from the server, expect it to exist.
        const l2ToL1LogIndex = receipt.l2ToL1Logs.findIndex(
            (log: zksync.types.L2ToL1Log) => log.sender == L1_MESSENGER_ADDRESS
        );
        const msgProof = await alice.provider.getLogProof(tx.hash, l2ToL1LogIndex);
        expect(msgProof).toBeTruthy();

        // Ensure that received proof matches the provided root hash.
        const { id, proof, root } = msgProof!;
        const accumutatedRoot = calculateAccumulatedRoot(alice.address, message, receipt.l1BatchTxIndex, id, proof);
        expect(accumutatedRoot).toBe(root);

        // Ensure that provided proof is accepted by the main zkSync contract.
        const zkSyncContract = await alice.getMainContract();
        const acceptedByContract = await zkSyncContract.proveL2MessageInclusion(
            receipt.l1BatchNumber,
            id,
            {
                txNumberInBlock: receipt.l1BatchTxIndex,
                sender: alice.address,
                data: message
            },
            proof
        );
        expect(acceptedByContract).toBeTruthy();
    });

    test('Should check max L2 gas limit for priority txs', async () => {
        const gasPrice = scaledGasPrice(alice);

        const l2GasLimit = maxL2GasLimitForPriorityTxs();

        // Check that the request with higher `gasLimit` fails.
        let priorityOpHandle = await alice.requestExecute({
            contractAddress: alice.address,
            calldata: '0x',
            l2GasLimit: l2GasLimit + 1,
            overrides: {
                gasPrice,
                gasLimit: 600_000
            }
        });
        let thrown = false;
        try {
            await priorityOpHandle.waitL1Commit();
        } catch {
            thrown = true;
        }
        expect(thrown).toBeTruthy();

        // Check that the request with `gasLimit` succeeds.
        priorityOpHandle = await alice.requestExecute({
            contractAddress: alice.address,
            calldata: '0x',
            l2GasLimit,
            overrides: {
                gasPrice
            }
        });
        await priorityOpHandle.waitL1Commit();
    });

    test('Should revert l1 tx with too many initial storage writes', async () => {
        // This test sends a transaction that consumes a lot of L2 ergs and so may be too expensive for
        // stage environment. That's why we only test it on the local environment (which includes CI).
        if (!testMaster.isLocalHost()) {
            return;
        }

        const contract = await deployContract(alice, contracts.writesAndMessages, []);
        // The circuit allows us to have ~4700 initial writes for an L1 batch.
        // We check that we will run out of gas if we do a bit smaller amount of writes.
        const calldata = contract.interface.encodeFunctionData('writes', [0, 4500, 1]);
        const gasPrice = scaledGasPrice(alice);

        const l2GasLimit = maxL2GasLimitForPriorityTxs();

        const priorityOpHandle = await alice.requestExecute({
            contractAddress: contract.address,
            calldata,
            l2GasLimit,
            overrides: {
                gasPrice
            }
        });
        // The request should be accepted on L1.
        await priorityOpHandle.waitL1Commit();
        // The L2 tx should revert.
        await expect(priorityOpHandle).toBeReverted();
    });

    test('Should revert l1 tx with too many repeated storage writes', async () => {
        // This test sends a transaction that consumes a lot of L2 ergs and so may be too expensive for
        // stage environment. That's why we only test it on the local environment (which includes CI).
        if (!testMaster.isLocalHost()) {
            return;
        }

        const contract = await deployContract(alice, contracts.writesAndMessages, []);
        // The circuit allows us to have ~7500 repeated writes for an L1 batch.
        // We check that we will run out of gas if we do a bit smaller amount of writes.
        // In order for writes to be repeated we should firstly write to the keys initially.
        const initialWritesInOneTx = 500;
        const repeatedWritesInOneTx = 8500;
        const gasLimit = await contract.estimateGas.writes(0, initialWritesInOneTx, 1);

        let proms = [];
        const nonce = await alice.getNonce();
        for (let i = 0; i < repeatedWritesInOneTx / initialWritesInOneTx; ++i) {
            proms.push(
                contract.writes(i * initialWritesInOneTx, initialWritesInOneTx, 1, { gasLimit, nonce: nonce + i })
            );
        }
        const handles = await Promise.all(proms);
        for (const handle of handles) {
            await handle.wait();
        }
        await waitForNewL1Batch(alice);

        const calldata = contract.interface.encodeFunctionData('writes', [0, repeatedWritesInOneTx, 2]);
        const gasPrice = scaledGasPrice(alice);

        const l2GasLimit = maxL2GasLimitForPriorityTxs();

        const priorityOpHandle = await alice.requestExecute({
            contractAddress: contract.address,
            calldata,
            l2GasLimit,
            overrides: {
                gasPrice
            }
        });
        // The request should be accepted on L1.
        await priorityOpHandle.waitL1Commit();
        // The L2 tx should revert.
        await expect(priorityOpHandle).toBeReverted();
    });

    test('Should revert l1 tx with too many l2 to l1 messages', async () => {
        // This test sends a transaction that consumes a lot of L2 ergs and so may be too expensive for
        // stage environment. That's why we only test it on the local environment (which includes CI).
        if (!testMaster.isLocalHost()) {
            return;
        }

        const contract = await deployContract(alice, contracts.writesAndMessages, []);
        // The circuit allows us to have 512 L2->L1 logs for an L1 batch.
        // We check that we will run out of gas if we send a bit smaller amount of L2->L1 logs.
        const calldata = contract.interface.encodeFunctionData('l2_l1_messages', [1000]);
        const gasPrice = scaledGasPrice(alice);

        const l2GasLimit = maxL2GasLimitForPriorityTxs();

        const priorityOpHandle = await alice.requestExecute({
            contractAddress: contract.address,
            calldata,
            l2GasLimit,
            overrides: {
                gasPrice
            }
        });
        // The request should be accepted on L1.
        await priorityOpHandle.waitL1Commit();
        // The L2 tx should revert.
        await expect(priorityOpHandle).toBeReverted();
    });

    test('Should revert l1 tx with too big l2 to l1 message', async () => {
        // This test sends a transaction that consumes a lot of L2 ergs and so may be too expensive for
        // stage environment. That's why we only test it on the local environment (which includes CI).
        if (!testMaster.isLocalHost()) {
            return;
        }

        const contract = await deployContract(alice, contracts.writesAndMessages, []);
        const MAX_PUBDATA_PER_BATCH = ethers.BigNumber.from(SYSTEM_CONFIG['MAX_PUBDATA_PER_BATCH']);
        // We check that we will run out of gas if we send a bit
        // smaller than `MAX_PUBDATA_PER_BATCH` amount of pubdata in a single tx.
        const calldata = contract.interface.encodeFunctionData('big_l2_l1_message', [
            MAX_PUBDATA_PER_BATCH.mul(9).div(10)
        ]);
        const gasPrice = scaledGasPrice(alice);

        const l2GasLimit = maxL2GasLimitForPriorityTxs();

        const priorityOpHandle = await alice.requestExecute({
            contractAddress: contract.address,
            calldata,
            l2GasLimit,
            overrides: {
                gasPrice
            }
        });
        // The request should be accepted on L1.
        await priorityOpHandle.waitL1Commit();
        // The L2 tx should revert.
        await expect(priorityOpHandle).toBeReverted();
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});

/**
 * Recreates the root hash of the merkle tree based on the provided proof.
 */
function calculateAccumulatedRoot(
    address: string,
    message: Uint8Array,
    l1BatchTxIndex: number,
    id: number,
    proof: string[]
): string {
    let accumutatedRoot = getHashedL2ToL1Msg(address, message, l1BatchTxIndex);

    let idCopy = id;
    for (const elem of proof) {
        const bytes =
            (idCopy & 1) == 0
                ? new Uint8Array([...ethers.utils.arrayify(accumutatedRoot), ...ethers.utils.arrayify(elem)])
                : new Uint8Array([...ethers.utils.arrayify(elem), ...ethers.utils.arrayify(accumutatedRoot)]);

        accumutatedRoot = ethers.utils.keccak256(bytes);
        idCopy /= 2;
    }
    return accumutatedRoot;
}

function maxL2GasLimitForPriorityTxs(): number {
    // Find maximum `gasLimit` that satisfies `txBodyGasLimit <= CONTRACTS_PRIORITY_TX_MAX_GAS_LIMIT`
    // using binary search.
    let maxGasBodyLimit = +process.env.CONTRACTS_PRIORITY_TX_MAX_GAS_LIMIT!;

    const overhead = getOverheadForTransaction(
        ethers.BigNumber.from(maxGasBodyLimit),
        ethers.BigNumber.from(zksync.utils.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT),
        // We can just pass 0 as `encodingLength` because `overheadForPublicData` and `overheadForGas`
        // will be greater than `overheadForLength` for large `gasLimit`.
        ethers.BigNumber.from(0)
    );
    return maxGasBodyLimit + overhead;
}

function getOverheadForTransaction(
    bodyGasLimit: ethers.BigNumber,
    gasPricePerPubdata: ethers.BigNumber,
    encodingLength: ethers.BigNumber
): number {
    const BATCH_OVERHEAD_L2_GAS = ethers.BigNumber.from(SYSTEM_CONFIG['BATCH_OVERHEAD_L2_GAS']);
    const L1_GAS_PER_PUBDATA_BYTE = ethers.BigNumber.from(SYSTEM_CONFIG['L1_GAS_PER_PUBDATA_BYTE']);
    const BATCH_OVERHEAD_L1_GAS = ethers.BigNumber.from(SYSTEM_CONFIG['BATCH_OVERHEAD_L1_GAS']);
    const BATCH_OVERHEAD_PUBDATA = BATCH_OVERHEAD_L1_GAS.div(L1_GAS_PER_PUBDATA_BYTE);

    const MAX_TRANSACTIONS_IN_BATCH = ethers.BigNumber.from(SYSTEM_CONFIG['MAX_TRANSACTIONS_IN_BATCH']);
    const BOOTLOADER_TX_ENCODING_SPACE = ethers.BigNumber.from(SYSTEM_CONFIG['BOOTLOADER_TX_ENCODING_SPACE']);
    // TODO (EVM-67): possibly charge overhead for pubdata
    // const MAX_PUBDATA_PER_BATCH = ethers.BigNumber.from(SYSTEM_CONFIG['MAX_PUBDATA_PER_BATCH']);
    const L2_TX_MAX_GAS_LIMIT = ethers.BigNumber.from(SYSTEM_CONFIG['L2_TX_MAX_GAS_LIMIT']);

    const maxBlockOverhead = BATCH_OVERHEAD_L2_GAS.add(BATCH_OVERHEAD_PUBDATA.mul(gasPricePerPubdata));

    // The overhead from taking up the transaction's slot
    const txSlotOverhead = ceilDiv(maxBlockOverhead, MAX_TRANSACTIONS_IN_BATCH);
    let blockOverheadForTransaction = txSlotOverhead;

    // The overhead for occupying the bootloader memory can be derived from encoded_len
    const overheadForLength = ceilDiv(encodingLength.mul(maxBlockOverhead), BOOTLOADER_TX_ENCODING_SPACE);
    if (overheadForLength.gt(blockOverheadForTransaction)) {
        blockOverheadForTransaction = overheadForLength;
    }

    // The overhead for possible published public data
    // TODO (EVM-67): possibly charge overhead for pubdata
    // let maxPubdataInTx = ceilDiv(bodyGasLimit, gasPricePerPubdata);
    // let overheadForPublicData = ceilDiv(maxPubdataInTx.mul(maxBlockOverhead), MAX_PUBDATA_PER_BATCH);
    // if (overheadForPublicData.gt(blockOverheadForTransaction)) {
    //     blockOverheadForTransaction = overheadForPublicData;
    // }

    // The overhead for gas that could be used to use single-instance circuits
    let overheadForSingleInstanceCircuits = ceilDiv(bodyGasLimit.mul(maxBlockOverhead), L2_TX_MAX_GAS_LIMIT);
    if (overheadForSingleInstanceCircuits.gt(blockOverheadForTransaction)) {
        blockOverheadForTransaction = overheadForSingleInstanceCircuits;
    }

    return blockOverheadForTransaction.toNumber();
}

function ceilDiv(a: ethers.BigNumber, b: ethers.BigNumber): ethers.BigNumber {
    return a.add(b.sub(1)).div(b);
}
