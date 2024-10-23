/**
 * This suite contains tests checking interop behavior.
 */

import * as fs from 'fs';

import { TestMaster } from '../src/index';

import * as zksync from 'zksync-ethers-interop-support';
import * as ethers from 'ethers';
import { Wallet } from 'ethers';
import { scaledGasPrice, deployContract, waitForBlockToBeFinalizedOnL1 } from '../src/helpers';

import {
    L2_ASSET_ROUTER_ADDRESS,
    L2_BRIDGEHUB_ADDRESS,
    L2_NATIVE_TOKEN_VAULT_ADDRESS,
    REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
    L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR,
    BRIDGEHUB_L2_CANONICAL_TRANSACTION_ABI,
    ETH_ADDRESS_IN_CONTRACTS,
    L2_LOG_STRING,
    ARTIFACTS_PATH
} from '../src/constants';
import { RetryProvider } from '../src/retry-provider';

// Read contract ABIs
function readContract(path: string, fileName: string, contractName?: string) {
    contractName = contractName || fileName;
    return JSON.parse(fs.readFileSync(`${path}/${fileName}.sol/${contractName}.json`, { encoding: 'utf-8' }));
}

const ArtifactBridgeHub = readContract(`${ARTIFACTS_PATH}bridgehub`, 'Bridgehub');
const ArtifactNativeTokenVault = readContract(`${ARTIFACTS_PATH}bridge/ntv`, 'L2NativeTokenVault');
const ArtifactMintableERC20 = readContract(
    '../../../contracts/l1-contracts/artifacts-zk/contracts/dev-contracts',
    'TestnetERC20Token'
);
const l1AssetRouterInterface = readContract(`${ARTIFACTS_PATH}/bridge/asset-router`, 'L1AssetRouter').abi;
const ArtifactSwap = readContract('./artifacts-zk/contracts/Swap', 'Swap');

const richPk = '0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110';

// Constants
const INTEROP_TX_TYPE = 253;

describe('Interop checks', () => {
    let testMaster: TestMaster;

    // L1 Variables
    let l1_provider: ethers.Provider;
    let l1_wallet: zksync.Wallet;
    let l1EthersWallet: Wallet;
    let veryRichWallet: zksync.Wallet;

    let tokenA_details: {
        assetId: string;
        addresses: {
            interop1: string;
            interop2: string;
            l1: string;
        };
    };
    let tokenB_details: {
        assetId: string;
        addresses: {
            interop1: string;
            interop2: string;
            l1: string;
        };
    };

    // Common Variables
    const timeout = 10000;

    // Interop1 (Main Chain) Variables
    let interop1_provider: zksync.Provider;
    let interop1_wallet: zksync.Wallet;
    let interop1_rich_wallet: zksync.Wallet;
    let interop1_bridgehub_contract: zksync.Contract;
    let interop1_nativeTokenVault_contract: zksync.Contract;
    let interop1_tokenA_contract: zksync.Contract;

    // Interop2 (Second Chain) Variables
    let interop2_provider: zksync.Provider;
    let interop2_wallet: zksync.Wallet;
    let interop2_rich_wallet: zksync.Wallet;
    let interop2_nativeTokenVault_contract: zksync.Contract;
    let interop2_swap_contract: zksync.Contract;
    let interop2_tokenB_contract: zksync.Contract;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        const test_wallet_pk = testMaster.newEmptyAccount().privateKey;
        const mainAccount = testMaster.mainAccount();

        // Initialize the providers
        l1_provider = mainAccount.providerL1!;
        interop1_provider = mainAccount.provider;
        // Setup Interop1 Provider and Wallet
        l1_wallet = new zksync.Wallet(mainAccount.privateKey, interop1_provider, l1_provider);
        veryRichWallet = new zksync.Wallet(richPk, interop1_provider, l1_provider);
        l1EthersWallet = new Wallet(mainAccount.privateKey, l1_provider);

        // Initialize Test Master and L1 Wallet
        interop1_wallet = new zksync.Wallet(test_wallet_pk, interop1_provider, l1_provider);
        interop1_rich_wallet = new zksync.Wallet(mainAccount.privateKey, interop1_provider, l1_provider);
        console.log('PK', test_wallet_pk);

        // Setup Interop2 Provider and Wallet
        interop2_provider = new RetryProvider(
            { url: 'http://localhost:3050', timeout: 1200 * 1000 },
            undefined,
            testMaster.reporter
        );
        interop2_wallet = new zksync.Wallet(test_wallet_pk, interop2_provider, l1_provider);
        interop2_rich_wallet = new zksync.Wallet(mainAccount.privateKey, interop2_provider, l1_provider);

        // Initialize Contracts on Interop1
        interop1_bridgehub_contract = new zksync.Contract(L2_BRIDGEHUB_ADDRESS, ArtifactBridgeHub.abi, interop1_wallet);
        interop1_nativeTokenVault_contract = new zksync.Contract(
            L2_NATIVE_TOKEN_VAULT_ADDRESS,
            ArtifactNativeTokenVault.abi,
            interop1_wallet
        );

        // Initialize Contracts on Interop2
        interop2_nativeTokenVault_contract = new zksync.Contract(
            L2_NATIVE_TOKEN_VAULT_ADDRESS,
            ArtifactNativeTokenVault.abi,
            interop2_wallet
        );

        console.log(`Test wallet 1 address: ${interop1_wallet.address}`);
        console.log(`Test wallet 2 address: ${interop1_wallet.address}`)
        console.log(
            `[rich wallet] l1_wallet address: ${l1_wallet.address} with ${ethers.formatEther(
                await l1_provider.getBalance(l1_wallet.address)
            )} ETH`
        );
        console.log('--------------------');

        // Token Details
        tokenA_details = {
            assetId: '',
            addresses: {
                interop1: '',
                interop2: '',
                l1: ''
            }
        };
        tokenB_details = {
            assetId: '',
            addresses: {
                interop1: '',
                interop2: '',
                l1: ''
            }
        };

        await (
            await veryRichWallet._signerL1!().sendTransaction({
                to: interop1_rich_wallet.address,
                value: ethers.parseEther('100') //amount*10n
            })
        ).wait();

        await (
            await veryRichWallet._signerL1!().sendTransaction({
                to: interop2_rich_wallet.address,
                value: ethers.parseEther('100') //amount*10n
            })
        ).wait();

        await (
            await veryRichWallet._signerL1!().sendTransaction({
                to: interop1_wallet.address,
                value: ethers.parseEther('100') //amount*10n
            })
        ).wait();

        await (
            await veryRichWallet._signerL1!().sendTransaction({
                to: interop2_wallet.address,
                value: ethers.parseEther('100') //amount*10n
            })
        ).wait();
    });

    test('Can perform an ETH deposit', async () => {
        // Fund accounts
        const gasPrice = await scaledGasPrice(interop1_rich_wallet);
        const fundAmount = ethers.parseEther('10');
        console.log('Funding test wallet at interop1');
        await (
            await interop1_rich_wallet.deposit({
                token: ETH_ADDRESS_IN_CONTRACTS,
                amount: fundAmount,
                to: interop1_wallet.address,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: { gasPrice },
                overrides: { gasPrice }
            })
        ).wait();
        console.log('Funding test wallet at interop2');
        await (
            await interop2_rich_wallet.deposit({
                token: ETH_ADDRESS_IN_CONTRACTS,
                amount: fundAmount,
                to: interop2_wallet.address,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: { gasPrice },
                overrides: { gasPrice }
            })
        ).wait();
        console.log('Test wallet funded');
    });

    test('Can deploy token contracts', async () => {
        // Deploy token A on interop1 and register
        console.log('Deploying token A on Interop1');
        const interop1_tokenA_contract_deployment = await deployContract(interop1_wallet, ArtifactMintableERC20, [
            'Token A',
            'AA',
            18
        ]);
        tokenA_details.addresses.interop1 = await interop1_tokenA_contract_deployment.getAddress();
        console.log('Registering token A on Interop1');
        await (await interop1_nativeTokenVault_contract.registerToken(tokenA_details.addresses.interop1)).wait();
        await (await interop1_tokenA_contract_deployment.mint(await interop1_wallet.getAddress(), 1000)).wait();
        await (await interop1_tokenA_contract_deployment.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, 1000)).wait();
        console.log('Token A registered on Interop1');
        tokenA_details.assetId = await interop1_nativeTokenVault_contract.assetId(tokenA_details.addresses.interop1);
        tokenA_details.addresses.interop2 = await interop2_nativeTokenVault_contract.tokenAddress(
            tokenA_details.assetId
        );
        interop1_tokenA_contract = new zksync.Contract(
            tokenA_details.addresses.interop1,
            ArtifactMintableERC20.abi,
            interop1_wallet
        );
        console.log('Token A info:', tokenA_details);

        // Deploy token B on interop2 and register
        console.log('Deploying token B on Interop2');
        const interop2_tokenB_contract_deployment = await deployContract(interop2_wallet, ArtifactMintableERC20, [
            'Token B',
            'BB',
            18
        ]);
        tokenB_details.addresses.interop2 = await interop2_tokenB_contract_deployment.getAddress();
        await (await interop2_tokenB_contract_deployment.mint(await interop1_wallet.getAddress(), ethers.parseEther('100'))).wait();
        await (await interop2_tokenB_contract_deployment.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS,  ethers.parseEther('100'))).wait();
        console.log('Registering token B on Interop2');
        await (await interop2_nativeTokenVault_contract.registerToken(tokenB_details.addresses.interop2)).wait();
        console.log('Token B registered on Interop2');
        // await delay(timeout);
        tokenB_details.assetId = await interop2_nativeTokenVault_contract.assetId(tokenB_details.addresses.interop2);
        tokenB_details.addresses.interop1 = await interop1_nativeTokenVault_contract.tokenAddress(
            tokenB_details.assetId
        );
        interop2_tokenB_contract = new zksync.Contract(
            tokenB_details.addresses.interop2,
            ArtifactMintableERC20.abi,
            interop2_wallet
        );
        console.log('Token B info:', tokenB_details);
    });

    test('Withdraw and deposit tokens via L1', async () => {
        const bridgeContracts = await interop1_wallet.getL1BridgeContracts();
        const assetRouter = bridgeContracts.shared;
        // console.log("assetRouter", assetRouter)
        const l1AssetRouter = new ethers.Contract(
            await assetRouter.getAddress(),
            l1AssetRouterInterface,
            l1EthersWallet
        );
        // console.log("ntv", await l1AssetRouter.)
        const l1NativeTokenVault = new ethers.Contract(
            await l1AssetRouter.nativeTokenVault(),
            ArtifactNativeTokenVault.abi,
            l1EthersWallet
        );

        const withdrawA = interop1_wallet.withdraw({
            token: tokenA_details.addresses.interop1,
            amount: 100
        });
        await expect(withdrawA).toBeAccepted();
        const withdrawalATx = await withdrawA;
        const l2TxAReceipt = await interop1_wallet.provider.getTransactionReceipt(withdrawalATx.hash);
        await withdrawalATx.waitFinalize();
        await waitForBlockToBeFinalizedOnL1(interop1_wallet, l2TxAReceipt!.blockNumber);

        await interop1_wallet.finalizeWithdrawalParams(withdrawalATx.hash); // kl todo finalize the Withdrawals with the params here. Alternatively do in the SDK.
        await expect(interop1_rich_wallet.finalizeWithdrawal(withdrawalATx.hash)).toBeAccepted();

        const withdrawB = interop2_wallet.withdraw({
            token: tokenB_details.addresses.interop2,
            amount: 100
        });

        await expect(withdrawB).toBeAccepted();
        const withdrawBTx = await withdrawB;
        const l2TxBReceipt = await interop2_wallet.provider.getTransactionReceipt(withdrawBTx.hash);
        await withdrawBTx.waitFinalize();
        await waitForBlockToBeFinalizedOnL1(interop2_wallet, l2TxBReceipt!.blockNumber);

        await interop2_wallet.finalizeWithdrawalParams(withdrawBTx.hash); // kl todo finalize the Withdrawals with the params here. Alternatively do in the SDK.
        await expect(interop2_rich_wallet.finalizeWithdrawal(withdrawBTx.hash)).toBeAccepted();

        tokenA_details.addresses.l1 = await l1NativeTokenVault.tokenAddress(tokenA_details.assetId);
        tokenB_details.addresses.l1 = await l1NativeTokenVault.tokenAddress(tokenB_details.assetId);

        await expect(
            interop1_wallet.deposit({
                token: tokenB_details.addresses.l1,
                amount: 50,
                approveERC20: true
            })
        ).toBeAccepted();

        await expect(
            interop2_wallet.deposit({
                token: tokenA_details.addresses.l1,
                amount: 50,
                approveERC20: true
            })
        ).toBeAccepted();

        tokenA_details.addresses.interop2 = await interop2_nativeTokenVault_contract.tokenAddress(
            tokenA_details.assetId
        );
        tokenB_details.addresses.interop1 = await interop1_nativeTokenVault_contract.tokenAddress(
            tokenB_details.assetId
        );

        console.log(tokenA_details);
        console.log(tokenB_details);
    });

    test('Deploy swap contract', async () => {
        // Deploy Swap Contracts on Interop2
        console.log('Deploying Swap Contract on Interop2');
        const interop2_swap_contract_deployment = await deployContract(interop2_wallet, ArtifactSwap, [
            tokenA_details.addresses.interop2,
            tokenB_details.addresses.interop2
        ]);
        const interop2_swap_contract_address = await interop2_swap_contract_deployment.getAddress();
        interop2_swap_contract = new zksync.Contract(interop2_swap_contract_address, ArtifactSwap.abi, interop2_wallet);
        console.log(`Swap Contract deployed to: ${interop2_swap_contract_address}`);

        // Mint token B on Interop2 for swap contract
        console.log('Minting token B on Interop2 for Swap Contract...');
        await (
            await interop2_tokenB_contract.mint(await interop2_swap_contract.getAddress(), ethers.parseEther('1000'))
        ).wait();
        console.log(
            `Swap contract token B balance: ${ethers.formatEther(
                await getTokenBalance({
                    provider: interop2_provider,
                    tokenAddress: tokenB_details.addresses.interop2,
                    address: interop2_swap_contract_address
                })
            )} BB`
        );

        // Mint token A on Interop1 for test wallet
        console.log('Minting token A on Interop1 for test wallet...');
        await (await interop1_tokenA_contract.mint(interop1_wallet.address, ethers.parseEther('500'))).wait();
        console.log('[SETUP COMPLETED]');
    });

    test('Can transfer token A from Interop1 to Interop2', async () => {
        console.log('\n\n[TEST STARTED] - Can transfer token A from Interop1 to Interop2.');
        const interop1_tokenA_balance_before = await getTokenBalance({
            provider: interop1_provider,
            tokenAddress: tokenA_details.addresses.interop1,
            address: interop1_wallet.address
        });
        console.log(
            `Test wallet token A Interop 1 balance before transfer: ${ethers.formatEther(
                interop1_tokenA_balance_before
            )} AA`
        );

        // Send Transfer Transaction
        await from_interop1_transfer_tokenA();

        const interop1_tokenA_balance_after = await getTokenBalance({
            provider: interop1_provider,
            tokenAddress: tokenA_details.addresses.interop1,
            address: interop1_wallet.address
        });
        console.log(
            `Test wallet token A Interop 1 balance after transfer: ${ethers.formatEther(
                interop1_tokenA_balance_after
            )} AA`
        );

        // Update token A address on Interop2
        // tokenA_details.addresses.interop2 = await interop2_nativeTokenVault_contract.tokenAddress(tokenA_details.assetId);
        // if (tokenA_details.addresses.interop2 === ethers.ZeroHash) throw new Error("Token A resolves to zero address on Interop2");

        const interop2_tokenA_balance_after = await getTokenBalance({
            provider: interop2_provider,
            tokenAddress: tokenA_details.addresses.interop2,
            address: interop2_wallet.address
        });
        console.log(
            `Test wallet token A Interop 2 balance after transfer: ${ethers.formatEther(
                interop2_tokenA_balance_after
            )} AA`
        );
    });

    test('Can perform cross chain swap', async () => {
        console.log('\n\n[TEST STARTED] - Can perform cross chain swap.');

        console.log('Approving token A allowance for Swap Contract on Interop2 from Interop1...');
        const allowanceBefore = await getTokenAllowance({
            provider: interop2_provider,
            tokenAddress: tokenA_details.addresses.interop2,
            fromAddress: await interop1_wallet.getAddress(),
            toAddress: await interop2_swap_contract.getAddress()
        });
        console.log('Allowance before', allowanceBefore);
        await from_interop1_approveSwapAllowance();
        const allowanceAfter = await getTokenAllowance({
            provider: interop2_provider,
            tokenAddress: tokenA_details.addresses.interop2,
            fromAddress: await interop1_wallet.getAddress(),
            toAddress: await interop2_swap_contract.getAddress()
        });
        console.log('Allowance after', allowanceAfter);

        await delay(5 * timeout);

        const interop2_tokenB_balance_before = await getTokenBalance({
            provider: interop2_provider,
            tokenAddress: tokenB_details.addresses.interop2,
            address: interop2_wallet.address
        });
        console.log(
            `Test wallet token B Interop2 balance before swap: ${ethers.formatEther(interop2_tokenB_balance_before)} BB`
        );

        // Send Swap Transaction
        console.log('Swapping token A to token B...');
        await from_interop1_swap_a_to_b();

        await delay(timeout);

        const interop2_tokenB_balance_after = await getTokenBalance({
            provider: interop2_provider,
            tokenAddress: tokenB_details.addresses.interop2,
            address: interop2_wallet.address
        });
        console.log(
            `Test wallet token B Interop2 balance after swap: ${ethers.formatEther(interop2_tokenB_balance_after)} BB`
        );
    });

    test('Can transfer token B from Interop2 to Interop1', async () => {
        console.log('\n\n[TEST STARTED] - Can transfer token B from Interop2 to Interop1.');

        console.log('Transferring token B from Interop2 to Interop1...');
        const interop1_tokenB_balance_before = await getTokenBalance({
            provider: interop1_provider,
            tokenAddress: tokenB_details.addresses.interop1,
            address: interop1_wallet.address
        });
        console.log(
            `Test wallet token B Interop1 balance after transfer: ${ethers.formatEther(
                interop1_tokenB_balance_before
            )} BB`
        );
        await delay(1000)
        await from_interop2_transfer_tokenB();

        // Update token B address on Interop1
        // tokenB_details.addresses.interop1 = await interop1_nativeTokenVault_contract.tokenAddress(
        //     tokenB_details.assetId
        // );
        // if (tokenB_details.addresses.interop1 === ethers.ZeroHash)
        //     throw new Error('Token B resolves to zero address on Interop1');

        const interop1_tokenB_balance_after = await getTokenBalance({
            provider: interop1_provider,
            tokenAddress: tokenB_details.addresses.interop1,
            address: interop1_wallet.address
        });
        console.log(
            `Test wallet token B Interop1 balance after transfer: ${ethers.formatEther(
                interop1_tokenB_balance_after
            )} BB`
        );
    });

    /**
     * Sends a transfer transaction from Interop1 to Interop2.
     */
    async function from_interop1_transfer_tokenA() {
        const amount = ethers.parseEther('0.1');
        const mintValue = ethers.parseEther('0.2');
        const bridgeCalldata = ethers.concat([
            '0x01',
            new ethers.AbiCoder().encode(
                ['bytes32', 'bytes'],
                [
                    tokenA_details.assetId,
                    new ethers.AbiCoder().encode(['uint256', 'address'], [amount, interop1_wallet.address])
                ]
            )
        ]);

        await (await interop1_tokenA_contract.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, amount)).wait();
        const tx = await from_interop1_requestL2TransactionTwoBridges(mintValue, bridgeCalldata);
        await tx.wait();

        await delay(timeout);

        await readAndBroadcastInteropTx(tx.hash, interop1_provider, interop2_provider);
    }

    /**
     * Sends an approve transaction from Interop1 to Interop2.
     */
    async function from_interop1_approveSwapAllowance() {
        const amount = ethers.parseEther('0.1');
        const mintValue = ethers.parseEther('0.2');

        // Use the token contract on Interop2 (Second Chain)
        const l2Calldata = interop1_tokenA_contract.interface.encodeFunctionData('approve', [
            await interop2_swap_contract.getAddress(),
            amount
        ]);

        // Create an interop transaction from Interop1 to Interop2
        const tx = await from_interop1_requestL2TransactionDirect(
            mintValue,
            tokenA_details.addresses.interop2,
            BigInt(0),
            l2Calldata
        );
        await tx.wait();

        await delay(timeout);

        // Read and broadcast the interop transaction from Interop1 to Interop2
        await readAndBroadcastInteropTx(tx.hash, interop1_provider, interop2_provider);
    }

    /**
     * Sends a swap transaction from Interop1 to Interop2.
     */
    async function from_interop1_swap_a_to_b() {
        const amount = ethers.parseEther('0.1');
        const mintValue = ethers.parseEther('0.2');

        // Prepare calldata using the Swap contract on Interop2
        const l2Calldata = interop2_swap_contract.interface.encodeFunctionData('swap', [amount]);

        // Create interop transaction from Interop1 to Interop2
        const tx = await from_interop1_requestL2TransactionDirect(
            mintValue,
            await interop2_swap_contract.getAddress(),
            BigInt(0),
            l2Calldata
        );
        await readAndBroadcastInteropTx(tx.hash, interop1_provider, interop2_provider);
    }

    /**
     * Sends a transfer transaction from Interop2 to Interop1.
     */
    async function from_interop2_transfer_tokenB() {
        const amount = ethers.parseEther('0.2'); // swap ratio is 1:2 (A:B)
        const mintValue = ethers.parseEther('0.2');
        const bridgeCalldata = ethers.concat([
            '0x01',
            new ethers.AbiCoder().encode(
                ['bytes32', 'bytes'],
                [
                    tokenB_details.assetId,
                    new ethers.AbiCoder().encode(['uint256', 'address'], [amount, interop1_wallet.address])
                ]
            )
        ]);

        const input = {
            chainId: (await interop1_provider.getNetwork()).chainId,
            mintValue,
            l2Value: 0,
            l2GasLimit: 30000000,
            l2GasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
            refundRecipient: interop1_wallet.address,
            secondBridgeAddress: L2_ASSET_ROUTER_ADDRESS,
            secondBridgeValue: 0,
            secondBridgeCalldata: bridgeCalldata
        };
        const l2Calldata = interop1_bridgehub_contract.interface.encodeFunctionData('requestL2TransactionTwoBridges', [input]);

        // Create interop transaction from Interop1 to Interop2
        const tx = await from_interop1_requestL2TransactionDirect(
            mintValue * 2n,
            L2_BRIDGEHUB_ADDRESS,
            mintValue,
            l2Calldata
        );
        await readAndBroadcastInteropTx(tx.hash, interop1_provider, interop2_provider);
    }

    /**
     * Requests an L2 transaction involving two bridges on Interop1.
     * @param mintValue - The value to mint.
     * @param secondBridgeCalldata - The calldata for the second bridge.
     * @returns The transaction response.
     */
    async function from_interop1_requestL2TransactionTwoBridges(mintValue: bigint, secondBridgeCalldata: string) {
        console.log('requestL2TransactionTwoBridges from Interop1 to Interop2');
        const input = {
            chainId: (await interop2_provider.getNetwork()).chainId,
            mintValue,
            l2Value: 0,
            l2GasLimit: 30000000,
            l2GasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
            refundRecipient: interop1_wallet.address,
            secondBridgeAddress: L2_ASSET_ROUTER_ADDRESS,
            secondBridgeValue: 0,
            secondBridgeCalldata
        };

        const request = await interop1_bridgehub_contract.requestL2TransactionTwoBridges.populateTransaction(input);
        request.value = mintValue;
        request.from = interop1_wallet.address;

        const tx = await interop1_bridgehub_contract.requestL2TransactionTwoBridges(input, {
            value: request.value,
            gasLimit: 30000000
        });
        await tx.wait();
        return tx;
    }

    /**
     * Requests a direct L2 transaction on Interop1.
     * @param mintValue - The value to mint.
     * @param l2Contract - The target L2 contract address.
     * @param l2Value - The value to send with the transaction.
     * @param l2Calldata - The calldata for the transaction.
     * @returns The transaction response.
     */
    async function from_interop1_requestL2TransactionDirect(
        mintValue: bigint,
        l2Contract: string,
        l2Value: bigint,
        l2Calldata: string
    ) {
        const input = {
            chainId: (await interop2_provider.getNetwork()).chainId,
            mintValue,
            l2Contract,
            l2Value,
            l2Calldata,
            l2GasLimit: 600000000,
            l2GasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
            factoryDeps: [],
            refundRecipient: interop1_wallet.address
        };

        const request = await interop1_bridgehub_contract.requestL2TransactionDirect.populateTransaction(input);
        request.value = mintValue;
        request.from = interop1_wallet.address;

        const tx = await interop1_bridgehub_contract.requestL2TransactionDirect(input, {
            value: request.value,
            gasLimit: 30000000
        });

        await tx.wait();
        return tx;
    }

    /**
     * Reads and broadcasts an interop transaction between Interop1 and Interop2.
     * @param tx - The original transaction response.
     * @param sender_chain_provider - The sender wallet (Interop1).
     * @param receiver_chain_provider - The receiver wallet (Interop2).
     */
    async function readAndBroadcastInteropTx(
        txHash: string,
        sender_chain_provider: zksync.Provider,
        receiver_chain_provider: zksync.Provider
    ) {
        console.log('*Reading and broadcasting interop tx*');
        let { l1BatchNumber, l2TxNumberInBlock, message } = { l1BatchNumber: 0, l2TxNumberInBlock: 0, message: '' };

        try {
            // console.log("Reading interop message");
            // `getFinalizeWithdrawalParamsWithoutProof` is only available for wallet instance but not provider
            const sender_chain_utilityWallet = new zksync.Wallet(
                zksync.Wallet.createRandom().privateKey,
                sender_chain_provider
            );
            const {
                l1BatchNumber: l1BatchNumberRead,
                l2TxNumberInBlock: l2TxNumberInBlockRead,
                message: messageRead
            } = await sender_chain_utilityWallet.getFinalizeWithdrawalParamsWithoutProof(txHash, 0);
            // console.log("Finished reading interop message");

            l1BatchNumber = l1BatchNumberRead || 0;
            l2TxNumberInBlock = l2TxNumberInBlockRead || 0;
            message = messageRead || '';

            if (!message) return;
        } catch (e) {
            console.log('Error reading interop message:', e); // note no error here, since we run out of txs sometime
            return;
        }

        // Decode the interop message
        const decodedRequest = ethers.AbiCoder.defaultAbiCoder().decode(
            [BRIDGEHUB_L2_CANONICAL_TRANSACTION_ABI],
            '0x' + message.slice(2)
        );

        const xl2Input = {
            txType: decodedRequest[0][0],
            from: decodedRequest[0][1],
            to: decodedRequest[0][2],
            gasLimit: decodedRequest[0][3],
            gasPerPubdataByteLimit: decodedRequest[0][4],
            maxFeePerGas: decodedRequest[0][5],
            maxPriorityFeePerGas: decodedRequest[0][6],
            paymaster: decodedRequest[0][7],
            nonce: decodedRequest[0][8],
            value: decodedRequest[0][9],
            reserved: [
                decodedRequest[0][10][0],
                decodedRequest[0][10][1],
                decodedRequest[0][10][2],
                decodedRequest[0][10][3]
            ],
            data: decodedRequest[0][11],
            signature: decodedRequest[0][12],
            factoryDeps: decodedRequest[0][13],
            paymasterInput: decodedRequest[0][14],
            reservedDynamic: decodedRequest[0][15]
        };

        // Construct log for Merkle proof
        const log = {
            l2ShardId: 0,
            isService: true,
            txNumberInBatch: l2TxNumberInBlock,
            sender: L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR,
            key: ethers.keccak256(ethers.AbiCoder.defaultAbiCoder().encode(['address'], [interop1_wallet.address])),
            value: ethers.keccak256(ethers.AbiCoder.defaultAbiCoder().encode(['bytes'], [message]))
        };

        const leafHash = ethers.keccak256(ethers.AbiCoder.defaultAbiCoder().encode([L2_LOG_STRING], [log]));

        const proof1 =
            ethers.ZeroHash +
            ethers.AbiCoder.defaultAbiCoder()
                .encode(
                    ['uint256', 'uint256', 'uint256', 'bytes32'],
                    [(await sender_chain_provider.getNetwork()).chainId, l1BatchNumber, l2TxNumberInBlock, leafHash]
                )
                .slice(2);

        // Construct the interop transaction
        const interopTx = {
            type: INTEROP_TX_TYPE,
            from: `0x${xl2Input.from.toString(16).padStart(40, '0')}`,
            to: `0x${xl2Input.to.toString(16).padStart(40, '0')}`,
            chainId: (await receiver_chain_provider.getNetwork()).chainId,
            data: xl2Input.data,
            nonce: Math.floor(Math.random() * 1000000),
            customData: {
                paymaster_params: { paymaster: ethers.ZeroAddress, paymasterInput: '0x' },
                merkleProof: proof1,
                fullFee: '0xf000000000000000',
                toMint:
                    (xl2Input.reserved[0].toString(16).length % 2 == 0 ? '0x' : '0x0') +
                    xl2Input.reserved[0].toString(16),
                refundRecipient: await interop1_wallet.getAddress()
            },
            maxFeePerGas: xl2Input.maxFeePerGas,
            maxPriorityFeePerGas: xl2Input.maxPriorityFeePerGas,
            gasLimit: xl2Input.gasLimit,
            value: xl2Input.value
        };
        const hexTx = zksync.utils.serializeEip712(interopTx);

        const receiverChainId = (await receiver_chain_provider.getNetwork()).chainId;
        const interop1ChainId = (await interop1_provider.getNetwork()).chainId;
        // console.log("kl tod inteorp tx", interopTx)
        console.log(`Broadcasting interop tx to ${receiverChainId === interop1ChainId ? 'Interop1' : 'Interop2'}`);
        const broadcastTx = await receiver_chain_provider.broadcastTransaction(hexTx);
        await delay(timeout * 2);

        const interopTxAsCanonicalTx = {
            txType: 253n,
            from: interopTx.from,
            to: interopTx.to,
            gasLimit: interopTx.gasLimit,
            gasPerPubdataByteLimit: 50000n,
            maxFeePerGas: interopTx.maxFeePerGas,
            maxPriorityFeePerGas: 0,
            paymaster: interopTx.customData.paymaster_params.paymaster,
            nonce: interopTx.nonce,
            value: interopTx.value,
            reserved: [interopTx.customData.toMint, interopTx.customData.refundRecipient, '0x00', '0x00'],
            data: interopTx.data,
            signature: '0x',
            factoryDeps: [],
            paymasterInput: '0x',
            reservedDynamic: proof1
        };
        const encodedTx = ethers.AbiCoder.defaultAbiCoder().encode(
            [BRIDGEHUB_L2_CANONICAL_TRANSACTION_ABI],
            [interopTxAsCanonicalTx]
        );
        const interopTxHash = ethers.keccak256(ethers.getBytes(encodedTx));
        console.log('interopTxHash', interopTxHash);

        // Recursively read and broadcast
        await readAndBroadcastInteropTx(interopTxHash, receiver_chain_provider, sender_chain_provider);
    }

    async function getTokenBalance({
        provider,
        tokenAddress,
        address
    }: {
        provider: zksync.Provider;
        tokenAddress: string;
        address: string;
    }) {
        const tokenContract = new zksync.Contract(tokenAddress, ArtifactMintableERC20.abi, provider);
        return await tokenContract.balanceOf(address);
    }

    async function getTokenAllowance({
        provider,
        tokenAddress,
        fromAddress,
        toAddress
    }: {
        provider: zksync.Provider;
        tokenAddress: string;
        fromAddress: string;
        toAddress: string;
    }) {
        const tokenContract = new zksync.Contract(tokenAddress, ArtifactMintableERC20.abi, provider);
        return await tokenContract.allowance(fromAddress, toAddress);
    }

    /**
     * Utility function to delay execution for a specified time.
     * @param ms - Milliseconds to delay.
     */
    function delay(ms: number) {
        return new Promise((resolve) => setTimeout(resolve, ms));
    }
});
