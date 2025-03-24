/**
 * This suite contains tests checking interop behavior.
 */

import { TestMaster } from '../src';
import { Token } from '../src/types';

import * as zksync from 'zksync-ethers-interop-support';
import * as ethers from 'ethers';
import { Wallet } from 'ethers';
import {
    scaledGasPrice,
    deployContract,
    waitForBlockToBeFinalizedOnL1,
    waitForL2ToL1LogProof,
    waitUntilBlockCommitted,
    waitUntilBlockFinalized
} from '../src/helpers';

import {
    L2_ASSET_ROUTER_ADDRESS,
    L2_BRIDGEHUB_ADDRESS,
    L2_NATIVE_TOKEN_VAULT_ADDRESS,
    L2_INTEROP_HANDLER_ADDRESS,
    L2_INTEROP_CENTER_ADDRESS,
    L2_STANDARD_TRIGGER_ACCOUNT_ADDRESS,
    REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
    L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR,
    BRIDGEHUB_L2_CANONICAL_TRANSACTION_ABI,
    ETH_ADDRESS_IN_CONTRACTS,
    L2_LOG_STRING,
    ARTIFACTS_PATH,
    INTEROP_CALL_ABI,
    MESSAGE_INCLUSION_PROOF_ABI,
    ArtifactBridgeHub,
    ArtifactInteropCenter,
    ArtifactInteropHandler,
    ArtifactMessageRootStorage,
    L2_MESSAGE_ROOT_STORAGE_ADDRESS,
    ArtifactNativeTokenVault,
    ArtifactMintableERC20,
    ArtifactL1AssetRouter,
    ArtifactSwap
} from '../src/constants';
import { RetryProvider } from '../src/retry-provider';

import { getInteropTriggerData, getInteropBundleData } from '../src/temp-sdk';

const richPk = '0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110';

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
    let skipTest = false;

    // Interop1 (Main Chain) Variables
    let interop1_provider: zksync.Provider;
    let interop1_wallet: zksync.Wallet;
    let interop1_rich_wallet: zksync.Wallet;
    let interop1_bridgehub_contract: zksync.Contract;
    let interop1_interop_center_contract: zksync.Contract;
    let interop2_interop_handler: zksync.Contract;
    let interop2_message_root_storage: zksync.Contract;
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

        // Setup Interop2 Provider and Wallet
        interop2_provider = new RetryProvider(
            { url: 'http://localhost:3150', timeout: 1200 * 1000 },
            undefined,
            testMaster.reporter
        );
        try {
            const blockNumber = await interop2_provider.getBlockNumber();
            console.log('Second chain started, block number:', blockNumber);
        } catch (_) {
            console.log('Second chain not started, skipping');
            skipTest = true;
            return;
        }
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
        interop2_message_root_storage = new zksync.Contract(
            L2_MESSAGE_ROOT_STORAGE_ADDRESS,
            ArtifactMessageRootStorage.abi,
            interop2_wallet.provider
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

        // console.log('PK', test_wallet_pk);
        // console.log(`Test wallet 1 address: ${interop1_wallet.address}`);
        // console.log(`Test wallet 2 address: ${interop1_wallet.address}`);
        // console.log(
        //     `[rich wallet] l1_wallet address: ${l1_wallet.address} with ${ethers.formatEther(
        //         await l1_provider.getBalance(l1_wallet.address)
        //     )} ETH`
        // );
        // console.log('--------------------');

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

        aliased_interop1_wallet_address = await interop2_interop_handler.getAliasedAccount(interop1_wallet.address, 0);
        // console.log('aliased address', aliased_interop1_wallet_address);
    });

    test('Can perform an ETH deposit', async () => {
        if (skipTest) {
            // console.log('Skipping ETH deposit test');
            return;
        }
        // Fund accounts
        const gasPrice = await scaledGasPrice(interop1_rich_wallet);
        const fundAmount = ethers.parseEther('10');
        // console.log('Funding test wallet at interop1');
        let nonce = await interop1_rich_wallet.getNonce();
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
        // console.log('Funding test wallet at interop2');
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
        // console.log('Test wallet funded');
    });

    test('Can deploy token contracts', async () => {
        if (skipTest) {
            // console.log('Skipping token deployment test');
            return;
        }
        // Deploy token A on interop1 and register
        // console.log('Deploying token A on Interop1');
        const interop1_tokenA_contract_deployment = await deployContract(interop1_wallet, ArtifactMintableERC20, [
            tokenA_details.name,
            tokenA_details.symbol,
            tokenA_details.decimals
        ]);
        tokenA_details.l2Address = await interop1_tokenA_contract_deployment.getAddress();
        // console.log('Registering token A on Interop1');
        await Promise.all([
            (await interop1_nativeTokenVault_contract.registerToken(tokenA_details.l2Address)).wait(),
            (await interop1_tokenA_contract_deployment.mint(await interop1_wallet.getAddress(), 1000)).wait(),
            (await interop1_tokenA_contract_deployment.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, 1000)).wait()
        ]);
        // console.log('Token A registered on Interop1');
        tokenA_details.assetId = await interop1_nativeTokenVault_contract.assetId(tokenA_details.l2Address);
        tokenA_details.l2AddressSecondChain = await interop2_nativeTokenVault_contract.tokenAddress(
            tokenA_details.assetId
        );
        interop1_tokenA_contract = new zksync.Contract(
            tokenA_details.l2Address,
            ArtifactMintableERC20.abi,
            interop1_wallet
        );
        // console.log('Token A info:', tokenA_details);
    });

    test('Can perform cross chain transfer', async () => {
        if (skipTest) {
            console.log('Skipping cross chain transfer test');
            return;
        }
        // Fund accounts
        const gasPrice = await scaledGasPrice(interop1_rich_wallet);
        const fundAmount = ethers.parseEther('10');
        // console.log('Cross chain swap and additional calls...');
        // await printBalances('before request interop');

        await (await interop1_tokenA_contract.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, swapAmount)).wait();

        // Mint token A on Interop1 for test wallet
        // console.log('Minting token A on Interop1 for test wallet...');
        await (await interop1_tokenA_contract.mint(interop1_wallet.address, ethers.parseEther('500'))).wait();
        // console.log('[SETUP COMPLETED]');

        // Send Transfer Transaction
        // console.log('fundAmount', fundAmount);
        // console.log('interop1_wallet.privateKey', interop1_wallet.privateKey);
        const tx = await from_interop1_requestInterop(
            //feeStarters
            [
                {
                    directCall: true,
                    nextContract: L2_STANDARD_TRIGGER_ACCOUNT_ADDRESS,
                    data: '0x',
                    value: '0x0',
                    requestedInteropCallValue: '0x' + mintValue.toString(16)
                }
            ],
            //executionStarters
            [
                // getL2TokenTransferIndirectStarter(),
                {
                    directCall: false,
                    nextContract: L2_ASSET_ROUTER_ADDRESS,
                    data: getTokenTransferSecondBridgeData(
                        tokenA_details.assetId!,
                        swapAmount,
                        aliased_interop1_wallet_address
                    ),
                    value: '0x0',
                    requestedInteropCallValue: '0x0'
                }
            ],
            {
                gasLimit: 30000000,
                gasPerPubdataByteLimit: 1000,
                refundRecipient: interop1_wallet.address,
                paymaster: ethers.ZeroAddress,
                paymasterInput: '0x'
            }
        );

        // console.log("tx", tx)
        const receipt = await tx.wait();

        await delay(timeout);

        // await waitForL2ToL1LogProof(sender_chain_utilityWallet, txReceipt!.blockNumber, txHash);
        await waitUntilBlockCommitted(interop1_wallet, receipt!.blockNumber);
        // kl todo wait here for the batch to be committd by checking L1.
        // this should be checked in the server, we should not need this.
        await delay(10000);
        // kl todo. to add message roots we need to send a tx. Otherwise the gas estimation likely fails.

        // console.log(interop2_message_root_storage.interface)
        // console.log('waiting for message root');
        const receipt2 = await interop1_wallet.provider.getTransactionReceipt(tx.hash);
        // console.log(receipt2);
        const interop1_chainId = (await interop1_wallet.provider.getNetwork()).chainId;
        while (true) {
            await delay(1000);
            await (
                await interop1_wallet.transfer({
                    to: interop1_wallet.address,
                    amount: 1
                })
            ).wait();
            await (
                await interop2_wallet.transfer({
                    to: interop2_wallet.address,
                    amount: 1
                })
            ).wait();
            const msgRoots = await interop2_message_root_storage.msgRoots(interop1_chainId, receipt2!.l1BatchNumber);
            // console.log('msgRoots', msgRoots);
            if (msgRoots !== ethers.ZeroHash) {
                break;
            }
        }

        // Read and broadcast the interop transaction from Interop1 to Interop2

        await readAndBroadcastInteropTx(tx.hash, interop1_provider, interop2_provider);
        await delay(timeout);

        tokenA_details.l2AddressSecondChain = await interop2_nativeTokenVault_contract.tokenAddress(
            tokenA_details.assetId
        );
        // await printBalances('after request transfer');
        expect(
            (await getTokenBalance({
                provider: interop2_provider,
                tokenAddress: tokenA_details.l2AddressSecondChain!,
                address: aliased_interop1_wallet_address
            })) > 0n
        ).toBe(true);
    });

    interface InteropCallStarter {
        directCall: boolean;
        nextContract: string;
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
        paymaster: string;
        paymasterInput: string;
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
            L2_STANDARD_TRIGGER_ACCOUNT_ADDRESS,
            // feePaymentCallStarters:
            feePaymentCallStarters,
            // executionCallStarters:
            executionCallStarters,
            // gasFields:
            {
                gasLimit: 600000000,
                gasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
                refundRecipient: interop1_wallet.address,
                paymaster: ethers.ZeroAddress,
                paymasterInput: '0x'
            }
            // refundRecipient: interop1_wallet.address
        ];

        // console.log("input", input)
        // console.log("interop1_interop_center_contract", interop1_interop_center_contract.interface.fragments[21])
        // console.log("interop1_interop_center_contract", interop1_interop_center_contract.interface.fragments[22])
        // console.log("interop1_interop_center_contract", interop1_interop_center_contract.interface.fragments[23])
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
                [
                    assetId,
                    new ethers.AbiCoder().encode(
                        ['uint256', 'address', 'address'],
                        [amount, recipient, ethers.ZeroAddress]
                    )
                ]
            )
        ]);
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
        console.log('*Reading and broadcasting interop tx initiated by txHash*', txHash);
        const sender_chain_utilityWallet = new zksync.Wallet(
            zksync.Wallet.createRandom().privateKey,
            sender_chain_provider
        );
        const txReceipt = await sender_chain_provider.getTransactionReceipt(txHash);
        // await waitForL2ToL1LogProof(sender_chain_utilityWallet, txReceipt!.blockNumber, txHash);
        await waitUntilBlockCommitted(sender_chain_utilityWallet, txReceipt!.blockNumber);

        const {
            l1BatchNumber: l1BatchNumber_trigger,
            l2TxNumberInBlock: l2TxNumberInBlock_trigger,
            output: triggerData,
            rawData: rawData_trigger,
            l2MessageIndex: l2MessageIndex_trigger,
            fullProof: proof_trigger
        } = await getInteropTriggerData(sender_chain_provider, txHash, 2);
        const {
            l1BatchNumber: l1BatchNumber_fee,
            l2TxNumberInBlock: l2TxNumberInBlock_fee,
            output: feeBundleData,
            rawData: rawData_fee,
            l2MessageIndex: l2MessageIndex_fee,
            fullProof: proof_fee
        } = await getInteropBundleData(sender_chain_provider, txHash, 0);
        const {
            l1BatchNumber: l1BatchNumber_execution,
            l2TxNumberInBlock: l2TxNumberInBlock_execution,
            output: executionBundleData,
            rawData: rawData_execution,
            l2MessageIndex: l2MessageIndex_execution,
            fullProof: proof_execution
        } = await getInteropBundleData(sender_chain_provider, txHash, 1);
        if (triggerData == null) {
            return;
        }

        // console.log("trigger message", rawData_trigger)

        // console.log('triggerData', triggerData);
        // console.log('feeBundleData', feeBundleData);
        // console.log('executionBundleData', executionBundleData);
        // throw new Error('stop here');

        // const fromFormatted = `0x${xl2Input.from.toString(16).padStart(40, '0')}`;
        // const toFormatted = `0x${xl2Input.to.toString(16).padStart(40, '0')}`;

        const txData = ethers.AbiCoder.defaultAbiCoder().encode(
            ['bytes', 'bytes'],
            [rawData_execution, proof_execution]
        );
        // Construct the interop transaction
        const nonce = await receiver_chain_provider.getTransactionCount(L2_STANDARD_TRIGGER_ACCOUNT_ADDRESS);
        const feeData = await receiver_chain_provider.getFeeData();
        let interopTx = {
            from: L2_STANDARD_TRIGGER_ACCOUNT_ADDRESS, // kl todo
            to: L2_INTEROP_HANDLER_ADDRESS,
            chainId: (await receiver_chain_provider.getNetwork()).chainId.toString(),
            data: txData,
            nonce: nonce,
            customData: {
                paymasterParams: {
                    paymaster: triggerData.gasFields.paymaster,
                    paymasterInput: triggerData.gasFields.paymasterInput
                },
                customSignature: '',
                gasPerPubdata: triggerData.gasFields.gasPerPubdataByteLimit
            },
            maxFeePerGas: feeData.maxFeePerGas,
            maxPriorityFeePerGas: feeData.maxPriorityFeePerGas,
            gasLimit: triggerData.gasFields.gasLimit,
            value: 0
        };

        const custom_sig = ethers.AbiCoder.defaultAbiCoder().encode(
            ['bytes', 'bytes', 'address', 'address', 'bytes'],
            [rawData_fee, proof_fee, triggerData.from, triggerData.gasFields.refundRecipient, proof_trigger]
        );
        // console.log('interopTx', interopTx, 'sig', custom_sig);
        interopTx.customData.customSignature = custom_sig;

        // console.log(`Transaction({
        //     txType: ${interopTx.customData ? 113 : 0},
        //     from: uint256(uint160(${interopTx.from})),
        //     to: uint256(uint160(${interopTx.to})), 
        //     gasLimit: ${interopTx.gasLimit},
        //     gasPerPubdataByteLimit: ${interopTx.customData?.gasPerPubdata || 0},
        //     maxFeePerGas: ${interopTx.maxFeePerGas || 0},
        //     maxPriorityFeePerGas: ${interopTx.maxPriorityFeePerGas || 0},
        //     paymaster: uint256(uint160(${interopTx.customData?.paymasterParams?.paymaster || 0})),
        //     nonce: ${interopTx.nonce},
        //     value: ${interopTx.value},
        //     reserved: [uint256(0), 0, 0, 0],
        //     data: hex"${interopTx.data.slice(2)}",
        //     signature: hex"${(interopTx.customData?.customSignature || '').slice(2)}",
        //     factoryDeps: new bytes32[](0),
        //     paymasterInput: "${interopTx.customData?.paymasterParams?.paymasterInput || ''}",
        //     reservedDynamic: ""
        // });`);
        
        // console.log("interopTx", interopTx)
        const hexTx = zksync.utils.serializeEip712(interopTx);
        // const receiverChainId = (await receiver_chain_provider.getNetwork()).chainId;
        // const interop1ChainId = (await interop1_provider.getNetwork()).chainId;
        // console.log("kl tod inteorp tx", interopTx)
        // console.log(`Broadcasting interop tx to ${receiverChainId === interop1ChainId ? 'Interop1' : 'Interop2'}`);
        const broadcastTx = await receiver_chain_provider.broadcastTransaction(hexTx);

        // console.log('Resolved hash', broadcastTx.realInteropHash);
        console.log('broadcastTx hash', broadcastTx.hash);
        // await delay(timeout * 3);

        // Recursively read and broadcast
        // await readAndBroadcastInteropTx(broadcastTx.realInteropHash!, receiver_chain_provider, sender_chain_provider);
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
        if (tokenAddress == undefined || tokenAddress == ethers.ZeroAddress) {
            return 0;
        }
        // console.log('tokenAddress', tokenAddress);
        const tokenContract = new zksync.Contract(tokenAddress, ArtifactMintableERC20.abi, provider);
        return await tokenContract.balanceOf(address);
    }

    /**
     * Utility function to delay execution for a specified time.
     * @param ms - Milliseconds to delay.
     */
    function delay(ms: number) {
        return new Promise((resolve) => setTimeout(resolve, ms));
    }
});
