/**
 * This suite contains tests checking the mempool behavior: how transactions are inserted,
 * scheduled, processed and/or postponed.
 */
import { TestMaster } from '../src/index';
import * as zksync from 'zksync-web3';

describe('Tests for the mempool behavior', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;

    beforeAll(() => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
    });

    test('Should allow a nonce gap', async () => {
        // Here we check a basic case: first we send a transaction with nonce +1, then with valid nonce.
        // Both transactions should be processed.
        const startNonce = await alice.getTransactionCount();

        const tx2 = await sendTxWithNonce(alice, startNonce + 1);
        const tx1 = await sendTxWithNonce(alice, startNonce);

        await expect(tx1).toBeAccepted([]);
        await expect(tx2).toBeAccepted([]);
        // @ts-ignore
    }, 600000);

    test('Should process shuffled nonces', async () => {
        // More complex nonce mixup: we send 5 txs completely out of order.
        const startNonce = await alice.getTransactionCount();

        const nonceOffsets = [4, 0, 3, 1, 2];
        const txs = nonceOffsets.map((offset) => sendTxWithNonce(alice, startNonce + offset).then((tx) => tx.wait()));

        // If any of txs would fail, it would throw.
        // If txs would get stuck, test would be killed because of timeout.
        await Promise.all(txs);
        // @ts-ignore
    }, 600000);

    test('Should discard too low nonce', async () => {
        const startNonce = await alice.getTransactionCount();
        await expect(sendTxWithNonce(alice, startNonce - 1)).toBeRejected('nonce too low.');
    });

    test('Should discard too big nonce', async () => {
        const maxNonceAhead = 450; // Matches the server config.
        const startNonce = await alice.getTransactionCount();
        await expect(sendTxWithNonce(alice, startNonce + maxNonceAhead + 1)).toBeRejected('nonce too high.');
    });

    test('Should correctly show pending nonce', async () => {
        const startNonce = await alice.getTransactionCount();
        // Send tx with nonce + 1
        const tx2 = await sendTxWithNonce(alice, startNonce + 1);

        // Nonce from API should not change (e.g. not become "nonce + 2").
        const nonce = await alice.getTransactionCount();
        expect(nonce).toEqual(startNonce);

        // Finish both transactions to not ruin the flow for other tests.
        const tx1 = await sendTxWithNonce(alice, startNonce);
        await Promise.all([tx1.wait(), tx2.wait()]);
    });

    test('Should replace the transaction', async () => {
        const startNonce = await alice.getTransactionCount();
        // Send tx with nonce + 1
        const tx2 = await sendTxWithNonce(alice, startNonce + 1);
        await expect(alice.provider.getTransaction(tx2.hash)).resolves.toMatchObject({
            nonce: startNonce + 1,
            to: alice.address
        });

        // Change our mind, replace the transaction, while we can!
        const bob = testMaster.newEmptyAccount();
        const replacedTx2 = await sendTxWithNonce(alice, startNonce + 1, bob.address);
        await expect(alice.provider.getTransaction(replacedTx2.hash)).resolves.toMatchObject({
            nonce: startNonce + 1,
            to: bob.address
        });
        // First transaction should disappear from the server.
        await expect(alice.provider.getTransaction(tx2.hash)).resolves.toBeNull();

        // Now fill the gap and see what gets executed
        await sendTxWithNonce(alice, startNonce).then((tx) => tx.wait());
        const replacedReceipt = await replacedTx2.wait();

        expect(replacedReceipt.to).toEqual(bob.address);
    });

    test('Should reject a pre-sent transaction with not enough balance', async () => {
        // In this test we send tx with the nonce from the future that should be rejected,
        // i.e. transaction should pass the API server, but be rejected once queried by the mempool.
        // To do so we create an account that has balance to execute just one transaction, and
        // send two transactions with `nonce + 1` and after that with `nonce`.
        const poorBob = testMaster.newEmptyAccount();
        const nonce = 0; // No transactions from this account were sent.

        const gasLimit = await alice.estimateGas({ to: alice.address });
        const gasPrice = await alice.provider.getGasPrice();
        const fund = gasLimit.mul(gasPrice).mul(13).div(10);
        await alice.sendTransaction({ to: poorBob.address, value: fund }).then((tx) => tx.wait());

        const delayedTx = await poorBob.sendTransaction({
            to: poorBob.address,
            nonce: nonce + 1
        });
        // delayedTx passed API checks (since we got the response) but should be rejected by the state-keeper.
        const rejection = expect(delayedTx).toBeReverted();

        await expect(poorBob.sendTransaction({ to: poorBob.address, nonce })).toBeAccepted();
        await rejection;

        // Now check that there is only one executed transaction for the account.
        await expect(poorBob.getTransactionCount()).resolves.toEqual(1);
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});

/**
 * Sends a valid zkSync transaction with a certain nonce.
 * What transaction does is assumed to be not important besides the fact that it should be accepted.
 *
 * @param wallet Wallet to send transaction from.
 * @param nonce Nonce to use
 * @param to Optional recipient of the transaction.
 *
 * @returns Transaction request object.
 */
function sendTxWithNonce(wallet: zksync.Wallet, nonce: number, to?: string) {
    return wallet.sendTransaction({
        to: to ?? wallet.address,
        value: 1,
        nonce
    });
}
