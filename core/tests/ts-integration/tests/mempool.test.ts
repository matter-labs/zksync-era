/**
 * This suite contains tests checking the mempool behavior: how transactions are inserted,
 * scheduled, processed and/or postponed.
 */
import { TestMaster } from '../src';
import * as zksync from 'zksync-ethers';

describe.skip('Tests for the mempool behavior', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;

    beforeAll(() => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
    });

    test('Should allow a nonce gap', async () => {
        // Here we check a basic case: first we send a transaction with nonce +1, then with valid nonce.
        // Both transactions should be processed.
        const startNonce = await alice.getNonce();

        const tx2 = await sendTxWithNonce(alice, startNonce + 1);
        const tx1 = await sendTxWithNonce(alice, startNonce);

        await expect(tx1).toBeAccepted([]);
        await expect(tx2).toBeAccepted([]);
        // @ts-ignore
    }, 600000);

    test('Should process shuffled nonces', async () => {
        // More complex nonce mixup: we send 5 txs completely out of order.
        const startNonce = await alice.getNonce();

        const nonceOffsets = [4, 0, 3, 1, 2];
        const txs = nonceOffsets.map((offset) => sendTxWithNonce(alice, startNonce + offset).then((tx) => tx.wait()));

        // If any of txs would fail, it would throw.
        // If txs would get stuck, test would be killed because of timeout.
        await Promise.all(txs);
        // @ts-ignore
    }, 600000);

    test('Should discard too low nonce', async () => {
        const startNonce = await alice.getNonce();
        await expect(sendTxWithNonce(alice, startNonce - 1)).toBeRejected('nonce too low.');
    });

    test('Should discard too big nonce', async () => {
        const maxNonceAhead = 450; // Matches the server config.
        const startNonce = await alice.getNonce();
        await expect(sendTxWithNonce(alice, startNonce + maxNonceAhead + 1)).toBeRejected('nonce too high.');
    });

    test('Should correctly show pending nonce', async () => {
        const startNonce = await alice.getNonce();
        // Send tx with nonce + 1
        const tx2 = await sendTxWithNonce(alice, startNonce + 1);

        // Nonce from API should not change (e.g. not become "nonce + 2").
        const nonce = await alice.getNonce();
        expect(nonce).toEqual(startNonce);

        // Finish both transactions to not ruin the flow for other tests.
        const tx1 = await sendTxWithNonce(alice, startNonce);
        await Promise.all([tx1.wait(), tx2.wait()]);
    });

    test('Should replace the transaction', async () => {
        const startNonce = await alice.getNonce();
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

        // Now fill the gap and see what gets executed
        await sendTxWithNonce(alice, startNonce).then((tx) => tx.wait());
        const replacedReceipt = await replacedTx2.wait();

        expect(replacedReceipt.to).toEqual(bob.address);

        // First transaction should disappear from the server.
        await expect(alice.provider.getTransaction(tx2.hash)).resolves.toBeNull();
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
        const fund = (gasLimit * gasPrice * 13n) / 10n;
        await alice.sendTransaction({ to: poorBob.address, value: fund }).then((tx) => tx.wait());

        // delayedTx should pass API checks (if not then error will be thrown on the next lime)
        // but should be rejected by the state-keeper (checked later).
        const delayedTx = await poorBob.sendTransaction({
            to: poorBob.address,
            nonce: nonce + 1,
            type: 0
        });

        await expect(
            poorBob.sendTransaction({
                to: poorBob.address,
                nonce,
                type: 0
            })
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

/**
 * Sends a valid ZKsync transaction with a certain nonce.
 * What transaction does is assumed to be not important besides the fact that it should be accepted.
 *
 * @param wallet Wallet to send transaction from.
 * @param nonce Nonce to use
 * @param to Optional recipient of the transaction.
 *
 * @returns Transaction request object.
 */
function sendTxWithNonce(wallet: zksync.Wallet, nonce: number, to?: string): Promise<zksync.types.TransactionResponse> {
    return wallet.sendTransaction({
        to: to ?? wallet.address,
        value: 1,
        nonce
    });
}
