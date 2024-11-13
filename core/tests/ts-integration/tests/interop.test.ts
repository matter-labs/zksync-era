/**
 * This suite contains tests checking interop behavior.
 */

import * as fs from 'fs';

import { TestMaster } from '../src/index';
import { Token } from '../src/types';

import * as zksync from 'zksync-ethers-interop-support';
import * as ethers from 'ethers';
import { Wallet } from 'ethers';
import { scaledGasPrice, deployContract, waitForBlockToBeFinalizedOnL1 } from '../src/helpers';

import {
    L2_ASSET_ROUTER_ADDRESS,
    L2_BRIDGEHUB_ADDRESS,
    L2_NATIVE_TOKEN_VAULT_ADDRESS,
    L2_INTEROP_HANDLER_ADDRESS,
    L2_INTEROP_CENTER_ADDRESS,
    REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
    L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR,
    BRIDGEHUB_L2_CANONICAL_TRANSACTION_ABI,
    ETH_ADDRESS_IN_CONTRACTS,
    L2_LOG_STRING,
    ARTIFACTS_PATH,
    INTEROP_CALL_ABI
} from '../src/constants';
import { RetryProvider } from '../src/retry-provider';

import { getInteropTriggerData, getInteropBundleData } from '../src/temp-sdk';

// Read contract artifacts
function readContract(path: string, fileName: string, contractName?: string) {
    contractName = contractName || fileName;
    return JSON.parse(fs.readFileSync(`${path}/${fileName}.sol/${contractName}.json`, { encoding: 'utf-8' }));
}

const ArtifactBridgeHub = readContract(`${ARTIFACTS_PATH}bridgehub`, 'Bridgehub');
const ArtifactInteropCenter = readContract(`${ARTIFACTS_PATH}bridgehub`, 'InteropCenter');
const ArtifactInteropHandler = readContract(
    `${ARTIFACTS_PATH}bridgehub`,
    'InteropHandler'
);
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

    let tokenA_details: Token = {
        name: 'Token A',
        symbol: 'AA',
        decimals: 18n,
        l1Address: '',
        l2Address: '',
        l2AddressSecondChain: ''
    };
    let tokenB_details: Token = {
        name: 'Token B',
        symbol: 'BB',
        decimals: 18n,
        l1Address: '',
        l2Address: '',
        l2AddressSecondChain: ''
    };

    // Common Variables
    const timeout = 10000;

    // Interop1 (Main Chain) Variables
    let interop1_provider: zksync.Provider;
    let interop1_wallet: zksync.Wallet;
    let interop1_rich_wallet: zksync.Wallet;
    let interop1_bridgehub_contract: zksync.Contract;
    let interop1_interop_center_contract: zksync.Contract;
    let interop2_interop_handler: zksync.Contract;
    let interop1_nativeTokenVault_contract: zksync.Contract;
    let interop1_tokenA_contract: zksync.Contract;
    let aliased_interop1_wallet_address: string;

    // Interop2 (Second Chain) Variables
    let interop2_provider: zksync.Provider;
    let interop2_wallet: zksync.Wallet;
    let interop2_rich_wallet: zksync.Wallet;
    let interop2_nativeTokenVault_contract: zksync.Contract;
    let interop2_swap_contract: zksync.Contract;
    let interop2_tokenB_contract: zksync.Contract;

    const swapAmount = ethers.parseEther('0.1');
    const mintValue = ethers.parseEther('0.2');
    const bridgeBackAmount = ethers.parseEther('0.2');

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
        interop1_interop_center_contract = new zksync.Contract(
            L2_INTEROP_CENTER_ADDRESS,
            ArtifactInteropCenter.abi,
            interop1_wallet
        );
        interop2_interop_handler = new zksync.Contract(
            L2_INTEROP_HANDLER_ADDRESS,
            ArtifactInteropHandler.abi,
            interop2_wallet
        );
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
        console.log(`Test wallet 2 address: ${interop1_wallet.address}`);
        console.log(
            `[rich wallet] l1_wallet address: ${l1_wallet.address} with ${ethers.formatEther(
                await l1_provider.getBalance(l1_wallet.address)
            )} ETH`
        );
        console.log('--------------------');

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

        aliased_interop1_wallet_address = await interop2_interop_handler.aliasAccount(interop1_wallet.address);
        console.log('aliased address', aliased_interop1_wallet_address);
    });

    test('Can perform an ETH deposit', async () => {
        // Fund accounts
        const gasPrice = await scaledGasPrice(interop1_rich_wallet);
        const fundAmount = ethers.parseEther('10');
        console.log('Funding test wallet at interop1');
        await (
            await interop1_rich_wallet.deposit({
                token: ETH_ADDRESS_IN_CONTRACTS,
                amount: fundAmount * 10n,
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
                amount: fundAmount * 10n,
                to: interop2_wallet.address,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: { gasPrice },
                overrides: { gasPrice }
            })
        ).wait();
        console.log('Test wallet funded');
    });

    test.skip('Can perform an ETH interop', async () => {
        // Fund accounts
        const gasPrice = await scaledGasPrice(interop1_rich_wallet);
        const fundAmount = ethers.parseEther('10');
        console.log('Funding test wallet at interop1');
        printBalances('before eth value interop');

        // Send Transfer Transaction
        console.log('interop1_wallet.privateKey', interop1_wallet.privateKey);
        const tx = await from_interop1_requestInterop(
            [
                {
                    directCall: true,
                    to: interop1_wallet.address,
                    from: ethers.ZeroAddress,
                    data: '0x',
                    value: '0x' + mintValue.toString(16),
                    requestedInteropCallValue: '0x' + mintValue.toString(16)
                }
            ],
            //feeStarters
            [
                // getL2TokenTransferIndirectStarter(),
                {
                    directCall: false,
                    to: ethers.ZeroAddress,
                    from: L2_ASSET_ROUTER_ADDRESS,
                    data: getTokenTransferSecondBridgeData(
                        tokenA_details.assetId!,
                        swapAmount,
                        interop1_wallet.address
                    ),
                    value: '0x0',
                    requestedInteropCallValue: '0x0'
                }
            ],
            {
                gasLimit: 30000000,
                gasPerPubdataByteLimit: 1000,
                refundRecipient: interop1_wallet.address
            }
        );

        console.log('tx', tx);
        await tx.wait();

        await delay(timeout);

        // Read and broadcast the interop transaction from Interop1 to Interop2
        await readAndBroadcastInteropTx(tx.hash, interop1_provider, interop2_provider);
        await delay(timeout);

        printBalances('after eth value interop');

        console.log('Test wallet funded');
    });

    test('Can deploy token contracts', async () => {
        // Deploy token A on interop1 and register
        console.log('Deploying token A on Interop1');
        const interop1_tokenA_contract_deployment = await deployContract(interop1_wallet, ArtifactMintableERC20, [
            tokenA_details.name,
            tokenA_details.symbol,
            tokenA_details.decimals
        ]);
        tokenA_details.l2Address = await interop1_tokenA_contract_deployment.getAddress();
        console.log('Registering token A on Interop1');
        await (await interop1_nativeTokenVault_contract.registerToken(tokenA_details.l2Address)).wait();
        await (await interop1_tokenA_contract_deployment.mint(await interop1_wallet.getAddress(), 1000)).wait();
        await (await interop1_tokenA_contract_deployment.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, 1000)).wait();
        console.log('Token A registered on Interop1');
        tokenA_details.assetId = await interop1_nativeTokenVault_contract.assetId(tokenA_details.l2Address);
        tokenA_details.l2AddressSecondChain = await interop2_nativeTokenVault_contract.tokenAddress(
            tokenA_details.assetId
        );
        interop1_tokenA_contract = new zksync.Contract(
            tokenA_details.l2Address,
            ArtifactMintableERC20.abi,
            interop1_wallet
        );
        console.log('Token A info:', tokenA_details);

        // Deploy token B on interop2 and register
        console.log('Deploying token B on Interop2');
        const interop2_tokenB_contract_deployment = await deployContract(interop2_wallet, ArtifactMintableERC20, [
            tokenB_details.name,
            tokenB_details.symbol,
            tokenB_details.decimals
        ]);
        tokenB_details.l2AddressSecondChain = await interop2_tokenB_contract_deployment.getAddress();
        await (
            await interop2_tokenB_contract_deployment.mint(await interop1_wallet.getAddress(), ethers.parseEther('100'))
        ).wait();
        await (
            await interop2_tokenB_contract_deployment.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, ethers.parseEther('100'))
        ).wait();
        console.log('Registering token B on Interop2');
        await (await interop2_nativeTokenVault_contract.registerToken(tokenB_details.l2AddressSecondChain)).wait();
        console.log('Token B registered on Interop2');
        // await delay(timeout);
        tokenB_details.assetId = await interop2_nativeTokenVault_contract.assetId(tokenB_details.l2AddressSecondChain);
        tokenB_details.l2Address = await interop1_nativeTokenVault_contract.tokenAddress(tokenB_details.assetId);
        interop2_tokenB_contract = new zksync.Contract(
            tokenB_details.l2AddressSecondChain,
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
            token: tokenA_details.l2Address,
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
            token: tokenB_details.l2AddressSecondChain!,
            amount: 100
        });

        await expect(withdrawB).toBeAccepted();
        const withdrawBTx = await withdrawB;
        const l2TxBReceipt = await interop2_wallet.provider.getTransactionReceipt(withdrawBTx.hash);
        await withdrawBTx.waitFinalize();
        await waitForBlockToBeFinalizedOnL1(interop2_wallet, l2TxBReceipt!.blockNumber);

        await interop2_wallet.finalizeWithdrawalParams(withdrawBTx.hash); // kl todo finalize the Withdrawals with the params here. Alternatively do in the SDK.
        await expect(interop2_rich_wallet.finalizeWithdrawal(withdrawBTx.hash)).toBeAccepted();

        tokenA_details.l1Address = await l1NativeTokenVault.tokenAddress(tokenA_details.assetId);
        tokenB_details.l1Address = await l1NativeTokenVault.tokenAddress(tokenB_details.assetId);

        await expect(
            interop1_wallet.deposit({
                token: tokenB_details.l1Address,
                amount: 50,
                approveERC20: true
            })
        ).toBeAccepted();

        await expect(
            interop2_wallet.deposit({
                token: tokenA_details.l1Address,
                amount: 50,
                approveERC20: true
            })
        ).toBeAccepted();

        tokenA_details.l2AddressSecondChain = await interop2_nativeTokenVault_contract.tokenAddress(
            tokenA_details.assetId
        );
        tokenB_details.l2Address = await interop1_nativeTokenVault_contract.tokenAddress(tokenB_details.assetId);

        console.log(tokenA_details);
        console.log(tokenB_details);
    });

    test('Deploy swap contract', async () => {
        // Deploy Swap Contracts on Interop2
        console.log('Deploying Swap Contract on Interop2');
        const interop2_swap_contract_deployment = await deployContract(interop2_wallet, ArtifactSwap, [
            tokenA_details.l2AddressSecondChain!,
            tokenB_details.l2AddressSecondChain!
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
                    tokenAddress: tokenB_details.l2AddressSecondChain!,
                    address: interop2_swap_contract_address
                })
            )} BB`
        );

        // Mint token A on Interop1 for test wallet
        console.log('Minting token A on Interop1 for test wallet...');
        await (await interop1_tokenA_contract.mint(interop1_wallet.address, ethers.parseEther('500'))).wait();
        console.log('[SETUP COMPLETED]');
    });

    // test('Can transfer token A from Interop1 to Interop2', async () => {
    //     await printBalances('before token transfer');
    //     // Send Transfer Transaction
    //     await from_interop1_transfer_tokenA();
    //     await delay(timeout);
    //     await printBalances('after token transfer');
    //     // await delay(1);
    // });

    test('Can perform request interop new interface', async () => {
        // Fund accounts
        const gasPrice = await scaledGasPrice(interop1_rich_wallet);
        const fundAmount = ethers.parseEther('10');
        console.log('Cross chain swap and additional calls...');
        await printBalances('before request interop');

        await (await interop1_tokenA_contract.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, swapAmount)).wait();

        // Send Transfer Transaction
        console.log('fundAmount', fundAmount);
        console.log('interop1_wallet.privateKey', interop1_wallet.privateKey);
        const tx = await from_interop1_requestInterop(
            [
                {
                    directCall: true,
                    to: aliased_interop1_wallet_address,
                    from: ethers.ZeroAddress,
                    data: '0x',
                    value: '0x' + mintValue.toString(16),
                    requestedInteropCallValue: '0x' + mintValue.toString(16)
                }
            ],
            //feeStarters
            [
                // getL2TokenTransferIndirectStarter(),
                {
                    directCall: false,
                    to: ethers.ZeroAddress,
                    from: L2_ASSET_ROUTER_ADDRESS,
                    data: getTokenTransferSecondBridgeData(
                        tokenA_details.assetId!,
                        swapAmount,
                        aliased_interop1_wallet_address
                    ),
                    value: '0x0',
                    requestedInteropCallValue: '0x0'
                },
                // getCrossChainSwapApprovalDirectStarter(),
                {
                    directCall: true,
                    to: tokenA_details.l2AddressSecondChain!,
                    from: ethers.ZeroAddress,
                    data: interop1_tokenA_contract.interface.encodeFunctionData('approve', [
                        await interop2_swap_contract.getAddress(),
                        swapAmount
                    ]),
                    value: '0x0',
                    requestedInteropCallValue: '0x0'
                },
                // getCrossChainSwapDirectStarter(),
                {
                    directCall: true,
                    to: await interop2_swap_contract.getAddress(),
                    from: ethers.ZeroAddress,
                    data: interop2_swap_contract.interface.encodeFunctionData('swap', [swapAmount]),
                    value: '0x0',
                    requestedInteropCallValue: '0x0'
                },
                // getCrossChainNtvApprovalDirectStarter(),
                {
                    directCall: true,
                    to: tokenB_details.l2AddressSecondChain!,
                    from: ethers.ZeroAddress,
                    data: interop1_tokenA_contract.interface.encodeFunctionData('approve', [
                        L2_NATIVE_TOKEN_VAULT_ADDRESS,
                        swapAmount * 2n
                    ]),
                    value: '0x0',
                    requestedInteropCallValue: '0x0'
                },
                // getTransferBackTokenDirectStarter()
                {
                    directCall: true,
                    to: L2_BRIDGEHUB_ADDRESS,
                    from: ethers.ZeroAddress,
                    data: await getRequestL2TransactionTwoBridgesData(
                        (
                            await interop1_wallet.provider.getNetwork()
                        ).chainId,
                        mintValue,
                        0n,
                        getTokenTransferSecondBridgeData(tokenB_details.assetId!, swapAmount, interop1_wallet.address),
                        interop1_wallet.address
                    ),
                    value: '0x' + mintValue.toString(16), // note in two bridges this is * 2n , because we pay for gas as well. This cleans it up.
                    requestedInteropCallValue: '0x' + mintValue.toString(16)
                }
            ],
            {
                gasLimit: 30000000,
                gasPerPubdataByteLimit: 1000,
                refundRecipient: interop1_wallet.address
            }
        );

        // console.log("tx", tx)
        await tx.wait();

        await delay(timeout);

        // Read and broadcast the interop transaction from Interop1 to Interop2
        await readAndBroadcastInteropTx(tx.hash, interop1_provider, interop2_provider);
        await delay(timeout);

        await printBalances('after request interop');
    });

    /**
     * Sends a transfer transaction from Interop1 to Interop2.
     */
    async function from_interop1_transfer_tokenA() {
        const amount = ethers.parseEther('0.1');
        const mintValue = ethers.parseEther('0.2');
        const bridgeCalldata = getTokenTransferSecondBridgeData(
            tokenA_details.assetId!,
            amount,
            interop1_wallet.address
        );

        await (await interop1_tokenA_contract.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, amount)).wait();
        const tx = await from_interop1_requestL2TransactionTwoBridges(mintValue, bridgeCalldata);
        await tx.wait();

        await delay(timeout);

        await readAndBroadcastInteropTx(tx.hash, interop1_provider, interop2_provider);
    }

    // /**
    //  * Sends an approve transaction from Interop1 to Interop2.
    //  */
    // async function from_interop1_approveSwapAllowance() {
    //     const amount = ethers.parseEther('0.1');
    //     const mintValue = ethers.parseEther('0.2');

    //     // Use the token contract on Interop2 (Second Chain)
    //     const l2Calldata = interop1_tokenA_contract.interface.encodeFunctionData('approve', [
    //         await interop2_swap_contract.getAddress(),
    //         amount
    //     ]);

    //     // Create an interop transaction from Interop1 to Interop2
    //     const tx = await from_interop1_requestL2TransactionDirect(
    //         mintValue,
    //         tokenA_details.l2AddressSecondChain!,
    //         BigInt(0),
    //         l2Calldata
    //     );
    //     await tx.wait();

    //     await delay(timeout);

    //     // Read and broadcast the interop transaction from Interop1 to Interop2
    //     await readAndBroadcastInteropTx(tx.hash, interop1_provider, interop2_provider);
    // }

    // /**
    //  * Sends a swap transaction from Interop1 to Interop2.
    //  */
    // async function from_interop1_swap_a_to_b() {
    //     const amount = ethers.parseEther('0.1');
    //     const mintValue = ethers.parseEther('0.2');

    //     // Prepare calldata using the Swap contract on Interop2
    //     const l2Calldata = interop2_swap_contract.interface.encodeFunctionData('swap', [amount]);

    //     // Create interop transaction from Interop1 to Interop2
    //     const tx = await from_interop1_requestL2TransactionDirect(
    //         mintValue,
    //         await interop2_swap_contract.getAddress(),
    //         BigInt(0),
    //         l2Calldata
    //     );
    //     await readAndBroadcastInteropTx(tx.hash, interop1_provider, interop2_provider);
    // }

    // /**
    //  * Sends a transfer transaction from Interop2 to Interop1.
    //  */
    // async function from_interop2_transfer_tokenB() {
    //     const amount = ethers.parseEther('0.2'); // swap ratio is 1:2 (A:B)
    //     const mintValue = ethers.parseEther('0.2');
    //     const bridgeCalldata = getTokenTransferSecondBridgeData(tokenB_details.assetId!, amount, interop1_wallet.address);

    //     const input = {
    //         chainId: (await interop1_provider.getNetwork()).chainId,
    //         mintValue,
    //         l2Value: 0,
    //         l2GasLimit: 30000000,
    //         l2GasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
    //         refundRecipient: interop1_wallet.address,
    //         secondBridgeAddress: L2_ASSET_ROUTER_ADDRESS,
    //         secondBridgeValue: 0,
    //         secondBridgeCalldata: bridgeCalldata
    //     };
    //     const l2Calldata = interop1_bridgehub_contract.interface.encodeFunctionData('requestL2TransactionTwoBridges', [input]);

    //     // Create interop transaction from Interop1 to Interop2
    //     const tx = await from_interop1_requestL2TransactionDirect(
    //         mintValue * 2n,
    //         L2_BRIDGEHUB_ADDRESS,
    //         mintValue,
    //         l2Calldata
    //     );
    //     await readAndBroadcastInteropTx(tx.hash, interop1_provider, interop2_provider);
    // }

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

    interface InteropCallStarter {
        directCall: boolean;
        to: string;
        from: string;
        data: string;
        value: string;
        // The value that is requested for the interop call.
        // This has to be known beforehand, as the funds in the interop call belong to the user.
        // This is because we cannot guarantee atomicity of xL2 txs (just the atomicity of calls on the destination chain)
        // So contracts cannot send their own value, only stamp the value that belongs to the user.
        requestedInteropCallValue: string;
    }

    interface GasFields {
        gasLimit: number;
        gasPerPubdataByteLimit: number;
        refundRecipient: string;
    }

    /**
     * Requests a direct L2 transaction on Interop1.
     * @param mintValue - The value to mint.
     * @param l2Contract - The target L2 contract address.
     * @param l2Value - The value to send with the transaction.
     * @param l2Calldata - The calldata for the transaction.
     * @returns The transaction response.
     */
    async function from_interop1_requestInterop(
        feePaymentCallStarters: InteropCallStarter[],
        executionCallStarters: InteropCallStarter[],
        gasFields: GasFields
    ) {
        const input = [
            // destinationChainId:
            (await interop2_provider.getNetwork()).chainId.toString(),
            // feePaymentCallStarters:
            feePaymentCallStarters,
            // executionCallStarters:
            executionCallStarters,
            // gasFields:
            {
                gasLimit: 600000000,
                gasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
                refundRecipient: interop1_wallet.address
            }
            // refundRecipient: interop1_wallet.address
        ];

        // console.log("input", input)
        // console.log("interop1_interop_center_contract", interop1_interop_center_contract.interface.fragments[32])
        const request = await interop1_interop_center_contract.requestInterop.populateTransaction(...input);
        request.value =
            mintValue +
            BigInt(
                executionCallStarters.reduce(
                    (acc: bigint, curr: InteropCallStarter) => acc + BigInt(curr.requestedInteropCallValue),
                    0n
                )
            );
        request.from = interop1_wallet.address;
        // console.log("request", request)

        const tx = await interop1_interop_center_contract.requestInterop(...input, {
            value: '0x' + request.value.toString(16),
            gasLimit: 30000000
        });

        await tx.wait();
        return tx;
    }

    function getTokenTransferSecondBridgeData(assetId: string, amount: bigint, recipient: string) {
        return ethers.concat([
            '0x01',
            new ethers.AbiCoder().encode(
                ['bytes32', 'bytes'],
                [assetId, new ethers.AbiCoder().encode(['uint256', 'address'], [amount, recipient])]
            )
        ]);
    }

    function getRequestL2TransactionTwoBridgesData(
        chainId: bigint,
        mintValue: bigint,
        l2Value: bigint,
        secondBridgeCalldata: string,
        refundRecipient: string
    ) {
        return interop1_bridgehub_contract.interface.encodeFunctionData('requestL2TransactionTwoBridges', [
            {
                chainId: chainId.toString(),
                mintValue,
                l2Value,
                l2GasLimit: 30000000,
                l2GasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
                refundRecipient: refundRecipient,
                secondBridgeAddress: L2_ASSET_ROUTER_ADDRESS,
                secondBridgeValue: 0,
                secondBridgeCalldata
            }
        ]);
    }

    //   function getRequestL2TransactionDirectData(
    //     mintValue: bigint,
    //     l2Contract: string,
    //     l2Value: bigint,
    //     l2Calldata: string,

    //   ) {
    //     return interop1_bridgehub_contract.interface.encodeFunctionData("requestL2TransactionDirect", [{
    //       chainId: interop2_chain.id.toString(),
    //       mintValue,
    //       l2Contract,
    //       l2Value,
    //       l2Calldata,
    //       l2GasLimit: 30000000,
    //       l2GasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
    //       factoryDeps: [],
    //       refundRecipient: interop1_zkAccount.address,
    //     }]);
    //   }

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
        console.log('txHash', txHash);
        // await getInteropTriggerData(sender_chain_provider, txHash, 0);
        // await getInteropTriggerData(sender_chain_provider, txHash, 1);
        // await getInteropTriggerData(sender_chain_provider, txHash, 2);
        // await getInteropTriggerData(sender_chain_provider, txHash, 3);
        // await getInteropTriggerData(sender_chain_provider, txHash, 4);
        // await getInteropTriggerData(sender_chain_provider, txHash, 5);

        const {
            l1BatchNumber: l1BatchNumber_trigger,
            l2TxNumberInBlock: l2TxNumberInBlock_trigger,
            output: triggerData,
            rawData: rawData_trigger
        } = await getInteropTriggerData(sender_chain_provider, txHash, 2);
        const {
            l1BatchNumber: l1BatchNumber_fee,
            l2TxNumberInBlock: l2TxNumberInBlock_fee,
            output: feeBundleData,
            rawData: rawData_fee
        } = await getInteropBundleData(sender_chain_provider, txHash, 0);
        const {
            l1BatchNumber: l1BatchNumber_execution,
            l2TxNumberInBlock: l2TxNumberInBlock_execution,
            output: executionBundleData,
            rawData: rawData_execution
        } = await getInteropBundleData(sender_chain_provider, txHash, 1);
        const xl2Input = triggerData;
        if (triggerData == null) {
            return;
        }

        // console.log("trigger message", rawData_trigger)

        console.log('triggerData', triggerData);
        console.log('feeBundleData', feeBundleData);
        console.log('executionBundleData', executionBundleData);
        // throw new Error('stop here');

        const fromFormatted = `0x${xl2Input.from.toString(16).padStart(40, '0')}`;
        // const toFormatted = `0x${xl2Input.to.toString(16).padStart(40, '0')}`;

        const txData = ethers.AbiCoder.defaultAbiCoder().encode(['bytes', 'bytes'], [rawData_fee, rawData_execution]);
        // Construct the interop transaction
        const nonce = await receiver_chain_provider.getTransactionCount(xl2Input.from);
        const feeData = await receiver_chain_provider.getFeeData();
        const interopTx = {
            // type: INTEROP_TX_TYPE,
            from: xl2Input.from,
            to: L2_INTEROP_HANDLER_ADDRESS,
            chainId: (await receiver_chain_provider.getNetwork()).chainId.toString(),
            data: txData,
            nonce: nonce,
            customData: {
                paymasterParams: { paymaster: ethers.ZeroAddress, paymasterInput: '0x' }
                // merkleProof: proof1,
                // fullFee: '0xf000000000000000',
                // toMint:
                //     (xl2Input.reserved[0].toString(16).length % 2 == 0 ? '0x' : '0x0') +
                //     xl2Input.reserved[0].toString(16),
                // refundRecipient: await interop1_wallet.getAddress()
            },
            maxFeePerGas: feeData.maxFeePerGas,
            maxPriorityFeePerGas: feeData.maxPriorityFeePerGas,
            gasLimit: 30000000,
            value: 0
        };

        console.log('interopTx', interopTx);
        const sig = ethers.Signature.from({
            v: 27,
            r: '0x0000000000000000000000000000000000000000000000000000000000000000',
            s: '0x0000000000000000000000000000000000000000000000000000000000000000'
        });
        const hexTx = zksync.utils.serializeEip712(interopTx, sig);
        // console.log("interopTx fail", xl2Input.unimplemented[0])
        const receiverChainId = (await receiver_chain_provider.getNetwork()).chainId;
        const interop1ChainId = (await interop1_provider.getNetwork()).chainId;
        // console.log("kl tod inteorp tx", interopTx)
        console.log(`Broadcasting interop tx to ${receiverChainId === interop1ChainId ? 'Interop1' : 'Interop2'}`);
        const broadcastTx = await receiver_chain_provider.broadcastTransaction(hexTx);

        console.log('Resolved hash', broadcastTx.realInteropHash);
        // console.log('broadcastTx hash', broadcastTx.hash);
        await delay(timeout * 3);

        // Recursively read and broadcast
        await readAndBroadcastInteropTx(broadcastTx.realInteropHash!, receiver_chain_provider, sender_chain_provider);
    }

    async function printBalances(balanceTime: string) {
        console.log(
            `Test wallet Eth Interop 1 balance ${balanceTime}: ${ethers.formatEther(
                await interop1_provider.getBalance(interop1_wallet.address)
            )} ETH, \n`,
            `Test wallet Eth Interop 2 balance ${balanceTime}: ${ethers.formatEther(
                await interop2_provider.getBalance(interop1_wallet.address)
            )} ETH, \n`,
            `Test wallet token A Interop 1 balance ${balanceTime}: ${ethers.formatEther(
                await getTokenBalance({
                    provider: interop1_provider,
                    tokenAddress: tokenA_details.l2Address,
                    address: interop1_wallet.address
                })
            )} AA, \n`,
            `Aliased Test wallet token A Interop 2 balance ${balanceTime}: ${ethers.formatEther(
                await getTokenBalance({
                    provider: interop2_provider,
                    tokenAddress: tokenA_details.l2AddressSecondChain!,
                    address: aliased_interop1_wallet_address
                })
            )} AA, \n`,
            `Test wallet token B Interop 1 balance ${balanceTime}: ${ethers.formatEther(
                await getTokenBalance({
                    provider: interop1_provider,
                    tokenAddress: tokenB_details.l2Address,
                    address: interop1_wallet.address
                })
            )} BB, \n`,
            `Test wallet token B Interop 2 balance ${balanceTime}: ${ethers.formatEther(
                await getTokenBalance({
                    provider: interop2_provider,
                    tokenAddress: tokenB_details.l2AddressSecondChain!,
                    address: aliased_interop1_wallet_address
                })
            )} BB`
        );
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
