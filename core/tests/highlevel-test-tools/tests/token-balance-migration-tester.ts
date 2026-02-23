import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';
import * as utils from 'utils';
import * as yaml from 'js-yaml';
import * as fs from 'fs';
import path from 'path';
import { expect } from 'vitest';
import { loadConfig, loadEcosystemConfig } from 'utils/build/file-configs';
import { sleep } from 'zksync-ethers/build/utils';
import { getMainWalletPk } from 'highlevel-test-tools/src/wallets';
import { createChainAndStartServer, TestChain, ChainType } from 'highlevel-test-tools/src/create-chain';
import {
    L2_NATIVE_TOKEN_VAULT_ADDRESS,
    L2_ASSET_TRACKER_ADDRESS,
    GW_ASSET_TRACKER_ADDRESS,
    GATEWAY_CHAIN_ID,
    L2_BRIDGEHUB_ADDRESS,
    L2_INTEROP_ROOT_STORAGE_ADDRESS,
    L2_ASSET_ROUTER_ADDRESS,
    L2_INTEROP_CENTER_ADDRESS,
    L2_INTEROP_HANDLER_ADDRESS,
    ArtifactL2InteropRootStorage,
    ArtifactIBridgehubBase,
    ArtifactIGetters
} from 'utils/src/constants';
import { executeCommand, FileMutex, migrateToGatewayIfNeeded, agreeToPaySettlementFees, startServer } from '../src';
import { removeErrorListeners } from '../src/execute-command';
import { initTestWallet } from '../src/run-integration-tests';
import { formatEvmV1Address, formatEvmV1Chain, getInteropBundleData } from '../src/temp-sdk';

const tbmMutex = new FileMutex();
export const RICH_WALLET_L2_BALANCE = ethers.parseEther('10.0');
export const TOKEN_MINT_AMOUNT = ethers.parseEther('1.0');
const MAX_WITHDRAW_AMOUNT = ethers.parseEther('0.1');
const TEST_SUITE_NAME = 'Token Balance Migration Test';
const pathToHome = path.join(__dirname, '../../../..');

export async function expectRevertWithSelector(
    action: Promise<unknown>,
    selector: string,
    failureMessage = 'Expected transaction to revert with selector'
): Promise<void> {
    try {
        await action;
        expect.fail(`${failureMessage} ${selector}`);
    } catch (err) {
        const errorText = [
            (err as any)?.data,
            (err as any)?.error?.data,
            (err as any)?.info?.error?.data,
            (err as any)?.shortMessage,
            (err as any)?.message
        ]
            .filter(Boolean)
            .join(' ');
        expect(errorText).toContain(selector);
    }
}

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

export const INTEROP_TEST_AMOUNT = 1_000_000_000n;
const INTEROP_CENTER_ABI = readArtifact('InteropCenter').abi;
const INTEROP_HANDLER_ABI = readArtifact('InteropHandler').abi;
const ERC7786_ATTR_INTERFACE = new ethers.Interface(readArtifact('IERC7786Attributes').abi);

type AssetTrackerLocation = 'L1AT' | 'L1AT_GW' | 'GWAT';
const ASSET_TRACKERS: readonly AssetTrackerLocation[] = ['L1AT', 'L1AT_GW', 'GWAT'] as const;

export interface InteropCallStarter {
    to: string;
    data: string;
    callAttributes: string[];
}

export type MessageInclusionProof = {
    chainId: bigint;
    l1BatchNumber: number;
    l2MessageIndex: number;
    message: [number, string, string];
    proof: string[];
};

export type InteropBundleData = {
    rawData: string | null;
    bundleHash: string;
    output: any | null;
    l1BatchNumber: number;
    l2TxNumberInBlock: number;
    l2MessageIndex: number;
    proof: MessageInclusionProof | null;
};

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
    // Interop sends increase the recipient-side balance tracked on L1AT_GW for each asset.
    public interopGatewayIncreases: Record<string, bigint> = {};

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
        // Fix baseTokenAssetId: js-yaml parses unquoted hex as a lossy JS number.
        // Query the on-chain NTV for the authoritative asset ID string.
        this.baseTokenAssetId = await this.l1Ntv.assetId(await this.l1BaseTokenContract.getAddress());

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
        const failures: string[] = [];
        const recordFailure = (where: AssetTrackerLocation, err: unknown) => {
            const reason = err instanceof Error ? err.message : String(err);
            failures.push(`[${where}] ${reason}`);
        };

        for (const where of ASSET_TRACKERS) {
            // Since we have several chains settling on the same gateway in parallel, accounting
            // for the base token on L1AT_GW can be very tricky, so we just skip it.
            if (where === 'L1AT_GW' && assetId === this.baseTokenAssetId) continue;
            const expected = balances?.[where];
            try {
                if (expected !== undefined) {
                    await this.assertChainBalance(assetId, where, expected);
                } else {
                    await this.assertChainBalance(assetId, where);
                }
            } catch (err) {
                recordFailure(where, err);
            }
        }

        if (migrations) {
            for (const where of ASSET_TRACKERS) {
                try {
                    await this.assertAssetMigrationNumber(assetId, where, migrations[where]);
                } catch (err) {
                    recordFailure(where, err);
                }
            }
        }

        if (failures.length > 0) {
            const message = `Asset tracker assertion failures:\n${failures.map((f) => `- ${f}`).join('\n')}`;
            console.error(message);
            throw new Error(message);
        }

        return true;
    }

    async stopServer() {
        await this.inner.mainNode.kill();
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
        await this.trackBaseTokenDelta(async () => {
            // Pause deposits before initiating migration
            await this.zkstackExecWithMutex(
                ['chain', 'pause-deposits', '--chain', this.inner.chainName],
                'pausing deposits before initiating migration',
                'gateway_migration'
            );
            // Wait for priority queue to be empty
            await this.waitForPriorityQueueToBeEmpty(this.l1GettersContract);
            // Wait for all batches to be executed
            await this.inner.waitForAllBatchesToBeExecuted();
            // We can now reliably migrate to gateway
            removeErrorListeners(this.inner.mainNode.process!);
            await migrateToGatewayIfNeeded(this.inner.chainName);
            await agreeToPaySettlementFees(this.inner.chainName);

            await this.waitForShutdown();
            await this.startServer();
        });

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
        await this.trackBaseTokenDelta(async () => {
            // Pause deposits before initiating migration
            await this.zkstackExecWithMutex(
                ['chain', 'pause-deposits', '--chain', this.inner.chainName],
                'pausing deposits before initiating migration',
                'gateway_migration'
            );
            // Wait for priority queue to be empty
            await this.waitForPriorityQueueToBeEmpty(this.l1GettersContract);
            // Wait for all batches to be executed
            await this.inner.waitForAllBatchesToBeExecuted();
            // Notify server
            await this.zkstackExecWithMutex(
                ['chain', 'gateway', 'notify-about-from-gateway-update', '--chain', this.inner.chainName],
                'notifying about from gateway update',
                'gateway_migration'
            );
            // We can now reliably migrate from gateway
            removeErrorListeners(this.inner.mainNode.process!);
            await this.zkstackExecWithMutex(
                [
                    'chain',
                    'gateway',
                    'migrate-from-gateway',
                    '--gateway-chain-name',
                    'gateway',
                    '--chain',
                    this.inner.chainName
                ],
                'migrating from gateway',
                'gateway_migration'
            );
            await this.waitForShutdown();
            await this.startServer();
        });
    }

    async initiateTokenBalanceMigration(direction: 'to-gateway' | 'from-gateway') {
        await this.trackBaseTokenDelta(async () => {
            await executeCommand(
                'zkstack',
                [
                    'chain',
                    'gateway',
                    'initiate-token-balance-migration',
                    '--to-gateway',
                    String(direction === 'to-gateway'),
                    '--gateway-chain-name',
                    'gateway',
                    '--chain',
                    this.inner.chainName
                ],
                this.inner.chainName,
                'token_balance_migration'
            );
        });
    }

    async finalizeTokenBalanceMigration(direction: 'to-gateway' | 'from-gateway') {
        await this.trackBaseTokenDelta(async () => {
            await executeCommand(
                'zkstack',
                [
                    'chain',
                    'gateway',
                    'finalize-token-balance-migration',
                    '--to-gateway',
                    String(direction === 'to-gateway'),
                    '--gateway-chain-name',
                    'gateway',
                    '--chain',
                    this.inner.chainName
                ],
                this.inner.chainName,
                'token_balance_migration'
            );
        });
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

    private async zkstackExecWithMutex(command: string[], name: string, logFileName: string) {
        try {
            // Acquire mutex for zkstack exec
            console.log(`🔒 Acquiring mutex for ${name} of ${this.inner.chainName}...`);
            await tbmMutex.acquire();
            console.log(`✅ Mutex acquired for ${name} of ${this.inner.chainName}`);

            try {
                await executeCommand('zkstack', command, this.inner.chainName, logFileName);

                console.log(`✅ Successfully executed ${name} for chain ${this.inner.chainName}`);
            } finally {
                // Always release the mutex
                tbmMutex.release();
            }
        } catch (e) {
            console.error(`❌ Failed to execute ${name} for chain ${this.inner.chainName}:`, e);
            throw e;
        }
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
            const tolerance = ethers.parseEther('0.005');
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

    /// Returns the total base token chain balance across L1AT and GWAT.
    /// The sum is always valid regardless of migration phase, since at any point
    /// the chain's balance is split between L1AT and GWAT.
    async getTotalBaseTokenChainBalance(): Promise<bigint> {
        const l1atBalance = await this.l1AssetTracker.chainBalance(this.inner.chainId, this.baseTokenAssetId);
        const gwatBalance = this.gwAssetTracker
            ? await this.gwAssetTracker.chainBalance(this.inner.chainId, this.baseTokenAssetId)
            : 0n;
        return l1atBalance + gwatBalance;
    }

    /// Executes an action and tracks any base token chain balance change caused by it.
    /// This captures gas spent on L1/GW priority operations that increase chain balance.
    async trackBaseTokenDelta(action: () => Promise<void>): Promise<void> {
        const before = await this.getTotalBaseTokenChainBalance();
        await action();
        const after = await this.getTotalBaseTokenChainBalance();
        const delta = after - before;
        if (delta !== 0n) {
            this.chainBalances[this.baseTokenAssetId] = (this.chainBalances[this.baseTokenAssetId] ?? 0n) + delta;
        }
    }

    async accountForSentInterop(token: ERC20Handler, amount: bigint = INTEROP_TEST_AMOUNT): Promise<void> {
        const assetId = await token.assetId(this);
        // For L2-native tokens we use MaxUint256 as the starting expected balance when not tracked yet.
        const currentBalance = this.chainBalances[assetId] ?? (token.isL2Token ? ethers.MaxUint256 : 0n);
        this.chainBalances[assetId] = currentBalance - amount;
        this.interopGatewayIncreases[assetId] = (this.interopGatewayIncreases[assetId] ?? 0n) + amount;
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
    public isBaseToken: boolean;
    cachedAssetId: string | null = null;

    constructor(
        wallet: zksync.Wallet,
        l1Contract: ethers.Contract | undefined,
        l2Contract: zksync.Contract | undefined,
        isBaseToken = false
    ) {
        this.wallet = wallet;
        this.l1Contract = l1Contract;
        this.l2Contract = l2Contract;
        this.isL2Token = !!l2Contract;
        this.isBaseToken = isBaseToken;
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

    async deposit(chainHandler: ChainHandler) {
        // For non-base-token deposits, measure the base token chain balance delta.
        // ERC20 deposits also send base token for L2 gas, which the asset tracker tracks
        // but would otherwise be unaccounted for in our local chainBalances.
        const trackBaseToken = !this.isBaseToken && !!chainHandler.baseTokenAssetId;
        const baseTokenBefore = trackBaseToken ? await chainHandler.getTotalBaseTokenChainBalance() : 0n;

        const depositTx = await this.wallet.deposit({
            token: await this.l1Contract!.getAddress(),
            amount: TOKEN_MINT_AMOUNT,
            approveERC20: true,
            approveBaseERC20: true
        });
        await depositTx.wait();

        await this.setL2Contract(chainHandler);
        await waitForBalanceNonZero(this.l2Contract!, this.wallet);

        const assetId = await this.assetId(chainHandler);
        chainHandler.chainBalances[assetId] = (chainHandler.chainBalances[assetId] ?? 0n) + TOKEN_MINT_AMOUNT;

        if (trackBaseToken) {
            const baseTokenAfter = await chainHandler.getTotalBaseTokenChainBalance();
            const baseTokenDelta = baseTokenAfter - baseTokenBefore;
            if (baseTokenDelta > 0n) {
                chainHandler.chainBalances[chainHandler.baseTokenAssetId] =
                    (chainHandler.chainBalances[chainHandler.baseTokenAssetId] ?? 0n) + baseTokenDelta;
            }
        }
    }

    async withdraw(amount?: bigint): Promise<WithdrawalHandler> {
        const withdrawAmount = amount ?? getRandomWithdrawAmount();

        let isETHBaseToken = false;
        let token;
        if (this.isBaseToken) {
            const baseToken = await this.wallet.provider.getBaseTokenContractAddress();
            isETHBaseToken = zksync.utils.isAddressEq(baseToken, zksync.utils.ETH_ADDRESS_IN_CONTRACTS);
            if (isETHBaseToken) {
                token = zksync.utils.ETH_ADDRESS;
            } else {
                const l2BaseTokenAddress = zksync.utils.L2_BASE_TOKEN_ADDRESS;
                token = l2BaseTokenAddress;
                if (!this.l2Contract || (await this.l2Contract.getAddress()) !== l2BaseTokenAddress) {
                    this.l2Contract = new zksync.Contract(l2BaseTokenAddress, ERC20_ABI, this.wallet);
                }
            }
        } else {
            token = await this.l2Contract!.getAddress();
        }

        if (
            !this.isBaseToken &&
            (await this.l2Contract!.allowance(this.wallet.address, L2_NATIVE_TOKEN_VAULT_ADDRESS)) < withdrawAmount
        ) {
            await (await this.l2Contract!.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, 0)).wait();
            await (await this.l2Contract!.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, withdrawAmount)).wait();
        }

        const withdrawTx = await this.wallet.withdraw({
            token,
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
        // Transfer the L2-B token balance on L1 to the target wallet.
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
        await (await l1Contract.mint(l1Wallet.address, TOKEN_MINT_AMOUNT)).wait();

        return new ERC20Handler(wallet, l1Contract, undefined);
    }

    static async deployTokenOnL2(chainHandler: ChainHandler) {
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
        await (await l2Contract.mint(chainHandler.l2RichWallet.address, TOKEN_MINT_AMOUNT)).wait();

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

function getRandomWithdrawAmount(): bigint {
    return BigInt(Math.floor(Math.random() * Number(MAX_WITHDRAW_AMOUNT)));
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

export function interopCallValueAttr(amount: bigint): string {
    return ERC7786_ATTR_INTERFACE.encodeFunctionData('interopCallValue', [amount]);
}

export function indirectCallAttr(callValue: bigint = 0n): string {
    return ERC7786_ATTR_INTERFACE.encodeFunctionData('indirectCall', [callValue]);
}

export function useFixedFeeAttr(useFixedFee: boolean): string {
    return ERC7786_ATTR_INTERFACE.encodeFunctionData('useFixedFee', [useFixedFee]);
}

export function executionAddressAttr(executionAddress: string, chainId: bigint): string {
    return ERC7786_ATTR_INTERFACE.encodeFunctionData('executionAddress', [
        formatEvmV1Address(executionAddress, chainId)
    ]);
}

export function unbundlerAddressAttr(unbundlerAddress: string, chainId: bigint): string {
    return ERC7786_ATTR_INTERFACE.encodeFunctionData('unbundlerAddress', [
        formatEvmV1Address(unbundlerAddress, chainId)
    ]);
}

export function getTokenTransferSecondBridgeData(assetId: string, amount: bigint, recipient: string) {
    return ethers.concat([
        '0x01',
        new ethers.AbiCoder().encode(
            ['bytes32', 'bytes'],
            [
                assetId,
                new ethers.AbiCoder().encode(['uint256', 'address', 'address'], [amount, recipient, ethers.ZeroAddress])
            ]
        )
    ]);
}

export async function sendInteropBundle(
    senderWallet: zksync.Wallet,
    destinationChainId: number,
    tokenAddress?: string
): Promise<ethers.TransactionReceipt> {
    const interopCenter = new zksync.Contract(L2_INTEROP_CENTER_ADDRESS, INTEROP_CENTER_ABI, senderWallet);
    const protocolFee = (await interopCenter.interopProtocolFee()) as bigint;

    let callStarters: InteropCallStarter[];
    let callValue = 0n;

    if (!tokenAddress) {
        callStarters = [
            {
                to: formatEvmV1Address(senderWallet.address),
                data: '0x',
                callAttributes: [interopCallValueAttr(INTEROP_TEST_AMOUNT)]
            }
        ];
        callValue = INTEROP_TEST_AMOUNT;
    } else {
        const l2Ntv = getL2Ntv(senderWallet);
        const assetId = await l2Ntv.assetId(tokenAddress);
        const tokenContract = new zksync.Contract(tokenAddress, ERC20_ABI, senderWallet);
        const allowance = await tokenContract.allowance(senderWallet.address, L2_NATIVE_TOKEN_VAULT_ADDRESS);
        if (allowance < INTEROP_TEST_AMOUNT) {
            await (await tokenContract.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, INTEROP_TEST_AMOUNT)).wait();
        }

        callStarters = [
            {
                to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                data: getTokenTransferSecondBridgeData(assetId, INTEROP_TEST_AMOUNT, senderWallet.address),
                callAttributes: [indirectCallAttr()]
            }
        ];
    }

    // InteropCenter now requires explicit fee mode via bundle attributes.
    // We use protocol fee in msg.value (useFixedFee=false), plus direct-call value when present.
    const bundleAttributes = [useFixedFeeAttr(false)];
    const msgValue = protocolFee * BigInt(callStarters.length) + callValue;
    const overrides: ethers.Overrides = { value: msgValue };

    const tx = await interopCenter.sendBundle(
        formatEvmV1Chain(BigInt(destinationChainId)),
        callStarters,
        bundleAttributes,
        overrides
    );
    return await tx.wait();
}

export async function awaitInteropBundle(
    senderWallet: zksync.Wallet,
    gwWallet: zksync.Wallet,
    destinationWallet: zksync.Wallet,
    txHash: string
) {
    const txReceipt = await senderWallet.provider.getTransactionReceipt(txHash);
    await waitUntilBlockFinalized(senderWallet, txReceipt!.blockNumber);

    await waitUntilBlockExecutedOnGateway(senderWallet, gwWallet, txReceipt!.blockNumber);
    await utils.sleep(1); // Additional delay to avoid flakiness
    const params = await senderWallet.getFinalizeWithdrawalParams(txHash, 0, 'proof_based_gw');
    await waitForInteropRootNonZero(destinationWallet.provider, destinationWallet, getGWBlockNumber(params));
}

export async function readAndBroadcastInteropBundle(
    wallet: zksync.Wallet,
    senderProvider: zksync.Provider,
    txHash: string
) {
    // Get interop trigger and bundle data from the sender chain.
    const executionBundle = await getInteropBundleData(senderProvider, txHash, 0);
    if (executionBundle.output == null) return;

    const interopHandler = new zksync.Contract(L2_INTEROP_HANDLER_ADDRESS, INTEROP_HANDLER_ABI, wallet);
    const receipt = await interopHandler.executeBundle(executionBundle.rawData, executionBundle.proofDecoded);
    await receipt.wait();
}

/**
 * Waits until the requested block is executed on the gateway.
 *
 * @param wallet Wallet to use to poll the server.
 * @param gwWallet Gateway wallet.
 * @param blockNumber Number of block.
 * @param timeoutMs Maximum time to wait (default 10 min).
 */
export async function waitUntilBlockExecutedOnGateway(
    wallet: zksync.Wallet,
    gwWallet: zksync.Wallet,
    blockNumber: number,
    timeoutMs: number = 10 * 60 * 1000
) {
    const start = Date.now();
    const bridgehub = new ethers.Contract(L2_BRIDGEHUB_ADDRESS, ArtifactIBridgehubBase.abi, gwWallet);
    const zkChainAddr = await bridgehub.getZKChain(await wallet.provider.getNetwork().then((net: any) => net.chainId));
    const gettersFacet = new ethers.Contract(zkChainAddr, ArtifactIGetters.abi, gwWallet);

    let batchNumber = (await wallet.provider.getBlockDetails(blockNumber)).l1BatchNumber;
    let currentExecutedBatchNumber = 0;
    while (currentExecutedBatchNumber < batchNumber) {
        if (Date.now() - start > timeoutMs) {
            throw new Error(
                `waitUntilBlockExecutedOnGateway: timed out after ${(timeoutMs / 1000).toFixed(0)}s waiting for block ${blockNumber} (batch ${batchNumber}, current executed: ${currentExecutedBatchNumber})`
            );
        }
        currentExecutedBatchNumber = await gettersFacet.getTotalBatchesExecuted();
        if (currentExecutedBatchNumber >= batchNumber) {
            break;
        } else {
            await zksync.utils.sleep(wallet.provider.pollingInterval);
        }
    }
}

export async function waitForInteropRootNonZero(
    provider: zksync.Provider,
    wallet: zksync.Wallet,
    l1BatchNumber: number
) {
    const interopRootStorageAbi = ArtifactL2InteropRootStorage.abi;
    const l2InteropRootStorage = new zksync.Contract(L2_INTEROP_ROOT_STORAGE_ADDRESS, interopRootStorageAbi, provider);
    const baseTokenAddress = await provider.getBaseTokenContractAddress();

    let currentRoot = ethers.ZeroHash;
    let count = 0;
    while (currentRoot === ethers.ZeroHash && count < 60) {
        // We make repeated transactions to force the L2 to update the interop root.
        const tx = await wallet.transfer({
            to: wallet.address,
            amount: 1,
            token: baseTokenAddress
        });
        await tx.wait();

        currentRoot = await l2InteropRootStorage.interopRoots(GATEWAY_CHAIN_ID, l1BatchNumber);
        await zksync.utils.sleep(wallet.provider.pollingInterval);

        count++;
    }
}

export function getGWBlockNumber(params: zksync.types.FinalizeWithdrawalParams): number {
    /// see hashProof in MessageHashing.sol for this logic.
    let gwProofIndex = 1 + parseInt(params.proof[0].slice(4, 6), 16) + 1 + parseInt(params.proof[0].slice(6, 8), 16);
    return parseInt(params.proof[gwProofIndex].slice(2, 34), 16);
}
