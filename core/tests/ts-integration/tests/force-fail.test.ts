/**
 * Operator force-fail for L1->L2 transactions (protocol v34). A priority op whose serial id is in
 * `force_failed_priority_ops` is failed by the bootloader before any user code runs; the rest is the
 * ordinary failure path (status log = 0, refund, priority queue advances).
 *
 * Runs only with RUN_FORCE_FAIL_TEST=1 and must run alone: it predicts the next priority op id, so a
 * concurrent suite's deposit would be force-failed instead. `runIntegrationTests` in
 * highlevel-test-tools runs it after the parallel suites finish.
 */
import { TestMaster } from '../src';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import {
    deployContract,
    getTestContract,
    scaledGasPrice,
    waitForL2ToL1LogProof,
    waitUntilBlockFinalized,
    maxL2GasLimitForPriorityTxs
} from '../src/helpers';
import { shouldChangeTokenBalances } from '../src/modifiers/balance-checker';
import { insertForceFailedPriorityOp, mainNodeDbUrl } from '../src/force-fail-db';
import { REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT } from 'zksync-ethers/build/utils';
import { Token } from '../src/types';
import { IZkSyncHyperchain } from 'zksync-ethers/build/typechain';

const testForceFail = process.env.RUN_FORCE_FAIL_TEST ? describe : describe.skip;

// Must exceed `state_keeper.force_fail.refresh_interval` (10 s by default): the state keeper only sees
// the mark on its next reload.
const FORCE_FAIL_MARK_SETTLE_MS = 15_000;
// Below the jest timeout so a stuck finalization fails with its own message.
const FINALIZATION_TIMEOUT_MS = 7 * 60 * 1000;
// Enough L2 gas for the priority ops below; skips gas estimation.
const DEFAULT_L2_GAS_LIMIT = 5_000_000;
const CREATED_BY = 'ts-integration/force-fail.test.ts';
// `VmRevertReason::Unknown` with empty selector and data.
const EMPTY_REVERT_REASON = 'Error function_selector = 0x, data = 0x';
// Key of the bootloader's second L2->L1 log for a force-failed tx: keccak256("zksync.bootloader.forceFailedL1TxLog").
const FORCE_FAILED_L1_TX_LOG_KEY = '0xc347406c856ed927ab49919146282a0c9feb316c83a07792fce67353bcf0e474';

interface ForceFailedRequest {
    id: bigint;
    handle: zksync.types.PriorityOpResponse;
    l2Hash: string;
    l2GasLimit: bigint;
    mintValue: bigint;
}

testForceFail('Force-failed L1->L2 transactions', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let bob: zksync.Wallet;
    let mainContract: IZkSyncHyperchain;
    let mainContractAddress: string;
    let dbUrl: string;
    let tokenDetails: Token;
    let isETHBasedChain: boolean;
    let counter: zksync.Contract;
    let controlHash: string;

    const mintValueForRequestExecute = async (gasPrice: bigint) => {
        if (isETHBasedChain) return 0n;
        const baseCost = await alice.getBaseCost({
            gasLimit: maxL2GasLimitForPriorityTxs(testMaster.environment().priorityTxMaxGasLimit),
            gasPerPubdataByte: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT,
            gasPrice
        });
        return (baseCost * 140n) / 100n;
    };

    function parseNewPriorityRequest(l1Receipt: ethers.TransactionReceipt) {
        for (const log of l1Receipt.logs) {
            if (!zksync.utils.isAddressEq(log.address, mainContractAddress)) continue;
            const parsed = zksync.utils.ZKSYNC_MAIN_ABI.parseLog({ topics: [...log.topics], data: log.data });
            if (parsed?.name === 'NewPriorityRequest') return parsed.args;
        }
        throw new Error(`NewPriorityRequest not emitted by ${mainContractAddress} in L1 tx ${l1Receipt.hash}`);
    }

    /** Marks the next priority op as force-failed, sends it with `send`, and returns what L1 recorded. */
    async function sendForceFailed(send: () => Promise<zksync.types.PriorityOpResponse>): Promise<ForceFailedRequest> {
        const expectedId: bigint = await mainContract.getTotalPriorityTxs();
        await insertForceFailedPriorityOp(dbUrl, expectedId, 'ts-integration force-fail test', CREATED_BY);
        await zksync.utils.sleep(FORCE_FAIL_MARK_SETTLE_MS);
        const handle = await send();
        const l1Receipt = await handle.waitL1Commit();
        const request = parseNewPriorityRequest(l1Receipt);
        // Otherwise a foreign priority op slipped in and the mark hit the wrong tx.
        expect(request.txId).toEqual(expectedId);
        return {
            id: expectedId,
            handle,
            l2Hash: request.txHash,
            l2GasLimit: request.transaction.gasLimit,
            mintValue: request.transaction.reserved[0]
        };
    }

    /** Checks the L2 side of a force-failed op; `refundBefore` is the recipient's base-token balance before it. */
    async function expectForceFailed(req: ForceFailedRequest, refundRecipient: zksync.Wallet, refundBefore: bigint) {
        const receipt = (await alice.provider.getTransactionReceipt(req.l2Hash))!;
        expect(receipt.status).toEqual(0);
        // The status log with value 0 is what `claimFailedDeposit` proves on L1; the marker log lets
        // anyone prove the failure was operator-requested.
        const bootloaderLogs = receipt.l2ToL1Logs
            .filter((l) => zksync.utils.isAddressEq(l.sender, zksync.utils.BOOTLOADER_FORMAL_ADDRESS))
            .map((l) => [l.key, l.value]);
        expect(bootloaderLogs).toEqual([
            [req.l2Hash, ethers.ZeroHash],
            [FORCE_FAILED_L1_TX_LOG_KEY, req.l2Hash]
        ]);
        // The whole gas limit is charged: the bootloader panics the execution frame and refunds nothing.
        expect(receipt.gasUsed).toEqual(req.l2GasLimit);
        const fee = receipt.gasUsed * receipt.gasPrice;

        // `forceFailed` is an extra camelCase field of `zks_getTransactionDetails`, untyped in the SDK.
        const details = (await alice.provider.getTransactionDetails(req.l2Hash)) as any;
        expect(details.forceFailed).toBe(true);
        expect(details.status).toEqual('failed');
        expect(details.isL1Originated).toBe(true);
        expect(BigInt(details.fee)).toEqual(fee);

        // No revert data and no call into the target: user code never ran.
        const trace = await alice.provider.send('debug_traceTransaction', [req.l2Hash]);
        expect(trace).not.toBeNull();
        expect(trace.revertReason).toEqual(EMPTY_REVERT_REASON);
        expect(trace.calls.every((c: any) => !zksync.utils.isAddressEq(c.to, receipt.to!))).toBe(true);

        // Everything minted for the op minus the operator's fee goes back to the refund recipient.
        await expect(refundRecipient.getBalance()).resolves.toEqual(refundBefore + req.mintValue - fee);
        return receipt;
    }

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        bob = testMaster.newEmptyAccount();
        const env = testMaster.environment();
        // Without the feature the marked txs would simply succeed.
        if (!env.forceFailEnabled) {
            throw new Error('RUN_FORCE_FAIL_TEST is set but state_keeper.force_fail.enabled is not true');
        }
        mainContract = await alice.getMainContract();
        mainContractAddress = await alice.getMainContractAddress();
        dbUrl = mainNodeDbUrl(env.pathToHome, process.env.CHAIN_NAME!);
        tokenDetails = env.erc20Token;
        isETHBasedChain = env.baseToken.l1Address == zksync.utils.ETH_ADDRESS_IN_CONTRACTS;
        if (!isETHBasedChain) {
            const maxAmount = await alice.getBalanceL1(env.baseToken.l1Address);
            await (await alice.approveERC20(env.baseToken.l1Address, maxAmount)).wait();
        }
    });

    test('Should deploy the counter', async () => {
        counter = await deployContract(alice, getTestContract('Counter'), []);
    });

    test('ERC20 deposit is force-failed and can be claimed back on L1', async () => {
        if (testMaster.isFastMode()) {
            return;
        }
        const amount = 1n;
        const gasPrice = await scaledGasPrice(alice);
        const l1Before = await alice.getBalanceL1(tokenDetails.l1Address);
        const bobTokenBefore = await bob.getBalance(tokenDetails.l2Address);
        const bobBaseBefore = await bob.getBalance();

        const req = await sendForceFailed(() =>
            alice.deposit({
                token: tokenDetails.l1Address,
                to: bob.address,
                amount,
                refundRecipient: bob.address,
                approveERC20: true,
                approveBaseERC20: true,
                l2GasLimit: DEFAULT_L2_GAS_LIMIT,
                approveOverrides: { gasPrice },
                overrides: { gasPrice }
            })
        );
        // Escrowed on L1, failed on L2, nothing minted.
        await expect(alice.getBalanceL1(tokenDetails.l1Address)).resolves.toEqual(l1Before - amount);
        await expect(req.handle).toBeReverted([]);
        const receipt = await expectForceFailed(req, bob, bobBaseBefore);
        await expect(bob.getBalance(tokenDetails.l2Address)).resolves.toEqual(bobTokenBefore);

        // The claim needs the batch executed on L1, which `_executeOneBatch` only does with the priority
        // queue advanced past the force-failed op.
        await waitForL2ToL1LogProof(alice, receipt.blockNumber, req.l2Hash, FINALIZATION_TIMEOUT_MS);
        await expect(alice.claimFailedDeposit(req.l2Hash)).toBeAccepted();
        await expect(alice.getBalanceL1(tokenDetails.l1Address)).resolves.toEqual(l1Before);
        expect(await mainContract.getFirstUnprocessedPriorityTx()).toBeGreaterThan(req.id);
    });

    test('requestExecute with L2 value is force-failed without running user code', async () => {
        if (testMaster.isFastMode()) {
            return;
        }
        const counterAddress = await counter.getAddress();
        const l2Value = 10n;
        const calldata = counter.interface.encodeFunctionData('incrementWithRevertPayable', [1n, false]);
        const gasPrice = await scaledGasPrice(alice);
        const mintValueParam = await mintValueForRequestExecute(gasPrice);
        const counterBefore = await counter.get();
        const counterBaseBefore = await alice.provider.getBalance(counterAddress);
        const bobBaseBefore = await bob.getBalance();

        const req = await sendForceFailed(() =>
            alice.requestExecute({
                contractAddress: counterAddress,
                calldata,
                l2Value,
                mintValue: mintValueParam,
                l2GasLimit: DEFAULT_L2_GAS_LIMIT,
                refundRecipient: bob.address,
                overrides: { gasPrice }
            })
        );
        await expect(req.handle).toBeReverted([]);
        await expectForceFailed(req, bob, bobBaseBefore);
        await expect(counter.get()).resolves.toEqual(counterBefore);
        await expect(alice.provider.getBalance(counterAddress)).resolves.toEqual(counterBaseBefore);
    });

    test('Unmarked deposit sent afterwards succeeds and finalizes', async () => {
        if (testMaster.isFastMode()) {
            return;
        }
        const amount = 1n;
        const gasPrice = await scaledGasPrice(alice);
        const balanceChange = await shouldChangeTokenBalances(tokenDetails.l2Address, [
            { wallet: alice, change: amount }
        ]);
        const handle = await alice.deposit({
            token: tokenDetails.l1Address,
            amount,
            approveERC20: true,
            approveBaseERC20: true,
            l2GasLimit: DEFAULT_L2_GAS_LIMIT,
            approveOverrides: { gasPrice },
            overrides: { gasPrice }
        });
        await expect(handle).toBeAccepted([balanceChange]);
        const receipt = await handle.wait();
        controlHash = receipt.hash;
        // Executed on L1 after the force-failed ops: the queue moved on.
        await waitUntilBlockFinalized(alice, receipt.blockNumber, FINALIZATION_TIMEOUT_MS);
    });

    test('forceFailed is absent for an ordinary L1->L2 transaction', async () => {
        if (testMaster.isFastMode()) {
            return;
        }
        const details = (await alice.provider.getTransactionDetails(controlHash)) as any;
        expect(details.forceFailed).toBeUndefined();
        expect(details.status).not.toEqual('failed');
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});
