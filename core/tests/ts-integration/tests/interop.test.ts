/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import * as fs from 'fs';

import { TestMaster } from '../src/index';
import { Token } from '../src/types';
// import { shouldChangeTokenBalances, shouldOnlyTakeFee } from '../src/modifiers/balance-checker';

// import * as zksync from 'zksync-ethers';
import * as zksync from 'zksync-ethers-interop-support';
import { Wallet } from 'ethers';
// import { BigNumber, utils as etherUtils } from 'ethers';
import * as ethers from 'ethers';
import { scaledGasPrice, deployContract } from '../src/helpers';
// import { L2_DEFAULT_ETH_PER_ACCOUNT } from '../src/context-owner';
// import { BridgehubFactory } from '../../../../contracts/l1-contracts/typechain/BridgehubFactory';
// import { Bridgehub } from '../../../../contracts/l1-contracts/typechain/Bridgehub';
// import { IL1NativeTokenVaultFactory } from '../../../../contracts/l1-contracts/typechain/IL1NativeTokenVaultFactory';
// import { IL2NativeTokenVaultFactory } from '../../../../contracts/l1-contracts/typechain/IL2NativeTokenVaultFactory';
import {
    L2_ASSET_ROUTER_ADDRESS,
    L2_BRIDGEHUB_ADDRESS,
    L2_NATIVE_TOKEN_VAULT_ADDRESS,
    REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
    L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR,
    BRIDGEHUB_L2_CANONICAL_TRANSACTION_ABI,
    ETH_ADDRESS_IN_CONTRACTS
    // BRIDGEHUB_L2_TRANSACTION_REQUEST_ABI
} from '../src/constants';
import { RetryProvider } from '../src/retry-provider';
// import { ETH_ADDRESS_IN_CONTRACTS } from 'zksync-ethers/build/utils';
import { waitForBlockToBeFinalizedOnL1, waitUntilBlockFinalized } from '../src/helpers';
// import { cwd } from 'process';

export function readContract(path: string, fileName: string, contractName?: string) {
    contractName = contractName || fileName;
    return JSON.parse(fs.readFileSync(`${path}/${fileName}.sol/${contractName}.json`, { encoding: 'utf-8' }));
}

const bridgehubInterface = readContract(
    '../../../contracts/l1-contracts/artifacts/contracts/bridgehub',
    'Bridgehub'
).abi;
// const assetRouterInterface = readContract(
//     '../../../contracts/l1-contracts/artifacts/contracts/bridge/asset-router',
//     'L2AssetRouter'
// ).abi;

const ntvInterface = readContract(
    '../../../contracts/l1-contracts/artifacts/contracts/bridge/ntv',
    'L2NativeTokenVault'
).abi;

const mailboxInterface = readContract(
    '../../../contracts/l1-contracts/artifacts/contracts/state-transition/chain-deps/facets',
    'Mailbox',
    'MailboxFacet'
).abi;

const INTEROP_TX_TYPE = 253;
const INTEROP_TX_TYPE_BIG_INT = 253n;

describe('Interop checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let bob: zksync.Wallet;
    let aliceOtherChain: zksync.Wallet;
    let bobOtherChain: zksync.Wallet;
    let richWallet: zksync.Wallet;
    let l2ProviderOtherChain: RetryProvider;
    let l2Wallet: Wallet;
    let sendingChainId = 271;
    // let secondChainId = 272;
    let secondChainId = 505;
    let l1Bridgehub: ethers.Contract;
    // let l1Mailbox: ethers.Contract;
    let tokenDetails: Token;
    let assetId: string;
    let zkAssetId: string;
    let bridgehub: ethers.Contract;
    let l2NativeTokenVault: ethers.Contract;
    let l2NativeTokenVaultOtherChain: ethers.Contract;
    let tokenAddressOtherChain: string;
    const timeout = 10000;
    let depth = 0;

    // let baseTokenDetails: Token;
    // let aliceErc20: zksync.Contract;
    let tokenErc20OtherChain: zksync.Contract;
    let zkErc20: zksync.Contract;
    let swap: ethers.Contract;
    const richPk = '0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110';

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        bob = testMaster.newEmptyAccount();
        // const url = alice._providerL2!().getRpcUrl();
        const url = testMaster.environment().l2NodeUrl;
        // const url2 = 'http://localhost:3050';
        const url2 = 'http://localhost:3050';
        console.log('kl todo url', url);

        l2ProviderOtherChain = new RetryProvider(
            {
                url: url2,
                timeout: 1200 * 1000
            },
            undefined,
            testMaster.reporter
        );
        aliceOtherChain = new zksync.Wallet(alice.privateKey, l2ProviderOtherChain, alice.providerL1!);
        bobOtherChain = new zksync.Wallet(bob.privateKey, l2ProviderOtherChain, bob.providerL1!);
        richWallet = new zksync.Wallet(richPk, alice._providerL2(), alice._providerL1());

        // l2Provider2 = new Provider('http://localhost:3050');
        // l2Provider2 = new ethers.JsonRpcProvider('http://localhost:3050');
        const l2Provider = new ethers.JsonRpcProvider(url);

        l2Wallet = new Wallet(alice.privateKey, l2Provider);
        const l1Wallet = new Wallet(alice.privateKey, alice.providerL1!);
        const bridgeContracts = await alice.getL1BridgeContracts();
        const l1BridgehubAddress = await bridgeContracts.shared.BRIDGE_HUB();
        l1Bridgehub = new ethers.Contract(l1BridgehubAddress, bridgehubInterface, l1Wallet);
        const chainAddress = await l1Bridgehub.getHyperchain(secondChainId);
        // l1Mailbox = new ethers.Contract(chainAddress, mailboxInterface, l1Wallet);

        tokenDetails = testMaster.environment().erc20Token;
        // baseTokenDetails = testMaster.environment().baseToken;
        // aliceErc20 = new zksync.Contract(tokenDetails.l2Address, zksync.utils.IERC20, alice);

        // console.log('kl alice pk', bob.privateKey);
        console.log('kl alice pk', alice.privateKey);
        console.log('alice address', alice.getAddress());
        console.log('bob address', bob.getAddress());
    });

    test('Can perform a deposit', async () => {
        const amount = 100_000_000_000_000n; // 1 wei is enough. // ethers.parseEther('100')
        const gasPrice = await scaledGasPrice(alice);

        console.log(
            'main wallet eth balance',
            await alice._providerL1!().getBalance(await testMaster.mainAccount().getAddress())
        );
        const transactionResponse = await richWallet._signerL1!().sendTransaction({
            to: aliceOtherChain.address,
            value: ethers.parseEther('100') //amount*10n
        });

        const receipt = await transactionResponse.wait();
        await new Promise((resolve) => setTimeout(resolve, 2000));
        console.log('kl todo first transfer');

        const transactionResponse1 = await richWallet._signerL1!().sendTransaction({
            to: bobOtherChain.address,
            value: ethers.parseEther('100') //amount*10n
        });
        const receipt1 = await transactionResponse1.wait();
        await new Promise((resolve) => setTimeout(resolve, 2000));
        console.log('kl todo second transfer');

        // 5. Send the eth tx transaction

        // 5. Send the token tx transaction
        const l1Erc20ABI = [
            'function mint(address to, uint256 amount)',
            'function approve(address spender, uint256 value)',
            'function balanceOf(address account) view returns (uint256)',
            'function allowance(address owner, address spender) view returns (uint256)'
        ];
        const l1Erc20Contract = new ethers.Contract(tokenDetails.l1Address, l1Erc20ABI, alice._signerL1!());
        const baseMintPromise = l1Erc20Contract.mint(aliceOtherChain.address, amount);

        const receipt2 = await baseMintPromise;
        await new Promise((resolve) => setTimeout(resolve, 2000));
        console.log('kl todo first mint');

        const baseMintPromise2 = l1Erc20Contract.mint(bobOtherChain.address, amount);

        const receipt3 = await baseMintPromise2;
        await new Promise((resolve) => setTimeout(resolve, 2000));

        // console.log('kl todo receipt', receipt2);
        console.log('kl alice eth balance', await alice._providerL1!().getBalance(aliceOtherChain.address));
        console.log('bob eth balance', await alice._providerL1!().getBalance(bobOtherChain.address));

        await expect(
            await alice.deposit({
                token: tokenDetails.l1Address,
                amount,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: {
                    gasPrice
                },
                overrides: {
                    gasPrice
                }
            })
        ).toBeAccepted([]);
        await new Promise((resolve) => setTimeout(resolve, 2000));
        console.log('kl todo first deposit');

        await expect(
            await bobOtherChain.deposit({
                token: tokenDetails.l1Address,
                amount,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: {
                    gasPrice
                },
                overrides: {
                    gasPrice
                }
            })
        ).toBeAccepted([]);

        console.log('second deposit');

        await expect(
            await bobOtherChain.deposit({
                token: ETH_ADDRESS_IN_CONTRACTS,
                amount,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: {
                    gasPrice
                }
            })
        ).toBeAccepted([]);

        await expect(
            await bobOtherChain.deposit({
                token: ETH_ADDRESS_IN_CONTRACTS,
                amount,
                to: await aliceOtherChain.getAddress(),
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: {
                    gasPrice
                }
            })
        ).toBeAccepted([]);

        console.log('after third deposit');

        bridgehub = new ethers.Contract(L2_BRIDGEHUB_ADDRESS, bridgehubInterface, l2Wallet);
        l2NativeTokenVault = new ethers.Contract(L2_NATIVE_TOKEN_VAULT_ADDRESS, ntvInterface, l2Wallet);
        l2NativeTokenVaultOtherChain = new ethers.Contract(
            L2_NATIVE_TOKEN_VAULT_ADDRESS,
            ntvInterface,
            aliceOtherChain
        );
        assetId = await l2NativeTokenVault.assetId(tokenDetails.l2Address);
        tokenAddressOtherChain = await l2NativeTokenVaultOtherChain.tokenAddress(assetId);
        console.log('kl todo tokenAddressOtherChain', tokenAddressOtherChain);
        tokenErc20OtherChain = new zksync.Contract(tokenAddressOtherChain, zksync.utils.IERC20, aliceOtherChain);

        const ZkSyncERC20 = await readContract(
            '../../../contracts/l1-contracts/artifacts-zk/contracts/dev-contracts',
            'TestnetERC20Token'
        );
        // const contractFactory = new zksync.ContractFactory(ZkSyncERC20.abi, ZkSyncERC20.bytecode, alice);

        zkErc20 = await deployContract(bobOtherChain, ZkSyncERC20, ['ZKsync', 'ZK', 18]);
        const zkErc20Alice = new ethers.Contract(await zkErc20.getAddress(), ZkSyncERC20.abi, aliceOtherChain);
        //zkErc20.connect(alice)
        // console.log(zkErc20Alice.interface)
        console.log('zkErc20 deployed');

        const ZkSyncSwap = await readContract('./artifacts-zk/contracts/Swap', 'Swap');

        swap = await deployContract(bobOtherChain, ZkSyncSwap, [tokenAddressOtherChain, await zkErc20.getAddress()]);
        await new Promise((resolve) => setTimeout(resolve, timeout));
        await (await zkErc20.mint(await swap.getAddress(), ethers.parseEther('1000'))).wait();
        await (await zkErc20.mint(await alice.getAddress(), ethers.parseEther('1000'))).wait();
        let allowance = await zkErc20.allowance(await alice.getAddress(), L2_NATIVE_TOKEN_VAULT_ADDRESS);
        console.log('allowance', allowance);
        if (allowance < ethers.parseEther('100')) {
            await (await zkErc20Alice.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, ethers.parseEther('100'))).wait();
        }
        allowance = await zkErc20.allowance(await alice.getAddress(), L2_NATIVE_TOKEN_VAULT_ADDRESS);
        console.log('allowance after', allowance);

        console.log('kl todo zkErc20', await zkErc20.getAddress(), await swap.getAddress());
        if ((await l2NativeTokenVaultOtherChain.assetId(await zkErc20.getAddress())) == ethers.ZeroHash) {
            const registerTx = await l2NativeTokenVaultOtherChain.registerToken(await zkErc20.getAddress());
            const receiptRegister = await registerTx.wait();
            // console.log('kl todo receiptRegister', receiptRegister);
        }
        await new Promise((resolve) => setTimeout(resolve, timeout));
    });

    test('Can send and receive interop tx', async () => {
        // await new Promise((resolve) => setTimeout(resolve, 30000));

        const tokenBalanceBefore = await tokenErc20OtherChain.balanceOf(aliceOtherChain.address);
        console.log('kl todo tokenBalanceBefore', tokenBalanceBefore, performance.now());
        await sendTransferTx();
        await new Promise((resolve) => setTimeout(resolve, timeout));
        const tokenBalanceAfterTransfer = await tokenErc20OtherChain.balanceOf(aliceOtherChain.address);
        console.log('kl todo tokenBalanceAfter transfer', tokenBalanceAfterTransfer, performance.now());

        // // console.log("kl todo interface", tokenErc20OtherChain.interface)
        // // console.log("kl todo swap interface", tokenErc20OtherChain.allowance.getFragment())
        // // console.log("measuring get allowance time", performance.now())
        // // const tokenAllowanceBefore = await tokenErc20OtherChain.allowance(await alice.getAddress(), await swap.getAddress());
        // // console.log('kl todo tokenAllowanceBefore', tokenAllowanceBefore, performance.now());
        // await sendSwapApproveTx()
        // await new Promise((resolve) => setTimeout(resolve, 5* timeout));
        // // const tokenAllowanceAfter = await tokenErc20OtherChain.allowance(await alice.getAddress(), await swap.getAddress());
        // // console.log('kl todo tokenAllowanceAfter', tokenAllowanceAfter, performance.now());
        // // expect(tokenAllowanceAfter).toBeGreaterThan(tokenAllowanceBefore);

        const zkTokenBalanceBeforeSwap = await zkErc20.balanceOf(await alice.getAddress());
        await sendSwapTx();
        await new Promise((resolve) => setTimeout(resolve, timeout));
        // const tokenBalanceAfterSwap = await tokenErc20OtherChain.balanceOf(await alice.getAddress());
        const zkTokenBalanceAfterSwap = await zkErc20.balanceOf(await alice.getAddress());
        console.log('kl todo zk token balance change', zkTokenBalanceAfterSwap - zkTokenBalanceBeforeSwap);
        // console.log("kl todo tokenBalanceAfterSwap", tokenBalanceAfterSwap, tokenBalanceAfterSwap - tokenBalanceAfterTransfer)

        // await setNTVApprove();

        zkAssetId = await l2NativeTokenVaultOtherChain.assetId(await zkErc20.getAddress());
        await sendTransferBackTx();
        await new Promise((resolve) => setTimeout(resolve, timeout));
        const zkTokenAddress = await l2NativeTokenVault.tokenAddress(zkAssetId);
        console.log('zk assetId, tokenAddress', zkAssetId, zkTokenAddress);

        // tokenAddressOtherChain = await l2NativeTokenVaultOtherChain.tokenAddress(assetId);
        // tokenErc20OtherChain = new zksync.Contract(tokenAddressOtherChain, zksync.utils.IERC20, aliceOtherChain);
        // const tokenBalanceAfter = await tokenErc20OtherChain.balanceOf(aliceOtherChain.address);
        // console.log('kl todo token assetId', assetId);
        // console.log('kl todo tokenAddressOtherChain', tokenAddressOtherChain);
        // console.log('kl todo aliceOtherChain', aliceOtherChain.address);
        // console.log('kl todo tokenBalanceBefore', tokenBalanceBefore);
        // console.log('kl todo tokenBalanceAfter', tokenBalanceAfter);
        // expect(tokenBalanceAfter).toBeGreaterThan(tokenBalanceBefore);
    });

    async function sendTransferTx() {
        const amount = ethers.parseEther('0.1');
        const mintValue = ethers.parseEther('0.2');
        const secondBridgeCalldata = ethers.concat([
            '0x01',
            new ethers.AbiCoder().encode(
                ['bytes32', 'bytes'],
                [assetId, new ethers.AbiCoder().encode(['uint256', 'address'], [amount, await alice.getAddress()])]
            )
        ]);
        const tx1 = await requestL2TransactionTwoBridges(mintValue, secondBridgeCalldata);
        const receipt1 = await tx1.wait();
        // await waitForBlockToBeFinalizedOnL1(alice, receipt1!.blockNumber);
        // console.log('kl todo receipt1', receipt1);
        await new Promise((resolve) => setTimeout(resolve, timeout));

        await readAndBroadcastInteropTx(tx1.hash, alice, aliceOtherChain);
    }

    async function sendSwapApproveTx() {
        const amount = ethers.parseEther('0.1');
        const mintValue = ethers.parseEther('0.2');
        const l2Calldata = tokenErc20OtherChain.interface.encodeFunctionData('approve', [
            await swap.getAddress(),
            amount
        ]);
        const tx1 = await requestL2TransactionDirect(mintValue, tokenAddressOtherChain, 0n, l2Calldata);
        const receipt1 = await tx1.wait();
        // await waitForBlockToBeFinalizedOnL1(alice, receipt1!.blockNumber);
        await new Promise((resolve) => setTimeout(resolve, timeout));

        await readAndBroadcastInteropTx(tx1.hash, alice, aliceOtherChain);
    }

    async function sendSwapTx() {
        const mintValue = ethers.parseEther('0.2');
        const amount = ethers.parseEther('0.1');

        const l2Calldata = swap.interface.encodeFunctionData('swap', [amount]);
        const tx1 = await requestL2TransactionDirect(mintValue, await swap.getAddress(), 0n, l2Calldata);
        await readAndBroadcastInteropTx(tx1, alice, aliceOtherChain);
    }

    async function sendNTVApprove() {
        const amount = ethers.parseEther('0.1');
        const mintValue = ethers.parseEther('0.2');
        const l2Calldata = tokenErc20OtherChain.interface.encodeFunctionData('approve', [
            L2_NATIVE_TOKEN_VAULT_ADDRESS,
            amount
        ]);
        const tx1 = await requestL2TransactionDirect(mintValue, await zkErc20.getAddress(), 0n, l2Calldata);
        const receipt1 = await tx1.wait();
        // await waitForBlockToBeFinalizedOnL1(alice, receipt1!.blockNumber);
        await new Promise((resolve) => setTimeout(resolve, timeout));

        await readAndBroadcastInteropTx(tx1.hash, alice, aliceOtherChain);
    }

    async function sendTransferBackTx() {
        const amount = ethers.parseEther('0.1'); //100_000_000_000n//
        const mintValue = ethers.parseEther('0.2'); //100_000_000_000n//
        // const secondBridgeCalldata = "0x00"
        const secondBridgeCalldata = ethers.concat([
            '0x01',
            new ethers.AbiCoder().encode(
                ['bytes32', 'bytes'],
                [zkAssetId, new ethers.AbiCoder().encode(['uint256', 'address'], [amount, await alice.getAddress()])]
            )
        ]);

        const input = {
            chainId: sendingChainId.toString(),
            mintValue,
            l2Value: 0,
            l2GasLimit: 30000000,
            l2GasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
            refundRecipient: alice.address,
            secondBridgeAddress: L2_ASSET_ROUTER_ADDRESS,
            secondBridgeValue: 0,
            secondBridgeCalldata: secondBridgeCalldata
        };
        console.log('kl wrapped tx', input);

        const data = bridgehub.interface.encodeFunctionData('requestL2TransactionTwoBridges', [input]);

        // const startingInput = {
        //     chainId: secondChainId.toString(),
        //     mintValue: mintValue*2n,
        //     l2Value: 0,
        //     l2GasLimit: 10000000,
        //     l2GasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
        //     refundRecipient: alice.address,
        //     secondBridgeAddress: L2_ASSET_ROUTER_ADDRESS,
        //     secondBridgeValue: 0,
        //     secondBridgeCalldata: secondBridgeCalldata
        // }

        // let request = await bridgehub.requestL2TransactionTwoBridges.populateTransaction(input);
        // console.log("kl todo data", data, mintValue*2n, L2_BRIDGEHUB_ADDRESS)
        const tx1 = await requestL2TransactionDirect(mintValue * 60n + 1n, L2_BRIDGEHUB_ADDRESS, mintValue, data);
        // await tx1
        await readAndBroadcastInteropTx(tx1.hash, alice, aliceOtherChain);
    }

    async function requestL2TransactionTwoBridges(mintValue: bigint, secondBridgeCalldata: string) {
        const input = {
            chainId: secondChainId.toString(),
            mintValue,
            l2Value: 0,
            l2GasLimit: 30000000,
            l2GasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
            refundRecipient: alice.address,
            secondBridgeAddress: L2_ASSET_ROUTER_ADDRESS,
            secondBridgeValue: 0,
            secondBridgeCalldata: secondBridgeCalldata
        };

        let request = await bridgehub.requestL2TransactionTwoBridges.populateTransaction(input);
        request.value = mintValue;
        request.from = l2Wallet.address;

        const tx1 = await bridgehub.requestL2TransactionTwoBridges(input, {
            value: request.value,
            gasLimit: 30000000
        });

        const receipt1 = await tx1.wait();
        // await waitForBlockToBeFinalizedOnL1(alice, receipt1!.blockNumber);
        return tx1;
    }
    // bytes l2Calldata;
    // uint256 l2GasLimit;
    // uint256 l2GasPerPubdataByteLimit;
    // bytes[] factoryDeps;
    // address refundRecipient;
    async function requestL2TransactionDirect(
        mintValue: bigint,
        l2Contract: string,
        l2Value: bigint,
        l2Calldata: string
    ) {
        const input = {
            chainId: secondChainId.toString(),
            mintValue,
            l2Contract,
            l2Value,
            l2Calldata,
            l2GasLimit: 30000000,
            l2GasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
            factoryDeps: [],
            refundRecipient: await alice.getAddress()
        };
        // console.log("inputs", input)
        let request = await bridgehub.requestL2TransactionDirect.populateTransaction(input);
        request.value = mintValue;
        request.from = l2Wallet.address;
        const tx1 = await bridgehub.requestL2TransactionDirect(input, {
            value: request.value,
            gasLimit: 30000000
        });
        // const tx1 = (await zkErc20.mint(await swap.getAddress(), ethers.parseEther('1000')))
        const receipt1 = await tx1.wait();
        // await waitForBlockToBeFinalizedOnL1(alice, receipt1!.blockNumber);
        return tx1;
    }

    async function readAndBroadcastInteropTx(
        txHash: string,
        senderWallet: zksync.Wallet,
        receiverWallet: zksync.Wallet
    ) {
        let { l1BatchNumber, l2MessageIndex, l2TxNumberInBlock, message, proof } = {
            l1BatchNumber: 0,
            l2MessageIndex: 0,
            l2TxNumberInBlock: 0,
            message: '',
            proof: ['']
        };
        try {
            console.log('reading ', txHash);
            let {
                l1BatchNumber: l1BatchNumberRead,
                l2TxNumberInBlock: l2TxNumberInBlockRead,
                message: messageRead
            } = await senderWallet.getFinalizeWithdrawalParamsWithoutProof(txHash, 0);
            console.log('logs read');
            l1BatchNumber = l1BatchNumberRead || 0;
            // l2MessageIndex = l2MessageIndexRead || 0;
            l2TxNumberInBlock = l2TxNumberInBlockRead || 0;
            message = messageRead || '';
            // proof = proofRead || [];
            // console.log("kl todo proof length", proofRead.length)
            if (message === '') {
                return;
            }
        } catch (e) {
            console.log('kl todo error in interop message', e);
            return;
        }
        console.log('kl todo interop message', depth);

        // to just test the receive part
        // let message = ethers.ZeroHash;
        // const l2TxNumberInBlock = 0;
        // const l1BatchNumber = 0;
        // const l2MessageIndex = 0;

        let decodedRequest = ethers.AbiCoder.defaultAbiCoder().decode(
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
        console.log('xl2Input', xl2Input);
        console.log('mintValue', xl2Input.reserved[0]);
        console.log('mintvalue hex', xl2Input.reserved[0].toString(16));

        const log = {
            l2ShardId: 0,
            isService: true,
            txNumberInBatch: l2TxNumberInBlock,
            sender: L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR,
            key: ethers.keccak256(ethers.AbiCoder.defaultAbiCoder().encode(['address'], [alice.address])),
            value: ethers.keccak256(ethers.AbiCoder.defaultAbiCoder().encode(['bytes'], [message]))
        };
        const L2LogString =
            'tuple(uint8 l2ShardId,bool isService,uint16 txNumberInBatch,address sender,bytes32 key,bytes32 value)';
        const leafHash = ethers.keccak256(ethers.AbiCoder.defaultAbiCoder().encode([L2LogString], [log]));
        // // const proof1 =  ethers.ZeroHash + ethers.AbiCoder.defaultAbiCoder().encode(['uint256'], [271]).slice(2);
        // // const proof1 = ethers.ZeroHash + ethers.AbiCoder.defaultAbiCoder().encode(['bytes'], [ethers.ZeroHash]).slice(2);
        // console.log('kl todo leafHash', leafHash);
        const proof1 =
            ethers.ZeroHash +
            ethers.AbiCoder.defaultAbiCoder()
                // .encode(['uint256', 'uint256', 'uint256', 'uint256', 'uint256'], [1, 2, 3, 4, 5])
                .encode(
                    ['uint256', 'uint256', 'uint256', 'bytes32'],
                    [(await senderWallet._providerL2().getNetwork()).chainId, l1BatchNumber, l2MessageIndex, leafHash]
                )
                .slice(2);

        // to pring proof for debugging in bootloader tests
        // const result: string[] = [];

        // for (let i = 2; i < proof1.length; i += 2) {
        //     result.push(proof1.slice(i, i + 2)); // Slice the string into pairs of 2 characters
        // }
        // process.stdout.write(JSON.stringify(result.map((r) => parseInt(r, 16))) + '\n');

        /// sending the interop tx

        const nonce = Math.floor(Math.random() * 1000000);
        let toMintString;

        const interopTx = {
            type: INTEROP_TX_TYPE,
            from: '0x' + xl2Input.from.toString(16).padStart(40, '0'),
            to: '0x' + xl2Input.to.toString(16).padStart(40, '0'),
            chainId: (await receiverWallet._providerL2().getNetwork()).chainId,
            data: xl2Input.data,
            nonce: nonce,
            customData: {
                paymaster_params: { paymaster: ethers.ZeroAddress, paymasterInput: '0x' },
                merkleProof: proof1,
                fullFee: '0xf000000000000000', //"0x"+xl2Input.reserved[0].toString(16).slice(0, xl2Input.reserved[0].toString(16).length -2),
                toMint:
                    (xl2Input.reserved[0].toString(16).length % 2 == 0 ? '0x' : '0x0') +
                    xl2Input.reserved[0].toString(16),
                refundRecipient: await alice.getAddress()
                // customSignature: ethers.ZeroHash
            },
            maxFeePerGas: xl2Input.maxFeePerGas,
            maxPriorityFeePerGas: xl2Input.maxPriorityFeePerGas,
            gasLimit: xl2Input.gasLimit,
            value: xl2Input.value // ethers.parseEther('2')
        };
        const interopTxAsCanonicalTx = {
            txType: INTEROP_TX_TYPE_BIG_INT,
            from: interopTx.from,
            to: interopTx.to,
            gasLimit: interopTx.gasLimit,
            pubdataPriceLimit: 50000n,
            // gasPerPubdataByteLimit: 800n,
            maxFeePerGas: interopTx.maxFeePerGas,
            maxPriorityFeePerGas: 0,
            paymaster: interopTx.customData.paymaster_params.paymaster,
            nonce: interopTx.nonce,
            value: interopTx.value,
            reserved: [interopTx.customData.toMint, interopTx.customData.refundRecipient, '0x00', '0x00'], ///
            data: interopTx.data,
            signature: '0x',
            factoryDeps: [],
            paymasterInput: '0x',
            reservedDynamic: proof1
        };
        const hexTx = zksync.utils.serializeEip712(interopTx);
        console.log('kl todo interopTx', interopTx);
        // console.log('kl todo serialized tx', hexTx, nonce.toString(16));
        // const modified = {...interopTx, nonce: 123456};
        // console.log('kl todo serialized tx', zksync.utils.serializeEip712(modified), (123456).toString(16));
        // alice.provider.getRpcTransaction(interopTx);
        // console.log("kl todo interopTx sending", performance.now())
        const tx = await l2ProviderOtherChain.broadcastTransaction(hexTx);
        // console.log(interopTxAsCanonicalTx) //
        const BRIDGEHUB_L2_CANONICAL_TRANSACTION_ABI2 =
            'tuple(uint256 txType,uint256 from, uint256 to, uint256 gasLimit,uint256 pubdataPriceLimit, uint256 maxFeePerGas, uint256 maxPriorityFeePerGas,  uint256 paymaster, uint256 nonce, uint256 value, uint256[4] reserved, bytes data, bytes signature,  uint256[] factoryDeps, bytes paymasterInput, bytes reservedDynamic)';

        const encodedTx = ethers.AbiCoder.defaultAbiCoder().encode(
            [BRIDGEHUB_L2_CANONICAL_TRANSACTION_ABI2],
            [interopTxAsCanonicalTx]
        );
        console.log('encoded tx', encodedTx.slice(2));
        const interopTxHash = ethers.keccak256(ethers.getBytes(encodedTx));
        // process.stdout.write(JSON.stringify(ethers.getBytes(encodedTx) +'\n'));
        console.log('hash', interopTxHash);
        // console.log("hash 2", ethers.keccak256(ethers.toUtf8Bytes(encodedTx.slice(2))))
        // console.log("kl todo interopTx sent", performance.now())
        // const receipt = await tx.wait();

        console.log('kl todo interop sent with destination', interopTx.to);
        // await waitUntilBlockFinalized(aliceOtherChain, tx.blockNumber!);
        await new Promise((resolve) => setTimeout(resolve, timeout * 2));
        // console.log(" interop tx", tx)
        // console.log('events', await receiverWallet._providerL2().getTransactionReceipt(interopTxHash))
        console.log('receiver wallet', await receiverWallet.getAddress());
        console.log(await zkErc20.allowance(await receiverWallet.getAddress(), L2_NATIVE_TOKEN_VAULT_ADDRESS));
        // await receiverWallet.sendTransaction({
        //     to: interopTx.to,
        //     data: interopTx.data,
        //     value: 100_000
        // })
        depth = depth + 1;
        await readAndBroadcastInteropTx(interopTxHash, receiverWallet, senderWallet);
        depth = depth - 1;
    }
});
