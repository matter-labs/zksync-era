import * as utils from 'utils';
import { loadConfig, shouldLoadConfigFromFile, getAllConfigsPath } from 'utils/build/file-configs';
import { runServerInBackground } from './utils';
import { Tester } from './tester';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { expect } from 'chai';
import fs from 'fs';
import { IZkSyncHyperchain } from 'zksync-ethers/build/typechain';
import path from 'path';

// Parses output of "print-suggested-values" command of the revert block tool.
function parseSuggestedValues(suggestedValuesString: string): {
    lastL1BatchNumber: bigint;
    nonce: bigint;
    priorityFee: bigint;
} {
    const json = JSON.parse(suggestedValuesString);
    if (!json || typeof json !== 'object') {
        throw new TypeError('suggested values are not an object');
    }

    const lastL1BatchNumber = json.last_executed_l1_batch_number;
    if (!Number.isInteger(lastL1BatchNumber)) {
        throw new TypeError('suggested `lastL1BatchNumber` is not an integer');
    }
    const nonce = json.nonce;
    if (!Number.isInteger(nonce)) {
        throw new TypeError('suggested `nonce` is not an integer');
    }
    const priorityFee = json.priority_fee;
    if (!Number.isInteger(priorityFee)) {
        throw new TypeError('suggested `priorityFee` is not an integer');
    }

    return {
        lastL1BatchNumber: BigInt(lastL1BatchNumber),
        nonce: BigInt(nonce),
        priorityFee: BigInt(priorityFee)
    };
}

async function killServerAndWaitForShutdown(tester: Tester) {
    await utils.exec('killall -9 zksync_server');
    // Wait until it's really stopped.
    let iter = 0;
    while (iter < 30) {
        try {
            await tester.syncWallet.provider.getBlockNumber();
            await utils.sleep(2);
            iter += 1;
        } catch (_) {
            // When exception happens, we assume that server died.
            return;
        }
    }
    // It's going to panic anyway, since the server is a singleton entity, so better to exit early.
    throw new Error("Server didn't stop after a kill request");
}

function ignoreError(_err: any, context?: string) {
    const message = context ? `Error ignored (context: ${context}).` : 'Error ignored.';
    console.info(message);
}

const depositAmount = ethers.parseEther('0.001');

describe('Block reverting test', function () {
    let tester: Tester;
    let alice: zksync.Wallet;
    let mainContract: IZkSyncHyperchain;
    let blocksCommittedBeforeRevert: bigint;
    let logs: fs.WriteStream;
    let operatorAddress: string;
    let ethClientWeb3Url: string;
    let apiWeb3JsonRpcHttpUrl: string;

    const fileConfig = shouldLoadConfigFromFile();

    const pathToHome = path.join(__dirname, '../../../..');

    const enableConsensus = process.env.ENABLE_CONSENSUS == 'true';
    let components = 'api,tree,eth,state_keeper,commitment_generator,da_dispatcher,vm_runner_protective_reads';
    if (enableConsensus) {
        components += ',consensus';
    }

    before('initialize test', async () => {
        // Clone file configs if necessary
        let baseTokenAddress: string;

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

        // Create test wallets
        tester = await Tester.init(ethClientWeb3Url, apiWeb3JsonRpcHttpUrl, baseTokenAddress);
        alice = tester.emptyWallet();
        logs = fs.createWriteStream('revert.log', { flags: 'a' });
    });

    step('run server and execute some transactions', async () => {
        // Make sure server isn't running.
        await killServerAndWaitForShutdown(tester).catch(ignoreError);

        // Run server in background.
        runServerInBackground({
            components: [components],
            stdio: [null, logs, logs],
            cwd: pathToHome,
            useZkInception: fileConfig.loadFromFile,
            chain: fileConfig.chain
        });

        // Server may need some time to recompile if it's a cold run, so wait for it.
        let iter = 0;
        while (iter < 30 && !mainContract) {
            try {
                mainContract = await tester.syncWallet.getMainContract();
            } catch (err) {
                ignoreError(err, 'waiting for server HTTP JSON-RPC to start');
                await utils.sleep(2);
                iter += 1;
            }
        }
        if (!mainContract) {
            throw new Error('Server did not start');
        }

        await tester.fundSyncWallet();

        // Seal 2 L1 batches.
        // One is not enough to test the reversion of sk cache because
        // it gets updated with some batch logs only at the start of the next batch.
        const initialL1BatchNumber = await tester.web3Provider.getL1BatchNumber();
        const firstDepositHandle = await tester.syncWallet.deposit({
            token: tester.isETHBasedChain ? zksync.utils.LEGACY_ETH_ADDRESS : tester.baseTokenAddress,
            amount: depositAmount,
            to: alice.address,
            approveBaseERC20: true,
            approveERC20: true
        });
        await firstDepositHandle.wait();
        while ((await tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber) {
            await utils.sleep(1);
        }
        const secondDepositHandle = await tester.syncWallet.deposit({
            token: tester.isETHBasedChain ? zksync.utils.LEGACY_ETH_ADDRESS : tester.baseTokenAddress,
            amount: depositAmount,
            to: alice.address,
            approveBaseERC20: true,
            approveERC20: true
        });
        await secondDepositHandle.wait();
        while ((await tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber + 1) {
            await utils.sleep(1);
        }

        const balance = await alice.getBalance();
        expect(balance === depositAmount * 2n, 'Incorrect balance after deposits').to.be.true;

        // Check L1 committed and executed blocks.
        let blocksCommitted = await mainContract.getTotalBatchesCommitted();
        let blocksExecuted = await mainContract.getTotalBatchesExecuted();
        let tryCount = 0;
        while (blocksCommitted === blocksExecuted && tryCount < 100) {
            blocksCommitted = await mainContract.getTotalBatchesCommitted();
            blocksExecuted = await mainContract.getTotalBatchesExecuted();
            tryCount += 1;
            await utils.sleep(1);
        }
        expect(blocksCommitted > blocksExecuted, 'There is no committed but not executed block').to.be.true;
        blocksCommittedBeforeRevert = blocksCommitted;

        // Stop server.
        await killServerAndWaitForShutdown(tester);
    });

    step('revert blocks', async () => {
        let fileConfigFlags = '';
        if (fileConfig.loadFromFile) {
            const configPaths = getAllConfigsPath({ pathToHome, chain: fileConfig.chain });
            fileConfigFlags = `
                --config-path=${configPaths['general.yaml']}
                --contracts-config-path=${configPaths['contracts.yaml']}
                --secrets-path=${configPaths['secrets.yaml']}
                --wallets-path=${configPaths['wallets.yaml']}
                --genesis-path=${configPaths['genesis.yaml']}
            `;
        }

        const executedProcess = await utils.exec(
            `cd ${pathToHome} && RUST_LOG=off cargo run --bin block_reverter --release -- print-suggested-values --json --operator-address ${operatorAddress} ${fileConfigFlags}`
            // ^ Switch off logs to not pollute the output JSON
        );
        const suggestedValuesOutput = executedProcess.stdout;
        const { lastL1BatchNumber, nonce, priorityFee } = parseSuggestedValues(suggestedValuesOutput);
        expect(lastL1BatchNumber < blocksCommittedBeforeRevert, 'There should be at least one block for revert').to.be
            .true;

        console.log(
            `Reverting with parameters: last unreverted L1 batch number: ${lastL1BatchNumber}, nonce: ${nonce}, priorityFee: ${priorityFee}`
        );

        console.log('Sending ETH transaction..');
        await utils.spawn(
            `cd ${pathToHome} && cargo run --bin block_reverter --release -- send-eth-transaction --l1-batch-number ${lastL1BatchNumber} --nonce ${nonce} --priority-fee-per-gas ${priorityFee} ${fileConfigFlags}`
        );

        console.log('Rolling back DB..');
        await utils.spawn(
            `cd ${pathToHome} && cargo run --bin block_reverter --release -- rollback-db --l1-batch-number ${lastL1BatchNumber} --rollback-postgres --rollback-tree --rollback-sk-cache --rollback-vm-runners-cache ${fileConfigFlags}`
        );

        let blocksCommitted = await mainContract.getTotalBatchesCommitted();
        expect(blocksCommitted === lastL1BatchNumber, 'Revert on contract was unsuccessful').to.be.true;
    });

    step('execute transaction after revert', async () => {
        // Run server.
        runServerInBackground({
            components: [components],
            stdio: [null, logs, logs],
            cwd: pathToHome,
            useZkInception: fileConfig.loadFromFile,
            chain: fileConfig.chain
        });
        await utils.sleep(30);

        const balanceBefore = await alice.getBalance();
        expect(balanceBefore === depositAmount * 2n, 'Incorrect balance after revert').to.be.true;

        // Execute a transaction
        const depositHandle = await tester.syncWallet.deposit({
            token: tester.isETHBasedChain ? zksync.utils.LEGACY_ETH_ADDRESS : tester.baseTokenAddress,
            amount: depositAmount,
            to: alice.address,
            approveBaseERC20: true,
            approveERC20: true
        });

        let l1TxResponse = await alice._providerL1().getTransaction(depositHandle.hash);
        while (!l1TxResponse) {
            console.log(`Deposit ${depositHandle.hash} is not visible to the L1 network; sleeping`);
            await utils.sleep(1);
            l1TxResponse = await alice._providerL1().getTransaction(depositHandle.hash);
        }

        // ethers doesn't work well with block reversions, so wait for the receipt before calling `.waitFinalize()`.
        const l2Tx = await alice._providerL2().getL2TransactionFromPriorityOp(l1TxResponse);
        let receipt = null;
        do {
            receipt = await tester.syncWallet.provider.getTransactionReceipt(l2Tx.hash);
            await utils.sleep(1);
        } while (receipt == null);

        await depositHandle.waitFinalize();
        expect(receipt.status).to.be.eql(1);

        const balanceAfter = await alice.getBalance();
        expect(balanceAfter === depositAmount * 3n, 'Incorrect balance after another deposit').to.be.true;
    });

    step('execute transactions after simple restart', async () => {
        // Execute an L2 transaction
        await checkedRandomTransfer(alice, 1n);

        // Stop server.
        await killServerAndWaitForShutdown(tester);

        // Run again.
        runServerInBackground({
            components: [components],
            stdio: [null, logs, logs],
            cwd: pathToHome,
            useZkInception: fileConfig.loadFromFile,
            chain: fileConfig.chain
        });
        await utils.sleep(30);

        // Trying to send a transaction from the same address again
        await checkedRandomTransfer(alice, 1n);
    });

    after('Try killing server', async () => {
        await utils.exec('killall zksync_server').catch(ignoreError);
    });
});

async function checkedRandomTransfer(sender: zksync.Wallet, amount: bigint) {
    const senderBalanceBefore = await sender.getBalance();
    const receiverHD = zksync.Wallet.createRandom();
    const receiver = new zksync.Wallet(receiverHD.privateKey, sender.provider);
    const transferHandle = await sender.sendTransaction({
        to: receiver.address,
        value: amount,
        type: 0
    });

    // ethers doesn't work well with block reversions, so we poll for the receipt manually.
    let txReceipt = null;
    do {
        txReceipt = await sender.provider.getTransactionReceipt(transferHandle.hash);
        await utils.sleep(1);
    } while (txReceipt == null);

    const senderBalance = await sender.getBalance();
    const receiverBalance = await receiver.getBalance();

    expect(receiverBalance === amount, 'Failed updated the balance of the receiver').to.be.true;

    const spentAmount = txReceipt.gasUsed * transferHandle.gasPrice! + amount;
    expect(senderBalance + spentAmount >= senderBalanceBefore, 'Failed to update the balance of the sender').to.be.true;
}
