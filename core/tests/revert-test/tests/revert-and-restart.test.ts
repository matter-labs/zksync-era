import * as utils from 'utils';
import { loadConfig, shouldLoadConfigFromFile } from 'utils/build/file-configs';
import {
    executeRevert,
    MainNode,
    killServerAndWaitForShutdown,
    createBatchWithDeposit,
    waitToExecuteBatch,
    waitToCommitBatchesWithoutExecution,
    executeDepositAfterRevert,
    checkRandomTransfer,
    MainNodeSpawner
} from './utils';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { assert } from 'chai';
import { IZkSyncHyperchain } from 'zksync-ethers/build/typechain';
import path from 'path';
import fs from 'node:fs/promises';
import { logsTestPath } from 'utils/build/logs';

function ignoreError(_err: any, context?: string) {
    const message = context ? `Error ignored (context: ${context}).` : 'Error ignored.';
    console.info(message);
}

describe('Block reverting test', function () {
    let alice: zksync.Wallet;
    let mainContract: IZkSyncHyperchain;
    let initialL1BatchNumber: number;
    let batchesCommittedBeforeRevert: bigint;
    let mainLogs: fs.FileHandle;
    let operatorAddress: string;
    let baseTokenAddress: string;
    let ethClientWeb3Url: string;
    let apiWeb3JsonRpcHttpUrl: string;
    let mainNodeSpawner: MainNodeSpawner;
    let mainNode: MainNode;

    const fileConfig = shouldLoadConfigFromFile();
    const pathToHome = path.join(__dirname, '../../../..');
    const autoKill: boolean = !fileConfig.loadFromFile || !process.env.NO_KILL;
    const enableConsensus = process.env.ENABLE_CONSENSUS == 'true';
    const depositAmount = ethers.parseEther('0.001');

    async function logsPath(name: string): Promise<string> {
        return await logsTestPath(fileConfig.chain, 'logs/revert/', name);
    }

    before('initialize test', async () => {
        if (!fileConfig.loadFromFile) {
            operatorAddress = process.env.ETH_SENDER_SENDER_OPERATOR_COMMIT_ETH_ADDR!;
            ethClientWeb3Url = process.env.ETH_CLIENT_WEB3_URL!;
            apiWeb3JsonRpcHttpUrl = process.env.API_WEB3_JSON_RPC_HTTP_URL!;
            baseTokenAddress = process.env.CONTRACTS_BASE_TOKEN_ADDR!;
        } else {
            const generalConfig = loadConfig({
                pathToHome,
                chain: fileConfig.chain,
                config: 'general.yaml'
            });
            const secretsConfig = loadConfig({
                pathToHome,
                chain: fileConfig.chain,
                config: 'secrets.yaml'
            });
            const walletsConfig = loadConfig({
                pathToHome,
                chain: fileConfig.chain,
                config: 'wallets.yaml'
            });
            const contractsConfig = loadConfig({
                pathToHome,
                chain: fileConfig.chain,
                config: 'contracts.yaml'
            });

            operatorAddress = walletsConfig.operator.address;
            ethClientWeb3Url = secretsConfig.l1.l1_rpc_url;
            apiWeb3JsonRpcHttpUrl = generalConfig.api.web3_json_rpc.http_url;
            baseTokenAddress = contractsConfig.l1.base_token_addr;
        }

        const pathToMainLogs = await logsPath('server.log');
        mainLogs = await fs.open(pathToMainLogs, 'a');
        console.log(`Writing server logs to ${pathToMainLogs}`);

        mainNodeSpawner = new MainNodeSpawner(pathToHome, mainLogs, fileConfig, {
            enableConsensus,
            ethClientWeb3Url,
            apiWeb3JsonRpcHttpUrl,
            baseTokenAddress
        });
    });

    step('Make sure that the server is not running', async () => {
        if (autoKill) {
            // Make sure server isn't running.
            await MainNode.terminateAll();
        }
    });

    step('start server', async () => {
        mainNode = await mainNodeSpawner.spawn(true);
    });

    step('fund wallet', async () => {
        await mainNode.tester.fundSyncWallet();
        mainContract = await mainNode.tester.syncWallet.getMainContract();
        alice = mainNode.tester.emptyWallet();
    });

    // Seal 2 L1 batches.
    // One is not enough to test the reversion of sk cache because
    // it gets updated with some batch logs only at the start of the next batch.
    step('seal L1 batch', async () => {
        initialL1BatchNumber = await createBatchWithDeposit(mainNode, alice.address, depositAmount);
    });

    step('wait for L1 batch to get executed', async () => {
        await waitToExecuteBatch(mainContract, initialL1BatchNumber);
    });

    step('restart server with batch execution turned off', async () => {
        await killServerAndWaitForShutdown(mainNode);
        mainNode = await mainNodeSpawner.spawn(false);
    });

    step('seal another L1 batch', async () => {
        await createBatchWithDeposit(mainNode, alice.address, depositAmount);
    });

    step('check wallet balance', async () => {
        const balance = await alice.getBalance();
        console.log(`Balance before revert: ${balance}`);
        assert(balance === depositAmount * 2n, 'Incorrect balance after deposits');
    });

    step('wait for the new batch to be committed', async () => {
        batchesCommittedBeforeRevert = await waitToCommitBatchesWithoutExecution(mainContract);
    });

    step('stop server', async () => {
        await killServerAndWaitForShutdown(mainNode);
    });

    step('revert batches', async () => {
        await executeRevert(pathToHome, fileConfig.chain, operatorAddress, batchesCommittedBeforeRevert, mainContract);
    });

    step('restart server', async () => {
        mainNode = await mainNodeSpawner.spawn(true);
    });

    step('wait until last deposit is re-executed', async () => {
        let balanceBefore;
        let tryCount = 0;
        while ((balanceBefore = await alice.getBalance()) !== 2n * depositAmount && tryCount < 30) {
            console.log(`Balance after revert: ${balanceBefore}`);
            tryCount++;
            await utils.sleep(1);
        }
        assert(balanceBefore === 2n * depositAmount, 'Incorrect balance after revert');
    });

    step('execute transaction after revert', async () => {
        await executeDepositAfterRevert(mainNode.tester, alice, depositAmount);
        const balanceAfter = await alice.getBalance();
        console.log(`Balance after another deposit: ${balanceAfter}`);
        assert(balanceAfter === depositAmount * 3n, 'Incorrect balance after another deposit');
    });

    step('execute transactions after simple restart', async () => {
        // Execute an L2 transaction
        await checkRandomTransfer(alice, 1n);

        // Stop server.
        await killServerAndWaitForShutdown(mainNode);

        // Run again.
        mainNode = await mainNodeSpawner.spawn(true);

        // Trying to send a transaction from the same address again
        await checkRandomTransfer(alice, 1n);
    });

    after('Try killing server', async () => {
        if (autoKill) {
            await utils.exec('killall zksync_server').catch(ignoreError);
        }
    });
});
