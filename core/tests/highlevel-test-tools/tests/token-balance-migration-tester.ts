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
import { migrateToGatewayIfNeeded, startServer } from '../src';
import { initTestWallet } from '../src/run-integration-tests';

const RICH_WALLET_L1_BALANCE = ethers.parseEther('10.0');
const RICH_WALLET_L2_BALANCE = RICH_WALLET_L1_BALANCE;
const TEST_SUITE_NAME = 'Token Balance Migration Test';
const pathToHome = path.join(__dirname, '../../../..');

function readArtifact(contractName: string, outFolder: string = 'out') {
    return JSON.parse(
        fs
            .readFileSync(
                path.join(pathToHome, `./contracts/l1-contracts/${outFolder}/${contractName}.sol/${contractName}.json`)
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
        const mintTx = await l1Token.mint(richWallet.address, RICH_WALLET_L2_BALANCE);
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

    constructor(inner: TestChain, l2RichWallet: zksync.Wallet) {
        this.inner = inner;
        this.l2RichWallet = l2RichWallet;

        const contractsConfig = loadConfig({ pathToHome, chain: inner.chainName, config: 'contracts.yaml' });
        this.l1Ntv = new ethers.Contract(
            contractsConfig.ecosystem_contracts.native_token_vault_addr,
            readArtifact('L1NativeTokenVault').abi,
            l2RichWallet.ethWallet()
        );
    }

    async stopServer() {
        await this.inner.mainNode.kill();
    }

    async migrateToGateway() {
        const pauseDepositsCmd = `zkstack chain pause-deposits --chain ${this.inner.chainName}`;
        await utils.spawn(pauseDepositsCmd);
        await this.stopServer();
        // By now, the priority queue should be empty, so we can migrate straight away
        await migrateToGatewayIfNeeded(this.inner.chainName);
        // Restart the server
        await startServer(this.inner.chainName);
    }

    async migrateFromGateway() {
        const pauseDepositsCmd = `zkstack chain pause-deposits --chain ${this.inner.chainName}`;
        await utils.spawn(pauseDepositsCmd);
        // Migrate from gateway
        await utils.spawn(
            `zkstack chain gateway migrate-from-gateway --gateway-chain-name gateway --chain ${this.inner.chainName}`
        );
    }

    async migrateTokenBalancesToGateway() {
        const migrationCmd = `zkstack chain gateway migrate-token-balances --to-gateway true --gateway-chain-name gateway --chain ${this.inner.chainName}`;

        // Migration might sometimes fail, so we retry a few times.
        for (let attempt = 1; attempt <= 3; attempt++) {
            try {
                await utils.spawn(migrationCmd);
                break;
            } catch (e) {
                if (attempt === 3) throw e;
                await utils.sleep(2 * attempt);
            }
        }
    }

    async migrateTokenBalancesToL1() {
        const migrationCmd = `zkstack chain gateway migrate-token-balances --to-gateway false --gateway-chain-name gateway --chain ${this.inner.chainName}`;

        // Migration might sometimes fail, so we retry a few times.
        for (let attempt = 1; attempt <= 3; attempt++) {
            try {
                await utils.spawn(migrationCmd);
                break;
            } catch (e) {
                if (attempt === 3) throw e;
                await utils.sleep(2 * attempt);
            }
        }
    }
    //18:13:19

    static async createNewChain(chainType: ChainType): Promise<ChainHandler> {
        const testChain = await createChainAndStartServer(chainType, TEST_SUITE_NAME, false);
        await initTestWallet(testChain.chainName);

        // Need to wait for a bit before the server works fully
        await sleep(2000);

        return new ChainHandler(testChain, await generateChainRichWallet(testChain.chainName));
    }

    async deployNativeToken() {
        return await ERC20Handler.deployTokenOnL2(this.l2RichWallet);
    }
}

export class ERC20Handler {
    public wallet: zksync.Wallet;
    public l1Contract: ethers.Contract | undefined;
    public l2Contract: zksync.Contract | undefined;
    _cachedAssetId: string | null = null;

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
        if (this._cachedAssetId) return this._cachedAssetId;

        let assetId: string;
        if (this.l1Contract) {
            if (!chainHandler) throw new Error('Chain handler must be provided');
            assetId = await chainHandler.l1Ntv.assetId(await this.l1Contract.getAddress());
        } else {
            const l2Ntv = getL2Ntv(this.wallet);
            assetId = await l2Ntv.assetId(await this.l2Contract!.getAddress());
        }
        this._cachedAssetId = assetId;
        return assetId;
    }

    async deposit(chainHandler: ChainHandler, amount: bigint = DEFAULT_SMALL_AMOUNT): Promise<bigint> {
        const depositAmount = amount ?? ethers.parseUnits((Math.floor(Math.random() * 900) + 100).toString(), 'gwei');
        const depositTx = await this.wallet.deposit({
            token: await this.l1Contract!.getAddress(),
            amount: depositAmount,
            approveERC20: true,
            approveBaseERC20: true
        });
        await depositTx.wait();

        await this.setL2Contract(chainHandler);

        return depositAmount;
    }

    async withdraw(amount: bigint = DEFAULT_SMALL_AMOUNT): Promise<WithdrawalHandler> {
        const withdrawAmount = amount ?? ethers.parseUnits((Math.floor(Math.random() * 900) + 100).toString(), 'gwei');
        await this._registerIfNeeded();

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

        const props = this._generateRandomTokenProps();
        const newToken = await factory.deploy(props.name, props.symbol, props.decimals);
        await newToken.waitForDeployment();
        const l1Contract = new ethers.Contract(await newToken.getAddress(), ERC20_ABI, l1Wallet);
        await (await l1Contract.mint(l1Wallet.address, RICH_WALLET_L1_BALANCE)).wait();

        return new ERC20Handler(wallet, l1Contract, undefined);
    }

    static async deployTokenOnL2(l2Wallet: zksync.Wallet) {
        const factory = new zksync.ContractFactory(ERC20_ABI, ERC20_ZKEVM_BYTECODE, l2Wallet, 'create');

        const props = this._generateRandomTokenProps();
        const newToken = await factory.deploy(props.name, props.symbol, props.decimals);
        await newToken.waitForDeployment();
        const l2Contract = new zksync.Contract(await newToken.getAddress(), ERC20_ABI, l2Wallet);
        await (await l2Contract.mint(l2Wallet.address, RICH_WALLET_L1_BALANCE)).wait();

        return new ERC20Handler(l2Wallet, undefined, l2Contract);
    }

    private async _registerIfNeeded() {
        const l2Ntv = getL2Ntv(this.wallet);
        const l2AssetId = await l2Ntv.assetId(await this.l2Contract!.getAddress());
        if (l2AssetId === ethers.ZeroHash) {
            // Registering the token
            await (await l2Ntv.registerToken(await this.l2Contract!.getAddress())).wait();
        }
    }

    private static _generateRandomTokenProps() {
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

        await waitForL2ToL1LogProof(l2Wallet.provider, receipt.blockNumber, this.txHash);

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

export class Tester {
    public runningFee: Map<zksync.types.Address, bigint>;
    constructor(
        public ethProvider: ethers.Provider,
        public ethWallet: ethers.Wallet,
        public syncWallet: zksync.Wallet,
        public web3Provider: zksync.Provider
    ) {
        this.runningFee = new Map();
    }

    // prettier-ignore
    static async init(ethProviderAddress: string, web3JsonRpc: string) {
        const ethProvider = new ethers.JsonRpcProvider(ethProviderAddress);

        const chainName = process.env.CHAIN_NAME!!;
        let ethWallet = new ethers.Wallet(getMainWalletPk(chainName));

        ethWallet = ethWallet.connect(ethProvider);
        const web3Provider = new zksync.Provider(web3JsonRpc);
        web3Provider.pollingInterval = 100; // It's OK to keep it low even on stage.
        const syncWallet = new zksync.Wallet(ethWallet.privateKey, web3Provider, ethProvider);


        // Since some tx may be pending on stage, we don't want to get stuck because of it.
        // In order to not get stuck transactions, we manually cancel all the pending txs.
        const latestNonce = await ethWallet.getNonce('latest');
        const pendingNonce = await ethWallet.getNonce('pending');
        const cancellationTxs = [];
        for (let nonce = latestNonce; nonce != pendingNonce; nonce++) {
            // For each transaction to override it, we need to provide greater fee. 
            // We would manually provide a value high enough (for a testnet) to be both valid
            // and higher than the previous one. It's OK as we'll only be charged for the bass fee
            // anyways. We will also set the miner's tip to 5 gwei, which is also much higher than the normal one.
            const maxFeePerGas = ethers.parseEther("0.00000025"); // 250 gwei
            const maxPriorityFeePerGas = ethers.parseEther("0.000000005"); // 5 gwei
            cancellationTxs.push(ethWallet.sendTransaction({ to: ethWallet.address, nonce, maxFeePerGas, maxPriorityFeePerGas }).then((tx) => tx.wait()));
        }
        if (cancellationTxs.length > 0) {
            await Promise.all(cancellationTxs);
            console.log(`Canceled ${cancellationTxs.length} pending transactions`);
        }

        return new Tester(ethProvider, ethWallet, syncWallet, web3Provider);
    }

    emptyWallet() {
        const walletHD = zksync.Wallet.createRandom();
        return new zksync.Wallet(walletHD.privateKey, this.web3Provider, this.ethProvider);
    }
}

export async function waitUntilBlockFinalized(provider: zksync.Provider, blockNumber: number) {
    console.log('Waiting for block to be finalized...', blockNumber);
    let printedBlockNumber = 0;
    while (true) {
        const block = await provider.getBlock('finalized');
        if (blockNumber <= block.number) {
            break;
        } else {
            if (printedBlockNumber < block.number) {
                console.log('Waiting for block to be finalized...', blockNumber, block.number);
                console.log('time', new Date().toISOString());
                printedBlockNumber = block.number;
            }
            await zksync.utils.sleep(provider.pollingInterval);
        }
    }
}

export async function waitForL2ToL1LogProof(provider: zksync.Provider, blockNumber: number, txHash: string) {
    console.log('waiting for block to be finalized');
    // First, we wait for block to be finalized.
    await waitUntilBlockFinalized(provider, blockNumber);

    console.log('waiting for log proof');
    // Second, we wait for the log proof.
    let i = 0;
    while ((await provider.getLogProof(txHash)) == null) {
        console.log(`Waiting for log proof... ${i}`);
        await zksync.utils.sleep(provider.pollingInterval);
        i++;
    }
}
