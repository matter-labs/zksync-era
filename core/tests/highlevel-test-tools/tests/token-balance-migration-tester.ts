import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';
import * as utils from 'utils';
import { getMainWalletPk } from 'highlevel-test-tools/src/wallets';
import { createChainAndStartServer, TestChain, ChainType } from 'highlevel-test-tools/src/create-chain';
import path from 'path';
import { loadConfig } from 'utils/build/file-configs';
import { sleep } from 'zksync-ethers/build/utils';
import { L2_NATIVE_TOKEN_VAULT_ADDRESS } from 'utils/src/constants';
import * as fs from 'fs';
import * as yaml from 'js-yaml';
import { executeCommand, migrateToGatewayIfNeeded, startServer } from '../src';
import { removeErrorListeners } from '../src/execute-command';
import { initTestWallet } from '../src/run-integration-tests';

const RICH_WALLET_L1_BALANCE = ethers.parseEther('10.0');
const RICH_WALLET_L2_BALANCE = RICH_WALLET_L1_BALANCE;
const TEST_SUITE_NAME = 'Token Balance Migration Test';
const pathToHome = path.join(__dirname, '../../../..');

function readArtifact(contractName: string, outFolder: string = 'out', fileName: string = contractName) {
    return JSON.parse(
        fs
            .readFileSync(
                path.join(pathToHome, `./contracts/l1-contracts/${outFolder}/${contractName}.sol/${fileName}.json`)
            )
            .toString()
    );
}

const ERC20_EVM_ARTIFACT = readArtifact('TestnetERC20Token');
const ERC20_EVM_BYTECODE = ERC20_EVM_ARTIFACT.bytecode.object;
const ERC20_ABI = ERC20_EVM_ARTIFACT.abi;

const ERC20_ZKEVM_BYTECODE = readArtifact('TestnetERC20Token', 'zkout').bytecode.object;

export const DEFAULT_SMALL_AMOUNT = 1n;
export const DEFAULT_LARGE_AMOUNT = ethers.parseEther('0.01');

export async function generateChainRichWallet(chainName: string): Promise<zksync.Wallet> {
    const generalConfig = loadConfig({ pathToHome, chain: chainName, config: 'general.yaml' });
    const contractsConfig = loadConfig({ pathToHome, chain: chainName, config: 'contracts.yaml' });
    const secretsConfig = loadConfig({ pathToHome, chain: chainName, config: 'secrets.yaml' });
    const ethProviderAddress = secretsConfig.l1.l1_rpc_url;
    const web3JsonRpc = generalConfig.api.web3_json_rpc.http_url;

    const richWallet = new zksync.Wallet(
        getMainWalletPk(chainName),
        new zksync.Provider(web3JsonRpc),
        new ethers.JsonRpcProvider(ethProviderAddress)
    );

    if (contractsConfig.l1.base_token_addr !== zksync.utils.ETH_ADDRESS_IN_CONTRACTS) {
        const l1Token = new ethers.Contract(contractsConfig.l1.base_token_addr, ERC20_ABI, richWallet.ethWallet());
        const mintTx = await l1Token.mint(richWallet.address, 2n * RICH_WALLET_L2_BALANCE);
        await mintTx.wait();
    }

    // We deposit funds to ensure that the wallet is rich
    await (
        await richWallet.deposit({
            token: contractsConfig.l1.base_token_addr,
            amount: RICH_WALLET_L2_BALANCE,
            approveBaseERC20: true,
            approveERC20: true
        })
    ).wait();

    return richWallet;
}

function getL2Ntv(l2Wallet: zksync.Wallet) {
    const abi = readArtifact('L2NativeTokenVault').abi;
    return new zksync.Contract(L2_NATIVE_TOKEN_VAULT_ADDRESS, abi, l2Wallet);
}

export class ChainHandler {
    public inner: TestChain;
    public l2RichWallet: zksync.Wallet;
    public l1Ntv: ethers.Contract;
    public l1GettersContract: ethers.Contract;
    public gwGettersContract: zksync.Contract;

    constructor(inner: TestChain, l2RichWallet: zksync.Wallet) {
        this.inner = inner;
        this.l2RichWallet = l2RichWallet;

        const contractsConfig = loadConfig({ pathToHome, chain: inner.chainName, config: 'contracts.yaml' });
        this.l1Ntv = new ethers.Contract(
            contractsConfig.ecosystem_contracts.native_token_vault_addr,
            readArtifact('L1NativeTokenVault').abi,
            l2RichWallet.ethWallet()
        );

        this.l1GettersContract = new ethers.Contract(
            contractsConfig.l1.diamond_proxy_addr,
            readArtifact('Getters', 'out', 'GettersFacet').abi,
            l2RichWallet.ethWallet()
        );
    }

    async stopServer() {
        await this.inner.mainNode.kill();
        await this.waitForShutdown();
    }

    async startServer() {
        const newServerHandle = await startServer(this.inner.chainName);
        this.inner.mainNode = newServerHandle;
        // Need to wait for a bit before the server works fully
        await sleep(5000);
    }

    async waitForShutdown() {
        // Wait until it's really stopped.
        const generalConfig = loadConfig({ pathToHome, chain: this.inner.chainName, config: 'general.yaml' });
        const l2NodeUrl = generalConfig.api.web3_json_rpc.http_url;
        let iter = 0;
        while (iter < 30) {
            try {
                console.log(l2NodeUrl);
                let provider = new zksync.Provider(l2NodeUrl);
                await provider.getBlockNumber();
                await sleep(1);
                iter += 1;
            } catch (_) {
                // When exception happens, we assume that server died.
                return;
            }
        }
        // It's going to panic anyway, since the server is a singleton entity, so better to exit early.
        throw new Error(`${this.inner.chainName} didn't stop after a kill request`);
    }

    async migrateToGateway() {
        // Pause deposits before initiating migration
        await executeCommand(
            'zkstack',
            ['chain', 'pause-deposits', '--chain', this.inner.chainName],
            this.inner.chainName,
            'gateway_migration'
        );
        // Wait for priority queue to be empty
        await this.waitForPriorityQueueToBeEmpty(this.l1GettersContract);
        // Notify server
        await executeCommand(
            'zkstack',
            ['chain', 'gateway', 'notify-about-to-gateway-update', '--chain', this.inner.chainName],
            this.inner.chainName,
            'gateway_migration'
        );
        // Wait for all batches to be executed
        const tx = await this.l2RichWallet.sendTransaction({
            to: this.l2RichWallet.address,
            value: 1n,
            type: 0
        });
        await tx.wait();
        await utils.sleep(30);
        await waitForAllBatchesToBeExecuted(this.l1GettersContract);
        // We can now reliably migrate to gateway
        removeErrorListeners(this.inner.mainNode.process!);
        await migrateToGatewayIfNeeded(this.inner.chainName);
        await this.waitForShutdown();
        await this.startServer();

        // After migration, we can define the gateway getters contract
        const gatewayConfig = loadConfig({ pathToHome, chain: this.inner.chainName, config: 'gateway_chain.yaml' });
        const secretsConfig = loadConfig({ pathToHome, chain: this.inner.chainName, config: 'secrets.yaml' });
        this.gwGettersContract = new zksync.Contract(
            gatewayConfig.diamond_proxy_addr,
            readArtifact('Getters', 'out', 'GettersFacet').abi,
            new zksync.Provider(secretsConfig.l1.gateway_rpc_url)
        );
    }

    async migrateFromGateway() {
        // Pause deposits before initiating migration
        await executeCommand(
            'zkstack',
            ['chain', 'pause-deposits', '--chain', this.inner.chainName],
            this.inner.chainName,
            'gateway_migration'
        );
        // Wait for priority queue to be empty
        await this.waitForPriorityQueueToBeEmpty(this.l1GettersContract);
        // Notify server
        await executeCommand(
            'zkstack',
            ['chain', 'gateway', 'notify-about-from-gateway-update', '--chain', this.inner.chainName],
            this.inner.chainName,
            'gateway_migration'
        );
        // Wait for all batches to be executed
        const tx = await this.l2RichWallet.sendTransaction({
            to: this.l2RichWallet.address,
            value: 1n,
            type: 0
        });
        await tx.wait();
        await utils.sleep(30);
        await waitForAllBatchesToBeExecuted(this.gwGettersContract);
        // We can now reliably migrate from gateway
        removeErrorListeners(this.inner.mainNode.process!);
        await executeCommand(
            'zkstack',
            [
                'chain',
                'gateway',
                'migrate-from-gateway',
                '--gateway-chain-name',
                'gateway',
                '--chain',
                this.inner.chainName
            ],
            this.inner.chainName,
            'gateway_migration'
        );
        await this.waitForShutdown();
        await this.startServer();
    }

    async migrateTokenBalancesToGateway() {
        await executeCommand(
            'zkstack',
            [
                'chain',
                'gateway',
                'migrate-token-balances',
                '--to-gateway',
                'true',
                '--gateway-chain-name',
                'gateway',
                '--chain',
                this.inner.chainName
            ],
            this.inner.chainName,
            'token_balance_migration'
        );
    }

    async migrateTokenBalancesToL1() {
        await executeCommand(
            'zkstack',
            [
                'chain',
                'gateway',
                'migrate-token-balances',
                '--to-gateway',
                'false',
                '--gateway-chain-name',
                'gateway',
                '--chain',
                this.inner.chainName
            ],
            this.inner.chainName,
            'token_balance_migration'
        );
    }

    static async createNewChain(chainType: ChainType): Promise<ChainHandler> {
        const testChain = await createChainAndStartServer(chainType, TEST_SUITE_NAME, false);

        // We need to kill the server first to set the gateway RPC URL in secrets.yaml
        await testChain.mainNode.kill();
        // Wait a bit for clean shutdown
        await sleep(5000);

        // Set the gateway RPC URL before any migration operations
        const gatewayGeneralConfig = loadConfig({ pathToHome, chain: 'gateway', config: 'general.yaml' });
        const secretsPath = path.join(pathToHome, 'chains', testChain.chainName, 'configs', 'secrets.yaml');
        const secretsConfig = loadConfig({ pathToHome, chain: testChain.chainName, config: 'secrets.yaml' });
        secretsConfig.l1.gateway_rpc_url = gatewayGeneralConfig.api.web3_json_rpc.http_url;
        fs.writeFileSync(secretsPath, yaml.dump(secretsConfig));

        // Restart the server
        const newServerHandle = await startServer(testChain.chainName);
        testChain.mainNode = newServerHandle;
        // Need to wait for a bit before the server works fully
        await sleep(5000);
        await initTestWallet(testChain.chainName);

        return new ChainHandler(testChain, await generateChainRichWallet(testChain.chainName));
    }

    async deployNativeToken() {
        return await ERC20Handler.deployTokenOnL2(this.l2RichWallet);
    }

    private async waitForPriorityQueueToBeEmpty(gettersContract: ethers.Contract | zksync.Contract) {
        let tryCount = 0;
        while ((await gettersContract.getPriorityQueueSize()) > 0 && tryCount < 100) {
            tryCount += 1;
            await zksync.utils.sleep(this.l2RichWallet.provider.pollingInterval);
        }
    }
}

export class ERC20Handler {
    public wallet: zksync.Wallet;
    public l1Contract: ethers.Contract | undefined;
    public l2Contract: zksync.Contract | undefined;
    cachedAssetId: string | null = null;

    constructor(
        wallet: zksync.Wallet,
        l1Contract: ethers.Contract | undefined,
        l2Contract: zksync.Contract | undefined
    ) {
        this.wallet = wallet;
        this.l1Contract = l1Contract;
        this.l2Contract = l2Contract;
    }

    async assetId(chainHandler?: ChainHandler): Promise<string> {
        if (this.cachedAssetId) return this.cachedAssetId;

        let assetId: string;
        if (this.l1Contract) {
            if (!chainHandler) throw new Error('Chain handler must be provided');
            assetId = await chainHandler.l1Ntv.assetId(await this.l1Contract.getAddress());
        } else {
            const l2Ntv = getL2Ntv(this.wallet);
            assetId = await l2Ntv.assetId(await this.l2Contract!.getAddress());
        }
        this.cachedAssetId = assetId;
        return assetId;
    }

    async deposit(
        chainHandler: ChainHandler,
        awaitDeposit = false,
        amount: bigint = DEFAULT_SMALL_AMOUNT
    ): Promise<bigint> {
        const depositAmount = amount ?? ethers.parseUnits((Math.floor(Math.random() * 900) + 100).toString(), 'gwei');
        const depositTx = await this.wallet.deposit({
            token: await this.l1Contract!.getAddress(),
            amount: depositAmount,
            approveERC20: true,
            approveBaseERC20: true
        });
        await depositTx.wait();

        await this.setL2Contract(chainHandler);
        if (awaitDeposit) await waitForBalanceNonZero(this.l2Contract!, this.wallet);

        return depositAmount;
    }

    async withdraw(amount: bigint = DEFAULT_SMALL_AMOUNT): Promise<WithdrawalHandler> {
        const withdrawAmount = amount ?? ethers.parseUnits((Math.floor(Math.random() * 900) + 100).toString(), 'gwei');
        await this.registerIfNeeded();

        if ((await this.l2Contract!.allowance(this.wallet.address, L2_NATIVE_TOKEN_VAULT_ADDRESS)) < amount) {
            await (await this.l2Contract!.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, 0)).wait();
            await (await this.l2Contract!.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, amount)).wait();
        }

        const withdrawTx = await this.wallet.withdraw({
            token: await this.l2Contract!.getAddress(),
            amount: withdrawAmount
        });
        await withdrawTx.wait();

        return new WithdrawalHandler(withdrawTx.hash, this.wallet.provider, withdrawAmount);
    }

    async setL2Contract(chainHandler: ChainHandler) {
        // After a deposit we can define the l2 contract if it wasn't already
        if (this.l2Contract) return;
        const l2Address = await getL2Ntv(this.wallet).tokenAddress(await this.assetId(chainHandler));
        this.l2Contract = new zksync.Contract(l2Address, ERC20_ABI, this.wallet);
    }

    async setL1Contract(chainHandler: ChainHandler) {
        // After a withdrawal we can define the l1 contract if it wasn't already
        if (this.l1Contract) return;
        const l1Address = await chainHandler.l1Ntv.tokenAddress(await this.assetId());
        this.l1Contract = new ethers.Contract(l1Address, ERC20_ABI, this.wallet.ethWallet());
    }

    async getL1Balance() {
        return await this.l1Contract!.balanceOf(this.wallet.address);
    }

    async getL2Balance() {
        return await this.l2Contract!.balanceOf(this.wallet.address);
    }

    static async deployTokenOnL1(wallet: zksync.Wallet) {
        const l1Wallet = wallet.ethWallet();
        const factory = new ethers.ContractFactory(ERC20_ABI, ERC20_EVM_BYTECODE, l1Wallet);

        const props = this.generateRandomTokenProps();
        const newToken = await factory.deploy(props.name, props.symbol, props.decimals);
        await newToken.waitForDeployment();
        const l1Contract = new ethers.Contract(await newToken.getAddress(), ERC20_ABI, l1Wallet);
        await (await l1Contract.mint(l1Wallet.address, RICH_WALLET_L1_BALANCE)).wait();

        return new ERC20Handler(wallet, l1Contract, undefined);
    }

    static async deployTokenOnL2(l2Wallet: zksync.Wallet) {
        const factory = new zksync.ContractFactory(ERC20_ABI, ERC20_ZKEVM_BYTECODE, l2Wallet, 'create');

        const props = this.generateRandomTokenProps();
        const newToken = await factory.deploy(props.name, props.symbol, props.decimals);
        await newToken.waitForDeployment();
        const l2Contract = new zksync.Contract(await newToken.getAddress(), ERC20_ABI, l2Wallet);
        await (await l2Contract.mint(l2Wallet.address, RICH_WALLET_L1_BALANCE)).wait();

        return new ERC20Handler(l2Wallet, undefined, l2Contract);
    }

    private async registerIfNeeded() {
        const l2Ntv = getL2Ntv(this.wallet);
        const l2AssetId = await l2Ntv.assetId(await this.l2Contract!.getAddress());
        if (l2AssetId === ethers.ZeroHash) {
            // Registering the token
            await (await l2Ntv.registerToken(await this.l2Contract!.getAddress())).wait();
        }
    }

    private static generateRandomTokenProps() {
        const name = 'NAME-' + ethers.hexlify(ethers.randomBytes(4));
        const symbol = 'SYM-' + ethers.hexlify(ethers.randomBytes(4));
        const decimals = Math.min(Math.floor(Math.random() * 18) + 1, 18);

        return { name, symbol, decimals };
    }
}

export class WithdrawalHandler {
    public txHash: string;
    public l2Provider: zksync.Provider;
    public amount: bigint;

    constructor(txHash: string, provider: zksync.Provider, amount: bigint) {
        this.txHash = txHash;
        this.l2Provider = provider;
        this.amount = amount;
    }

    async finalizeWithdrawal(l1RichWallet: ethers.Wallet) {
        // Firstly, we've need to wait for the batch to be finalized.
        const l2Wallet = new zksync.Wallet(l1RichWallet.privateKey, this.l2Provider, l1RichWallet.provider!);

        const receipt = await l2Wallet.provider.getTransactionReceipt(this.txHash);
        if (!receipt) {
            throw new Error('Receipt');
        }

        await waitForL2ToL1LogProof(l2Wallet, receipt.blockNumber, this.txHash);

        await (await l2Wallet.finalizeWithdrawal(this.txHash)).wait();
    }
}

export class MigrationHandler {
    public txHash: string;

    constructor(txHash: string, provider: zksync.Provider) {
        this.txHash = txHash;
    }

    async finalizeMigration(l1RichWallet: ethers.Wallet) {}
}

async function waitForBalanceNonZero(contract: ethers.Contract | zksync.Contract, wallet: zksync.Wallet) {
    let balance;
    while (true) {
        balance = await contract.balanceOf(wallet.address);
        console.log('Waiting for balance to be non-zero', balance);
        if (balance !== 0n) break;
        await zksync.utils.sleep(wallet.provider.pollingInterval);
    }
}

async function waitUntilBlockFinalized(wallet: zksync.Wallet, blockNumber: number) {
    let printedBlockNumber = 0;
    while (true) {
        const block = await wallet.provider.getBlock('finalized');
        console.log('block number', block.number, blockNumber);
        if (blockNumber <= block.number) {
            break;
        } else {
            if (printedBlockNumber < block.number) {
                printedBlockNumber = block.number;
            }
            await zksync.utils.sleep(wallet.provider.pollingInterval);
        }
    }
}

async function waitForL2ToL1LogProof(wallet: zksync.Wallet, blockNumber: number, txHash: string) {
    console.log('waiting for block to be finalized');
    // First, we wait for block to be finalized.
    await waitUntilBlockFinalized(wallet, blockNumber);

    console.log('waiting for log proof');
    // Second, we wait for the log proof.
    let i = 0;
    while ((await wallet.provider.getLogProof(txHash)) == null) {
        console.log(`Waiting for log proof... ${i}`);
        await zksync.utils.sleep(wallet.provider.pollingInterval);
        i++;
    }
}

async function waitForAllBatchesToBeExecuted(gettersContract: ethers.Contract | zksync.Contract) {
    let tryCount = 0;
    let totalBatchesCommitted = await gettersContract.getTotalBatchesCommitted();
    let totalBatchesExecuted = await gettersContract.getTotalBatchesExecuted();
    while (totalBatchesCommitted !== totalBatchesExecuted && tryCount < 100) {
        tryCount += 1;
        await utils.sleep(1);
        totalBatchesCommitted = await gettersContract.getTotalBatchesCommitted();
        totalBatchesExecuted = await gettersContract.getTotalBatchesExecuted();
    }
}
