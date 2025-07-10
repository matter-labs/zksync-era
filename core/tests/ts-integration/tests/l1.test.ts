/**
 * This suite contains tests checking the interaction with L1.
 *
 * !WARN! Tests that interact with L1 may be very time-consuming on stage.
 * Please only do the minimal amount of actions to test the behavior (e.g. no unnecessary deposits/withdrawals
 * and waiting for the block finalization).
 */
import { TestMaster } from "../src";
import * as zksync from "zksync-ethers";
import * as ethers from "ethers";
import {
  bigIntMax,
  deployContract,
  getTestContract,
  scaledGasPrice,
  waitForL2ToL1LogProof,
  waitForNewL1Batch,
} from "../src/helpers";
import {
  L1_MESSENGER,
  L1_MESSENGER_ADDRESS,
  REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT,
} from "zksync-ethers/build/utils";

// Sane amount of L2 gas enough to process a transaction.
const DEFAULT_L2_GAS_LIMIT = 5000000;

describe("Tests for L1 behavior", () => {
  let contracts: any;

  let testMaster: TestMaster;
  let alice: zksync.Wallet;

  let counterContract: zksync.Contract;
  let contextContract: zksync.Contract;
  let errorContract: zksync.Contract;

  let isETHBasedChain: boolean;
  let expectedL2Costs: bigint;

  beforeAll(() => {
    contracts = {
      counter: getTestContract("Counter"),
      errors: getTestContract("SimpleRequire"),
      context: getTestContract("Context"),
      writesAndMessages: getTestContract("WritesAndMessages"),
    };
    testMaster = TestMaster.getInstance(__filename);
    alice = testMaster.mainAccount();
  });

  test("Should deploy required contracts", async () => {
    // We will need to call several different contracts, so it's better to deploy all them
    // as a separate step.
    counterContract = await deployContract(alice, contracts.counter, []);
    contextContract = await deployContract(alice, contracts.context, []);
    errorContract = await deployContract(alice, contracts.errors, []);
  });

  test("Should provide allowance to shared bridge, if base token is not ETH", async () => {
    const baseTokenAddress = testMaster.environment().baseToken.l1Address;
    isETHBasedChain = baseTokenAddress == zksync.utils.ETH_ADDRESS_IN_CONTRACTS;
    if (!isETHBasedChain) {
      const baseTokenDetails = testMaster.environment().baseToken;
      const maxAmount = await alice.getBalanceL1(baseTokenDetails.l1Address);
      await (
        await alice.approveERC20(baseTokenDetails.l1Address, maxAmount)
      ).wait();
    }
  });

  test("Should calculate l2 base cost, if base token is not ETH", async () => {
    const gasPrice = await scaledGasPrice(alice);
    if (!isETHBasedChain) {
      expectedL2Costs =
        ((await alice.getBaseCost({
          gasLimit: maxL2GasLimitForPriorityTxs(
            testMaster.environment().priorityTxMaxGasLimit,
          ),
          gasPerPubdataByte: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT,
          gasPrice,
        })) *
          140n) /
        100n;
    }
  });

  test("Should request L1 execute", async () => {
    const calldata = counterContract.interface.encodeFunctionData("increment", [
      "1",
    ]);
    const gasPrice = await scaledGasPrice(alice);

    await expect(
      alice.requestExecute({
        contractAddress: await counterContract.getAddress(),
        calldata,
        mintValue: isETHBasedChain ? 0n : expectedL2Costs,
        overrides: {
          gasPrice,
        },
      }),
    ).toBeAccepted([]);
  });

  test("Should request L1 execute with msg.value", async () => {
    const l2Value = 10;
    const calldata = contextContract.interface.encodeFunctionData(
      "requireMsgValue",
      [l2Value],
    );
    const gasPrice = await scaledGasPrice(alice);

    await expect(
      alice.requestExecute({
        contractAddress: await contextContract.getAddress(),
        calldata,
        l2Value,
        mintValue: isETHBasedChain ? 0n : expectedL2Costs,
        overrides: {
          gasPrice,
        },
      }),
    ).toBeAccepted([]);
  });

  test("Should fail requested L1 execute", async () => {
    const calldata = errorContract.interface.encodeFunctionData(
      "require_short",
      [],
    );
    const gasPrice = await scaledGasPrice(alice);

    await expect(
      alice.requestExecute({
        contractAddress: await errorContract.getAddress(),
        calldata,
        l2GasLimit: DEFAULT_L2_GAS_LIMIT,
        mintValue: isETHBasedChain ? 0n : expectedL2Costs,
        overrides: {
          gasPrice,
        },
      }),
    ).toBeReverted([]);
  });

  test("Should send L2->L1 messages", async () => {
    if (testMaster.isFastMode()) {
      return;
    }
    const contract = new zksync.Contract(
      L1_MESSENGER_ADDRESS,
      L1_MESSENGER,
      alice,
    );

    // Send message to L1 and wait until it gets there.
    const message = ethers.toUtf8Bytes("Some L2->L1 message");
    const tx = await contract.sendToL1(message);
    const receipt = await (
      await alice.provider.getTransaction(tx.hash)
    ).waitFinalize();

    // Get the proof for the sent message from the server, expect it to exist.
    const l2ToL1LogIndex = receipt.l2ToL1Logs.findIndex(
      (log: zksync.types.L2ToL1Log) => log.sender == L1_MESSENGER_ADDRESS,
    );
    await waitForL2ToL1LogProof(alice, receipt.blockNumber, tx.hash);
    const msgProof = await alice.provider.getLogProof(tx.hash, l2ToL1LogIndex);
    expect(msgProof).toBeTruthy();

    // Note, that if a chain uses gateway, its `root` will correspond to the root of the messages from the batch,
    // while the `proof` would contain both the proof for the leaf belonging to the batch and the batch belonging
    // to the gateway.
    const { id, proof } = msgProof!;

    // Ensure that provided proof is accepted by the main ZKsync contract.
    const chainContract = await alice.getMainContract();
    const acceptedByContract = await chainContract.proveL2MessageInclusion(
      receipt.l1BatchNumber!,
      id,
      {
        txNumberInBatch: receipt.l1BatchTxIndex!,
        sender: alice.address,
        data: message,
      },
      proof,
    );
    expect(acceptedByContract).toBeTruthy();
  });

  test("Should check max L2 gas limit for priority txs", async () => {
    const gasPrice = await scaledGasPrice(alice);
    const l2GasLimit = maxL2GasLimitForPriorityTxs(
      testMaster.environment().priorityTxMaxGasLimit,
    );

    // Check that the request with higher `gasLimit` fails.
    let priorityOpHandle = await alice.requestExecute({
      contractAddress: alice.address,
      calldata: "0x",
      l2GasLimit: l2GasLimit + 1n,
      mintValue: isETHBasedChain ? 0n : expectedL2Costs,
      overrides: {
        gasPrice,
        gasLimit: 600_000,
      },
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
      calldata: "0x",
      l2GasLimit,
      mintValue: isETHBasedChain ? 0n : expectedL2Costs,
      overrides: {
        gasPrice,
      },
    });
    await priorityOpHandle.waitL1Commit();
  });

  test("Should revert l1 tx with too many initial storage writes", async () => {
    // This test sends a transaction that consumes a lot of L2 ergs and so may be too expensive for
    // stage environment. That's why we only test it on the local environment (which includes CI).
    if (!testMaster.isLocalHost()) {
      return;
    }

    const contract = await deployContract(
      alice,
      contracts.writesAndMessages,
      [],
    );
    testMaster.reporter.debug(
      `Deployed 'writesAndMessages' contract at ${await contract.getAddress()}`,
    );
    // The circuit allows us to have ~4700 initial writes for an L1 batch.
    // We check that we will run out of gas if we do a bit smaller amount of writes.
    const calldata = contract.interface.encodeFunctionData(
      "writes",
      [0, 4500, 1],
    );
    const gasPrice = await scaledGasPrice(alice);

    const l2GasLimit = maxL2GasLimitForPriorityTxs(
      testMaster.environment().priorityTxMaxGasLimit,
    );

    const priorityOpHandle = await alice.requestExecute({
      contractAddress: await contract.getAddress(),
      calldata,
      l2GasLimit,
      mintValue: isETHBasedChain ? 0n : expectedL2Costs,
      overrides: {
        gasPrice,
      },
    });
    testMaster.reporter.debug(
      `Requested priority execution of transaction with big message: ${priorityOpHandle.hash}`,
    );
    // The request should be accepted on L1.
    await priorityOpHandle.waitL1Commit();
    testMaster.reporter.debug(
      `Request ${priorityOpHandle.hash} is accepted on L1`,
    );
    // The L2 tx should revert.
    await expect(priorityOpHandle).toBeReverted();
  });

  test("Should revert l1 tx with too many repeated storage writes", async () => {
    // This test sends a transaction that consumes a lot of L2 ergs and so may be too expensive for
    // stage environment. That's why we only test it on the local environment (which includes CI).
    if (!testMaster.isLocalHost()) {
      return;
    }

    const contract = await deployContract(
      alice,
      contracts.writesAndMessages,
      [],
    );
    testMaster.reporter.debug(
      `Deployed 'writesAndMessages' contract at ${await contract.getAddress()}`,
    );
    // The circuit allows us to have ~7500 repeated writes for an L1 batch.
    // We check that we will run out of gas if we do a bit smaller amount of writes.
    // In order for writes to be repeated we should firstly write to the keys initially.
    const initialWritesInOneTx = 500;
    const repeatedWritesInOneTx = 8500;
    const gasLimit = await contract.writes.estimateGas(
      0,
      initialWritesInOneTx,
      1,
    );

    let proms = [];
    const nonce = await alice.getNonce();
    testMaster.reporter.debug(`Obtained nonce: ${nonce}`);
    for (let i = 0; i < repeatedWritesInOneTx / initialWritesInOneTx; ++i) {
      proms.push(
        contract.writes(i * initialWritesInOneTx, initialWritesInOneTx, 1, {
          gasLimit,
          nonce: nonce + i,
        }),
      );
    }
    const handles = await Promise.all(proms);
    testMaster.reporter.debug(
      `Sent ${handles.length} L2 transactions with writes`,
    );
    for (const handle of handles) {
      await handle.wait();
    }
    await waitForNewL1Batch(alice);
    testMaster.reporter.debug("L1 batch sealed with write transactions");

    const calldata = contract.interface.encodeFunctionData("writes", [
      0,
      repeatedWritesInOneTx,
      2,
    ]);
    const gasPrice = await scaledGasPrice(alice);
    const l2GasLimit = maxL2GasLimitForPriorityTxs(
      testMaster.environment().priorityTxMaxGasLimit,
    );

    const priorityOpHandle = await alice.requestExecute({
      contractAddress: await contract.getAddress(),
      calldata,
      l2GasLimit,
      mintValue: isETHBasedChain ? 0n : expectedL2Costs,
      overrides: {
        gasPrice,
      },
    });
    testMaster.reporter.debug(
      `Requested priority execution of transaction with repeated storage writes on L1: ${priorityOpHandle.hash}`,
    );
    // The request should be accepted on L1.
    await priorityOpHandle.waitL1Commit();
    testMaster.reporter.debug(
      `Request ${priorityOpHandle.hash} is accepted on L1`,
    );
    // The L2 tx should revert.
    await expect(priorityOpHandle).toBeReverted();
  });

  test("Should revert l1 tx with too many l2 to l1 messages", async () => {
    // This test sends a transaction that consumes a lot of L2 ergs and so may be too expensive for
    // stage environment. That's why we only test it on the local environment (which includes CI).
    if (!testMaster.isLocalHost()) {
      return;
    }

    const contract = await deployContract(
      alice,
      contracts.writesAndMessages,
      [],
    );
    testMaster.reporter.debug(
      `Deployed 'writesAndMessages' contract at ${await contract.getAddress()}`,
    );
    // The circuit allows us to have 512 L2->L1 logs for an L1 batch.
    // We check that we will run out of gas if we send a bit smaller amount of L2->L1 logs.
    const calldata = contract.interface.encodeFunctionData("l2_l1_messages", [
      1000,
    ]);
    const gasPrice = await scaledGasPrice(alice);

    const l2GasLimit = maxL2GasLimitForPriorityTxs(
      testMaster.environment().priorityTxMaxGasLimit,
    );

    const priorityOpHandle = await alice.requestExecute({
      contractAddress: await contract.getAddress(),
      calldata,
      l2GasLimit,
      mintValue: isETHBasedChain ? 0n : expectedL2Costs,
      overrides: {
        gasPrice,
      },
    });
    testMaster.reporter.debug(
      `Requested priority execution of transaction with big message: ${priorityOpHandle.hash}`,
    );
    // The request should be accepted on L1.
    await priorityOpHandle.waitL1Commit();
    testMaster.reporter.debug(
      `Request ${priorityOpHandle.hash} is accepted on L1`,
    );
    // The L2 tx should revert.
    await expect(priorityOpHandle).toBeReverted();
  });

  test("Should revert l1 tx with too big l2 to l1 message", async () => {
    // This test sends a transaction that consumes a lot of L2 ergs and so may be too expensive for
    // stage environment. That's why we only test it on the local environment (which includes CI).
    if (!testMaster.isLocalHost()) {
      return;
    }

    const contract = await deployContract(
      alice,
      contracts.writesAndMessages,
      [],
    );
    testMaster.reporter.debug(
      `Deployed 'writesAndMessages' contract at ${contract.address}`,
    );

    const SYSTEM_CONFIG = require(
      `${testMaster.environment().pathToHome}/contracts/SystemConfig.json`,
    );
    const MAX_PUBDATA_PER_BATCH = BigInt(
      SYSTEM_CONFIG["PRIORITY_TX_PUBDATA_PER_BATCH"],
    );
    // We check that we will run out of gas if we send a bit
    // smaller than `MAX_PUBDATA_PER_BATCH` amount of pubdata in a single tx.
    const calldata = contract.interface.encodeFunctionData(
      "big_l2_l1_message",
      [(MAX_PUBDATA_PER_BATCH * 9n) / 10n],
    );
    const gasPrice = await scaledGasPrice(alice);

    const l2GasLimit = maxL2GasLimitForPriorityTxs(
      testMaster.environment().priorityTxMaxGasLimit,
    );

    const priorityOpHandle = await alice.requestExecute({
      contractAddress: await contract.getAddress(),
      calldata,
      l2GasLimit,
      mintValue: isETHBasedChain ? 0n : expectedL2Costs,
      overrides: {
        gasPrice,
      },
    });
    testMaster.reporter.debug(
      `Requested priority execution of transaction with big message: ${priorityOpHandle.hash}`,
    );
    // The request should be accepted on L1.
    await priorityOpHandle.waitL1Commit();
    testMaster.reporter.debug(
      `Request ${priorityOpHandle.hash} is accepted on L1`,
    );
    // The L2 tx should revert.
    await expect(priorityOpHandle).toBeReverted();
  });

  afterAll(async () => {
    await testMaster.deinitialize();
  });
});

function maxL2GasLimitForPriorityTxs(maxGasBodyLimit: bigint): bigint {
  // Find maximum `gasLimit` that satisfies `txBodyGasLimit <= CONTRACTS_PRIORITY_TX_MAX_GAS_LIMIT`
  // using binary search.
  const overhead = getOverheadForTransaction(
    // We can just pass 0 as `encodingLength` because the overhead for the transaction's slot
    // will be greater than `overheadForLength` for a typical transacction
    0n,
  );
  return maxGasBodyLimit + overhead;
}

function getOverheadForTransaction(encodingLength: bigint): bigint {
  const TX_SLOT_OVERHEAD_GAS = 10_000n;
  const TX_LENGTH_BYTE_OVERHEAD_GAS = 10n;

  return bigIntMax(
    TX_SLOT_OVERHEAD_GAS,
    TX_LENGTH_BYTE_OVERHEAD_GAS * encodingLength,
  );
}
