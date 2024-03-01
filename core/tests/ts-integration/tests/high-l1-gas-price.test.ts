/**
 * This suit contains tests for the system behavior under high L1 gas price.
 *
 * IMPORTANT: this test affects the internal state of the server and so
 * it should never be run in parallel with other tests.
 *
 */
import * as utils from 'zk/build/utils';
import * as fs from 'fs';
import { TestMaster } from '../src/index';

import * as zksync from 'zksync-web3';
import { ethers } from 'ethers';
import { Token } from '../src/types';

const logs = fs.createWriteStream('high-gas-price.log', { flags: 'a' });

// Unless `RUN_FEE_TEST` is provided, skip the test suit
const testHighGasPriceBehavior = process.env.HIGH_L1_GAS_PRICE_TEST ? describe : describe.skip;

testHighGasPriceBehavior('Test the system under high gas price', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;

    beforeAll(() => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
    });

    test('Test fees', async () => {
        // 1000 gwei
        await setInternalL1GasPrice(alice._providerL2(), '1000000000000', true);
        const senderNonce = await alice.getNonce();

        // This transaction should not be processed, but it should be at least accepted by the server.
        await alice.sendTransaction({
            to: alice.address,
            gasPrice: ethers.BigNumber.from(process.env.CHAIN_STATE_KEEPER_MINIMAL_L2_GAS_PRICE),
            nonce: senderNonce
        });

        // The new transaction should pass
        await (
            await alice.sendTransaction({
                to: alice.address,
                nonce: senderNonce,
                gasPrice: 100_000_000_000
            })
        ).wait();

        await setInternalL1GasPrice(alice._providerL2(), undefined, true);
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});

async function killServerAndWaitForShutdown(provider: zksync.Provider) {
    await utils.exec('pkill zksync_server');
    // Wait until it's really stopped.
    let iter = 0;
    while (iter < 30) {
        try {
            await provider.getBlockNumber();
            await utils.sleep(5);
            iter += 1;
        } catch (_) {
            // When exception happens, we assume that server died.
            return;
        }
    }
    // It's going to panic anyway, since the server is a singleton entity, so better to exit early.
    throw new Error("Server didn't stop after a kill request");
}

async function setInternalL1GasPrice(provider: zksync.Provider, newPrice?: string, disconnect?: boolean) {
    // Make sure server isn't running.
    try {
        await killServerAndWaitForShutdown(provider);
    } catch (_) {}

    // Run server in background.
    let command = 'zk server --components api,tree,eth,state_keeper';
    command = `DATABASE_MERKLE_TREE_MODE=full ${command}`;
    if (newPrice) {
        // We need to ensure that each transaction gets into its own batch for more fair comparison.
        command = `CHAIN_STATE_KEEPER_TRANSACTION_SLOTS=1 ETH_SENDER_GAS_ADJUSTER_INTERNAL_ENFORCED_L1_GAS_PRICE=${newPrice} ${command}`;
    }
    const zkSyncServer = utils.background(command, [null, logs, logs]);

    if (disconnect) {
        zkSyncServer.unref();
    }

    // Server may need some time to recompile if it's a cold run, so wait for it.
    let iter = 0;
    let mainContract;
    while (iter < 30 && !mainContract) {
        try {
            mainContract = await provider.getMainContractAddress();
        } catch (_) {
            await utils.sleep(5);
            iter += 1;
        }
    }

    if (!mainContract) {
        throw new Error('Server did not start');
    }

    // We still need to wait a bit more to ensure that the server is fully operational.
    await utils.sleep(10);
}
