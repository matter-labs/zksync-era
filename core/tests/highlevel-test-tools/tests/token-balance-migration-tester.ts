import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';
import * as utils from 'utils';
import * as yaml from 'js-yaml';
import * as fs from 'fs';
import path from 'path';
import { loadConfig, loadEcosystemConfig } from 'utils/build/file-configs';
import { sleep } from 'zksync-ethers/build/utils';
import { getMainWalletPk } from 'highlevel-test-tools/src/wallets';
import { createChainAndStartServer, TestChain, ChainType } from 'highlevel-test-tools/src/create-chain';
import {
    L2_NATIVE_TOKEN_VAULT_ADDRESS,
    L2_ASSET_TRACKER_ADDRESS,
    GW_ASSET_TRACKER_ADDRESS,
    GATEWAY_CHAIN_ID
} from 'utils/src/constants';
import { executeCommand, migrateToGatewayIfNeeded, startServer } from '../src';
import { removeErrorListeners } from '../src/execute-command';
import { initTestWallet } from '../src/run-integration-tests';

export const RICH_WALLET_L1_BALANCE = ethers.parseEther('10.0');
export const RICH_WALLET_L2_BALANCE = RICH_WALLET_L1_BALANCE;
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

const AMOUNT_FLOOR = ethers.parseEther('0.01');
const AMOUNT_CEILING = ethers.parseEther('1');

type AssetTrackerLocation = 'L1AT' | 'L1AT_GW' | 'GWAT';
const ASSET_TRACKERS: readonly AssetTrackerLocation[] = ['L1AT', 'L1AT_GW', 'GWAT'] as const;

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

    // Additionally fund the deployer wallet
    // This skips the funding step on migrate_token_balances.rs, and allows for easier chain balance accounting
    const ecosystemWallets = loadEcosystemConfig({ pathToHome, config: 'wallets.yaml' });
    await (
        await richWallet.transfer({
            to: ecosystemWallets.deployer.address,
            amount: ethers.parseEther('1')
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
    public l2Ntv: zksync.Contract;
    public l1AssetTracker: ethers.Contract;
    public gwAssetTracker: zksync.Contract;
    public l2AssetTracker: zksync.Contract;
    public baseTokenAssetId: string;
    public l1BaseTokenContract: ethers.Contract;
    public l1GettersContract: ethers.Contract;
    public gwGettersContract: zksync.Contract;

    //
    public existingBaseTokenL1ATBalanceForGW = 0n;

    // Records expected chain balances for each token asset ID
    public chainBalances: Record<string, bigint> = {};

    constructor(inner: TestChain, l2RichWallet: zksync.Wallet) {
        const contractsConfig = loadConfig({ pathToHome, chain: inner.chainName, config: 'contracts.yaml' });

        this.inner = inner;
        this.l2RichWallet = l2RichWallet;

        this.l1Ntv = new ethers.Contract(
            contractsConfig.ecosystem_contracts.native_token_vault_addr,
            readArtifact('L1NativeTokenVault').abi,
            l2RichWallet.ethWallet()
        );
        this.l2Ntv = getL2Ntv(l2RichWallet);
        this.l2AssetTracker = new zksync.Contract(
            L2_ASSET_TRACKER_ADDRESS,
            readArtifact('L2AssetTracker').abi,
            l2RichWallet
        );

        this.baseTokenAssetId = contractsConfig.l1.base_token_asset_id;
        this.l1BaseTokenContract = new ethers.Contract(
            contractsConfig.l1.base_token_addr,
            ERC20_ABI,
            l2RichWallet.ethWallet()
        );
        this.l1GettersContract = new ethers.Contract(
            contractsConfig.l1.diamond_proxy_addr,
            readArtifact('Getters', 'out', 'GettersFacet').abi,
            l2RichWallet.ethWallet()
        );
    }

    async initEcosystemContracts(gwWallet: zksync.Wallet) {
        const l1AssetTrackerAddr = await this.l1Ntv.l1AssetTracker();
        this.l1AssetTracker = new ethers.Contract(
            l1AssetTrackerAddr,
            readArtifact('L1AssetTracker').abi,
            this.l2RichWallet.ethWallet()
        );
        this.existingBaseTokenL1ATBalanceForGW = await this.l1AssetTracker.chainBalance(
            GATEWAY_CHAIN_ID,
            this.baseTokenAssetId
        );
        this.gwAssetTracker = new zksync.Contract(
            GW_ASSET_TRACKER_ADDRESS,
            readArtifact('GWAssetTracker').abi,
            gwWallet
        );
    }

    async assertAssetTrackersState(
        assetId: string,
        {
            balances = {},
            migrations
        }: {
            balances?: Partial<Record<AssetTrackerLocation, bigint>>;
            migrations?: Record<AssetTrackerLocation, bigint>;
        }
    ): Promise<boolean> {
        for (const where of ASSET_TRACKERS) {
            const expected = balances?.[where];
            if (expected !== undefined) {
                await this.assertChainBalance(assetId, where, expected);
            } else {
                await this.assertChainBalance(assetId, where);
            }
        }

        if (migrations) {
            for (const where of ASSET_TRACKERS) {
                await this.assertAssetMigrationNumber(assetId, where, migrations[where]);
            }
        }

        return true;
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
        // Wait for all batches to be executed
        await this.inner.waitForAllBatchesToBeExecuted();
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

        // Redefine
        this.existingBaseTokenL1ATBalanceForGW = await this.l1AssetTracker.chainBalance(
            GATEWAY_CHAIN_ID,
            this.baseTokenAssetId
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
        // Wait for all batches to be executed
        await this.inner.waitForAllBatchesToBeExecuted();
        // Notify server
        await executeCommand(
            'zkstack',
            ['chain', 'gateway', 'notify-about-from-gateway-update', '--chain', this.inner.chainName],
            this.inner.chainName,
            'gateway_migration'
        );
        // We can now reliably migrate from gateway
        await this.stopServer();
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

    private async assertChainBalance(
        assetId: string,
        where: 'L1AT' | 'L1AT_GW' | 'GWAT',
        expectedBalance?: bigint
    ): Promise<boolean> {
        let balance = expectedBalance ?? this.chainBalances[assetId] ?? 0n;
        let contract: ethers.Contract | zksync.Contract;
        if (where === 'L1AT' || where === 'L1AT_GW') {
            contract = this.l1AssetTracker;
        } else if (where === 'GWAT') {
            contract = this.gwAssetTracker;
        }
        const chainId = where === 'L1AT_GW' ? GATEWAY_CHAIN_ID : this.inner.chainId;
        balance +=
            where === 'L1AT_GW' && assetId === this.baseTokenAssetId ? this.existingBaseTokenL1ATBalanceForGW : 0n;
        const actualBalance = await contract!.chainBalance(chainId, assetId);

        if (assetId === this.baseTokenAssetId && (where === 'GWAT' || where === 'L1AT')) {
            // Here we have to account for some balance drift from the migrate_token_balances.rs script
            const tolerance = ethers.parseEther('0.0015');
            const diff = actualBalance > balance ? actualBalance - balance : balance - actualBalance;
            if (diff > tolerance) {
                throw new Error(`Balance mismatch for ${where} ${assetId}: expected ${balance}, got ${actualBalance}`);
            }
            return true;
        }

        if (actualBalance !== balance) {
            throw new Error(`Balance mismatch for ${where} ${assetId}: expected ${balance}, got ${actualBalance}`);
        }
        return true;
    }

    private async assertAssetMigrationNumber(
        assetId: string,
        where: 'L1AT' | 'L1AT_GW' | 'GWAT',
        expectedMigrationNumber: bigint
    ): Promise<boolean> {
        let contract: ethers.Contract | zksync.Contract;
        if (where === 'L1AT' || where === 'L1AT_GW') {
            contract = this.l1AssetTracker;
        } else if (where === 'GWAT') {
            contract = this.gwAssetTracker;
        }
        // After migration to gateway, the chain balances of the chain on L1AT are accounted for inside GW's chain balance
        const chainId = where === 'L1AT_GW' ? GATEWAY_CHAIN_ID : this.inner.chainId;
        const actualMigrationNumber = await contract!.assetMigrationNumber(chainId, assetId);
        if (actualMigrationNumber !== expectedMigrationNumber) {
            throw new Error(
                `Asset migration number mismatch for ${where} ${assetId}: expected ${expectedMigrationNumber}, got ${actualMigrationNumber}`
            );
        }
        return true;
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
    public isL2Token: boolean;
    cachedAssetId: string | null = null;

    constructor(
        wallet: zksync.Wallet,
        l1Contract: ethers.Contract | undefined,
        l2Contract: zksync.Contract | undefined
    ) {
        this.wallet = wallet;
        this.l1Contract = l1Contract;
        this.l2Contract = l2Contract;
        this.isL2Token = !!l2Contract;
    }

    async assetId(chainHandler: ChainHandler): Promise<string> {
        if (this.cachedAssetId) return this.cachedAssetId;

        let assetId: string;
        if (this.l1Contract) {
            assetId = await chainHandler.l1Ntv.assetId(await this.l1Contract.getAddress());
        } else {
            assetId = await chainHandler.l2Ntv.assetId(await this.l2Contract!.getAddress());
        }
        if (assetId !== ethers.ZeroHash) this.cachedAssetId = assetId;
        return assetId;
    }

    async deposit(chainHandler: ChainHandler, amount?: bigint): Promise<bigint> {
        const depositAmount = amount ?? getRandomDepositAmount();
        const depositTx = await this.wallet.deposit({
            token: await this.l1Contract!.getAddress(),
            amount: depositAmount,
            approveERC20: true,
            approveBaseERC20: true
        });
        await depositTx.wait();

        await this.setL2Contract(chainHandler);
        await waitForBalanceNonZero(this.l2Contract!, this.wallet);

        const assetId = await this.assetId(chainHandler);
        chainHandler.chainBalances[assetId] = (chainHandler.chainBalances[assetId] ?? 0n) + depositAmount;

        return depositAmount;
    }

    async withdraw(
        chainHandler: ChainHandler,
        decreaseChainBalance = true,
        amount?: bigint
    ): Promise<WithdrawalHandler> {
        const withdrawAmount = amount ?? getRandomWithdrawAmount();

        if ((await this.l2Contract!.allowance(this.wallet.address, L2_NATIVE_TOKEN_VAULT_ADDRESS)) < withdrawAmount) {
            await (await this.l2Contract!.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, 0)).wait();
            await (await this.l2Contract!.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, withdrawAmount)).wait();
        }

        const withdrawTx = await this.wallet.withdraw({
            token: await this.l2Contract!.getAddress(),
            amount: withdrawAmount
        });
        await withdrawTx.wait();

        const assetId = await this.assetId(chainHandler);
        if (decreaseChainBalance) {
            if (this.isL2Token) chainHandler.chainBalances[assetId] = ethers.MaxUint256;
            chainHandler.chainBalances[assetId] -= withdrawAmount;
        }

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
        const l1Address = await chainHandler.l1Ntv.tokenAddress(await this.assetId(chainHandler));
        this.l1Contract = new ethers.Contract(l1Address, ERC20_ABI, this.wallet.ethWallet());
    }

    async getL1Contract(chainHandler: ChainHandler): Promise<ethers.Contract> {
        const l1Address = await chainHandler.l1Ntv.tokenAddress(await this.assetId(chainHandler));
        if (l1Address === ethers.ZeroAddress) throw new Error('L1 token not found');
        return new ethers.Contract(l1Address, ERC20_ABI, this.wallet.ethWallet());
    }

    async getL1Balance() {
        return await this.l1Contract!.balanceOf(this.wallet.address);
    }

    async getL2Balance() {
        return await this.l2Contract!.balanceOf(this.wallet.address);
    }

    static async fromL2BL1Token(l1Contract: ethers.Contract, wallet: zksync.Wallet, secondChainWallet: zksync.Wallet) {
        // L2-B wallet must hold some balance of the L2-B token on L1
        const balance = await secondChainWallet.getBalanceL1(await l1Contract.getAddress());
        if (balance === 0n) throw new Error('L2-B wallet must hold some balance of the L2-B token on L1');
        // We need to provide the chain rich wallet with some balance of the L2-B token on L1, to
        const l1Erc20 = new ethers.Contract(await l1Contract.getAddress(), ERC20_ABI, secondChainWallet.ethWallet());
        await (await l1Erc20.transfer(wallet.address, balance)).wait();
        return new ERC20Handler(wallet, l1Contract, undefined);
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

    static async deployTokenOnL2(chainHandler: ChainHandler, _mintAmount?: bigint) {
        const mintAmount = _mintAmount ?? getRandomDepositAmount();
        const factory = new zksync.ContractFactory(
            ERC20_ABI,
            ERC20_ZKEVM_BYTECODE,
            chainHandler.l2RichWallet,
            'create'
        );

        const props = this.generateRandomTokenProps();
        const newToken = await factory.deploy(props.name, props.symbol, props.decimals);
        await newToken.waitForDeployment();
        const l2Contract = new zksync.Contract(await newToken.getAddress(), ERC20_ABI, chainHandler.l2RichWallet);
        await (await l2Contract.mint(chainHandler.l2RichWallet.address, mintAmount)).wait();

        await (await chainHandler.l2Ntv.registerToken(await l2Contract.getAddress())).wait();

        return new ERC20Handler(chainHandler.l2RichWallet, undefined, l2Contract);
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

function getRandomDepositAmount(): bigint {
    return AMOUNT_FLOOR + BigInt(Math.floor(Math.random() * Number(AMOUNT_CEILING - AMOUNT_FLOOR + 1n)));
}

function getRandomWithdrawAmount(): bigint {
    return 1n + BigInt(Math.floor(Math.random() * Number(AMOUNT_FLOOR / 2n - 1n)));
}

async function waitForBalanceNonZero(contract: ethers.Contract | zksync.Contract, wallet: zksync.Wallet) {
    let balance;
    while (true) {
        balance = await contract.balanceOf(wallet.address);
        if (balance !== 0n) break;
        await zksync.utils.sleep(wallet.provider.pollingInterval);
    }
}

async function waitUntilBlockFinalized(wallet: zksync.Wallet, blockNumber: number) {
    let printedBlockNumber = 0;
    while (true) {
        const block = await wallet.provider.getBlock('finalized');
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
    // First, we wait for block to be finalized.
    await waitUntilBlockFinalized(wallet, blockNumber);

    // Second, we wait for the log proof.
    let i = 0;
    while ((await wallet.provider.getLogProof(txHash)) == null) {
        await zksync.utils.sleep(wallet.provider.pollingInterval);
        i++;
    }
}
