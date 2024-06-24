import * as fs from 'fs';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import * as hre from 'hardhat';
import { ZkSyncArtifact } from '@matterlabs/hardhat-zksync-solc/dist/src/types';

export const SYSTEM_CONTEXT_ADDRESS = '0x000000000000000000000000000000000000800b';

/**
 * Loads the test contract
 *
 * @param name Name of the contract, e.g. `Counter`
 * @returns Artifact containing the bytecode and ABI of the contract.
 */
export function getTestContract(name: string): ZkSyncArtifact {
    const artifact = hre.artifacts.readArtifactSync(name);
    return artifact as ZkSyncArtifact;
}

/**
 * Loads the `*.sol` file for a test contract.
 *
 * @param relativePath Path relative to the `ts-integration/contracts` folder (e.g. `contra).
 * @returns Conta
 */
export function getContractSource(relativePath: string): string {
    const contractPath = `${__dirname}/../contracts/${relativePath}`;
    const source = fs.readFileSync(contractPath, 'utf8');
    return source;
}

/**
 * Performs a contract deployment
 *
 * @param initiator Wallet from which contract should be deployed
 * @param artifact ABI and bytecode of the contract
 * @param args Constructor arguments for the contract
 * @param deploymentType Optional: should be set to `createAccount` if deployed contract would represent an account.
 * @param overrides Optional: overrides for the deployment transaction.
 * @returns Deployed contract object (with `initiator` wallet attached).
 */
export async function deployContract(
    initiator: zksync.Wallet,
    artifact: ZkSyncArtifact,
    args: any[],
    deploymentType?: zksync.types.DeploymentType,
    overrides: any = {}
): Promise<zksync.Contract> {
    const contractFactory = new zksync.ContractFactory(artifact.abi, artifact.bytecode, initiator, deploymentType);
    const contract = await contractFactory.deploy(...args, overrides);
    await contract.deployed();
    return contract;
}

/**
 * Just performs a transaction. Can be used when you don't care about a particular action,
 * but just need a transaction to be executed.
 *
 * @param wallet Wallet to send a transaction from. Should have enough balance to cover the fee.
 * @returns Transaction receipt.
 */
export async function anyTransaction(wallet: zksync.Wallet): Promise<ethers.providers.TransactionReceipt> {
    return await wallet.transfer({ to: wallet.address, amount: 0 }).then((tx) => tx.wait());
}

/**
 * Waits until a new L1 batch is created on ZKsync node.
 * This function attempts to trigger this action by sending an additional transaction,
 * however it may not be enough in some env (e.g. if some testnet is configured to utilize the block capacity).
 *
 * @param wallet Wallet to send transaction from. Should have enough balance to cover the fee.
 */
export async function waitForNewL1Batch(wallet: zksync.Wallet): Promise<zksync.types.TransactionReceipt> {
    // Send a dummy transaction and wait until the new L1 batch is created.
    const oldReceipt = await anyTransaction(wallet);
    // Invariant: even with 1 transaction, l1 batch must be eventually sealed, so this loop must exit.
    while (!(await wallet.provider.getTransactionReceipt(oldReceipt.transactionHash)).l1BatchNumber) {
        await zksync.utils.sleep(wallet.provider.pollingInterval);
    }
    return await wallet.provider.getTransactionReceipt(oldReceipt.transactionHash);
}

/**
 * Waits until the requested block is finalized.
 *
 * @param wallet Wallet to use to poll the server.
 * @param blockNumber Number of block.
 */
export async function waitUntilBlockFinalized(wallet: zksync.Wallet, blockNumber: number) {
    while (true) {
        const block = await wallet.provider.getBlock('finalized');
        if (blockNumber <= block.number) {
            break;
        } else {
            await zksync.utils.sleep(wallet.provider.pollingInterval);
        }
    }
}

/**
 * Returns an increased gas price to decrease chances of L1 transactions being stuck
 *
 * @param wallet Wallet to use to fetch the gas price.
 * @returns Scaled gas price.
 */
export async function scaledGasPrice(wallet: ethers.Wallet | zksync.Wallet): Promise<ethers.BigNumber> {
    const gasPrice = await wallet.getGasPrice();
    // Increase by 40%
    return gasPrice.mul(140).div(100);
}
