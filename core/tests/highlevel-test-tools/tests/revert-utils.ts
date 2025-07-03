import { assert, expect } from 'chai';
import { getAllConfigsPath } from 'utils/build/file-configs';
import { IZkSyncHyperchain } from 'zksync-ethers/build/typechain';
import { Tester } from './revert-tester';
import * as utils from 'utils';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import * as zksync from 'zksync-ethers';
import { executeCommand } from '../src';

export interface SuggestedValues {
    lastExecutedL1BatchNumber: bigint;
    nonce: number;
    priorityFee: number;
}

/** Parses output of "print-suggested-values" command of the revert block tool. */
export function parseSuggestedValues(jsonString: string): SuggestedValues {
    let json;
    try {
        json = JSON.parse(jsonString);
    } catch {
        utils.log(`Failed to parse string: ${jsonString}`);
    }
    assert(json && typeof json === 'object');
    assert(Number.isInteger(json.last_executed_l1_batch_number));
    assert(Number.isInteger(json.nonce));
    assert(Number.isInteger(json.priority_fee));
    return {
        lastExecutedL1BatchNumber: BigInt(json.last_executed_l1_batch_number),
        nonce: json.nonce,
        priorityFee: json.priority_fee
    };
}

async function runBlockReverter(pathToHome: string, chain: string, args: string[]) {
    const configPaths = getAllConfigsPath({ pathToHome, chain });
    const configsFlags = [
        `--config-path=${configPaths['general.yaml']}`,
        `--contracts-config-path=${configPaths['contracts.yaml']}`,
        `--secrets-path=${configPaths['secrets.yaml']}`,
        `--wallets-path=${configPaths['wallets.yaml']}`,
        `--genesis-path=${configPaths['genesis.yaml']}`,
        `--gateway-chain-path=${configPaths['gateway_chain.yaml']}`
    ];

    const basicArgs = ['run', '--manifest-path', './core/Cargo.toml', '--bin', 'block_reverter', '--release', '--'];

    await executeCommand('cargo', basicArgs.concat(args).concat(configsFlags), chain, 'block_reverter', false, {
        cwd: pathToHome
    });
}

export async function revertExternalNode(chain: string, l1Batch: bigint) {
    await executeCommand(
        'zkstack',
        ['external-node', 'run', '--chain', chain, '--', 'revert', l1Batch.toString()],
        chain,
        'external_node_revert'
    );
}

export async function executeRevert(
    pathToHome: string,
    chain: string,
    operatorAddress: string,
    batchesCommittedBeforeRevert: bigint,
    mainContract: IZkSyncHyperchain
): Promise<bigint> {
    const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'zksync-revert-test-'));
    const jsonPath = path.join(tmpDir, 'values.json');
    utils.log(`Temporary file for suggested revert values: ${jsonPath}`);

    let suggestedValuesOutput: string;
    try {
        await runBlockReverter(pathToHome, chain, [
            'print-suggested-values',
            '--json',
            jsonPath,
            '--operator-address',
            operatorAddress
        ]);
        suggestedValuesOutput = await fs.readFile(jsonPath, { encoding: 'utf-8' });
    } finally {
        await fs.rm(tmpDir, { recursive: true, force: true });
    }

    const values = parseSuggestedValues(suggestedValuesOutput);
    assert(
        values.lastExecutedL1BatchNumber < batchesCommittedBeforeRevert,
        'There should be at least one block for revert'
    );

    utils.log('Reverting with parameters', values);

    utils.log('Sending ETH transaction..');
    await runBlockReverter(pathToHome, chain, [
        'send-eth-transaction',
        '--l1-batch-number',
        values.lastExecutedL1BatchNumber.toString(),
        '--nonce',
        values.nonce.toString(),
        '--priority-fee-per-gas',
        values.priorityFee.toString()
    ]);

    utils.log('Rolling back DB..');
    await runBlockReverter(pathToHome, chain, [
        'rollback-db',
        '--l1-batch-number',
        values.lastExecutedL1BatchNumber.toString(),
        '--rollback-postgres',
        '--rollback-tree',
        '--rollback-sk-cache',
        '--rollback-vm-runners-cache'
    ]);

    const blocksCommitted = await mainContract.getTotalBatchesCommitted();
    assert(blocksCommitted === values.lastExecutedL1BatchNumber, 'Revert on contract was unsuccessful');
    return values.lastExecutedL1BatchNumber;
}

export async function waitToCommitBatchesWithoutExecution(mainContract: IZkSyncHyperchain): Promise<bigint> {
    let batchesCommitted = await mainContract.getTotalBatchesCommitted();
    let batchesExecuted = await mainContract.getTotalBatchesExecuted();
    utils.log(`Batches committed: ${batchesCommitted}, executed: ${batchesExecuted}`);

    let tryCount = 0;
    while ((batchesExecuted === 0n || batchesCommitted === batchesExecuted) && tryCount < 100) {
        await utils.sleep(1);
        batchesCommitted = await mainContract.getTotalBatchesCommitted();
        batchesExecuted = await mainContract.getTotalBatchesExecuted();
        utils.log(`Batches committed: ${batchesCommitted}, executed: ${batchesExecuted}`);
        tryCount += 1;
    }
    expect(batchesCommitted > batchesExecuted, 'There is no committed but not executed batch').to.be.true;
    return batchesCommitted;
}

export async function executeDepositAfterRevert(tester: Tester, wallet: zksync.Wallet, amount: bigint) {
    const depositHandle = await tester.syncWallet.deposit({
        token: tester.isETHBasedChain ? zksync.utils.LEGACY_ETH_ADDRESS : tester.baseTokenAddress,
        amount,
        to: wallet.address,
        approveBaseERC20: true,
        approveERC20: true
    });

    let l1TxResponse = await wallet._providerL1().getTransaction(depositHandle.hash);
    while (!l1TxResponse) {
        utils.log(`Deposit ${depositHandle.hash} is not visible to the L1 network; sleeping`);
        await utils.sleep(1);
        l1TxResponse = await wallet._providerL1().getTransaction(depositHandle.hash);
    }
    utils.log(`Got L1 deposit tx`, l1TxResponse);

    // ethers doesn't work well with block reversions, so wait for the receipt before calling `.waitFinalize()`.
    const l2Tx = await wallet._providerL2().getL2TransactionFromPriorityOp(l1TxResponse);
    let receipt = null;
    while (receipt === null) {
        utils.log(`L2 deposit transaction ${l2Tx.hash} is not confirmed; sleeping`);
        await utils.sleep(1);
        receipt = await tester.syncWallet.provider.getTransactionReceipt(l2Tx.hash);
    }
    expect(receipt.status).to.be.eql(1);
    utils.log(`L2 deposit transaction ${l2Tx.hash} is confirmed`);

    await depositHandle.waitFinalize();
    utils.log('New deposit is finalized');
}

/** Returns sender's balance after the transfer is complete. */
export async function checkRandomTransfer(sender: zksync.Wallet, amount: bigint): Promise<bigint> {
    const senderBalanceBefore = await sender.getBalance();
    utils.log(`Sender's balance before transfer: ${senderBalanceBefore}`);

    const receiverHD = zksync.Wallet.createRandom();
    const receiver = new zksync.Wallet(receiverHD.privateKey, sender.provider);
    const transferHandle = await sender.sendTransaction({
        to: receiver.address,
        value: amount,
        type: 0
    });

    // ethers doesn't work well with block reversions, so we poll for the receipt manually.
    let txReceipt = null;
    while (txReceipt === null) {
        utils.log(`Transfer ${transferHandle.hash} is not confirmed, sleeping`);
        await utils.sleep(1);
        txReceipt = await sender.provider.getTransactionReceipt(transferHandle.hash);
    }

    const senderBalance = await sender.getBalance();
    utils.log(`Sender's balance after transfer: ${senderBalance}`);
    const receiverBalance = await receiver.getBalance();
    utils.log(`Receiver's balance after transfer: ${receiverBalance}`);

    assert(receiverBalance === amount, 'Failed updated the balance of the receiver');

    const spentAmount = txReceipt.gasUsed * transferHandle.gasPrice! + amount;
    utils.log(`Expected spent amount: ${spentAmount}`);
    assert(senderBalance + spentAmount >= senderBalanceBefore, 'Failed to update the balance of the sender');
    return senderBalance;
}
