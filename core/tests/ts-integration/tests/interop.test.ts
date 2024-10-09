/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import * as fs from 'fs';

import { TestMaster } from '../src/index';
// import { Token } from '../src/types';
// import { shouldChangeTokenBalances, shouldOnlyTakeFee } from '../src/modifiers/balance-checker';

// import * as zksync from 'zksync-ethers';
import * as zksync from 'zksync-ethers-interop-support';
import { Provider, Wallet } from 'ethers';
// import { BigNumber, utils as etherUtils } from 'ethers';
import * as ethers from 'ethers';
// import { scaledGasPrice, waitUntilBlockFinalized } from '../src/helpers';
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
    L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR
} from '../src/constants';
import { RetryProvider } from '../src/retry-provider';
import { ETH_ADDRESS_IN_CONTRACTS } from 'zksync-ethers/build/utils';
import { waitForBlockToBeFinalizedOnL1 } from '../src/helpers';
// import { cwd } from 'process';

export function readContract(path: string, fileName: string) {
    return JSON.parse(fs.readFileSync(`${path}/${fileName}.sol/${fileName}.json`, { encoding: 'utf-8' }));
}

const bridgehubInterface = readContract(
    '../../../contracts/l1-contracts/artifacts/contracts/bridgehub',
    'Bridgehub'
).abi;
// const assetRouterInterface = readContract(
//     '../../../contracts/l1-contracts/artifacts/contracts/bridge/asset-router',
//     'L2AssetRouter'
// ).abi;

// const ntvInterface = readContract(
//     '../../../contracts/l1-contracts/artifacts/contracts/bridge/ntv',
//     'L2NativeTokenVault'
// ).abi;

const INTEROP_TX_TYPE = 253;

describe('Interop checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let bob: zksync.Wallet;
    let bobOtherChain: zksync.Wallet;
    let l2ProviderOtherChain: RetryProvider;
    let l2Wallet: Wallet;
    let l2Provider2: Provider;
    let sendingChainId = 271;
    let secondChainId = 505;
    // let tokenDetails: Token;
    // let baseTokenDetails: Token;
    // let aliceErc20: zksync.Contract;
    // let l2NativeTokenVault: ethers.Contract;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        bob = testMaster.newEmptyAccount();
        // const url = alice._providerL2!().getRpcUrl();
        const url = testMaster.environment().l2NodeUrl;
        const url2 = 'http://localhost:3050';
        console.log('kl todo url', url);
        const l2Provider = new RetryProvider(
            {
                url: url,
                timeout: 1200 * 1000
            },
            undefined,
            testMaster.reporter
        );
        l2ProviderOtherChain = new RetryProvider(
            {
                url: url2,
                timeout: 1200 * 1000
            },
            undefined,
            testMaster.reporter
        );
        bobOtherChain = new zksync.Wallet(bob.privateKey, l2ProviderOtherChain, bob.providerL1!);

        // l2Provider2 = new Provider('http://localhost:3050');
        // l2Provider2 = new ethers.JsonRpcProvider('http://localhost:3050');
        l2Provider2 = new ethers.JsonRpcProvider(url);

        l2Wallet = new Wallet(alice.privateKey, l2Provider2);

        // tokenDetails = testMaster.environment().erc20Token;
        // baseTokenDetails = testMaster.environment().baseToken;
        // aliceErc20 = new zksync.Contract(tokenDetails.l2Address, zksync.utils.IERC20, alice);
    });

    test('Can send and receive interop tx', async () => {
        console.log('kl todo', alice.privateKey);
        console.log('kl todo', bob.privateKey);

        const amount = ethers.parseEther('1');
        const mintValue = ethers.parseEther('2');

        const bridgehub = new ethers.Contract(L2_BRIDGEHUB_ADDRESS, bridgehubInterface, l2Wallet);
        // const l2NativeTokenVault = new ethers.Contract(L2_NATIVE_TOKEN_VAULT_ADDRESS, ntvInterface, l2Wallet);
        const assetId = ethers.keccak256(ethers.AbiCoder.defaultAbiCoder().encode(['uint256', 'address','address'], [271, L2_NATIVE_TOKEN_VAULT_ADDRESS, ETH_ADDRESS_IN_CONTRACTS]));
        // await l2NativeTokenVault.assetId(
        //     (
        //         await alice.providerL1!.getNetwork()
        //     ).chainId,
        //     ETH_ADDRESS_IN_CONTRACTS
        // );
        const balanceBefore = await bobOtherChain.getBalance();
        const input = {
            chainId: sendingChainId.toString(),
            mintValue,
            l2Value: amount,
            l2GasLimit: 1000000,
            l2GasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
            refundRecipient: alice.address,
            secondBridgeAddress: L2_ASSET_ROUTER_ADDRESS,
            secondBridgeValue: 0,
            secondBridgeCalldata: ethers.concat([
                '0x01',
                new ethers.AbiCoder().encode(
                    ['bytes32', 'bytes'],
                    [assetId, new ethers.AbiCoder().encode(['uint256', 'address'], [amount, await bob.getAddress()])]
                )
            ])
        };
        console.log('kl todo input');
        let request = await bridgehub.requestL2TransactionTwoBridges.populateTransaction(input);
        request.value = mintValue;
        request.from = l2Wallet.address;

        const tx1 = await bridgehub.requestL2TransactionTwoBridges(input, {
            value: request.value,
            gasLimit: 100000000
        });

        const receipt1 = await tx1.wait();
        await waitForBlockToBeFinalizedOnL1(alice, receipt1!.blockNumber);
        console.log('kl todo receipt1', tx1.hash);

        let { l1BatchNumber, l2MessageIndex, l2TxNumberInBlock, message, proof } = await alice.finalizeWithdrawalParams(
            tx1.hash,
            0
        );
        console.log('kl todo message', message);
        console.log('\n');
        console.log('kl todo proof', proof);


        //         /// @dev Convert arbitrary-length message to the raw l2 log
        // function _L2MessageToLog(L2Message calldata _message) internal pure returns (L2Log memory) {
        //     return
        //         L2Log({
        //             l2ShardId: 0,
        //             isService: true,
        //             txNumberInBatch: _message.txNumberInBatch,
        //             sender: L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR,
        //             key: bytes32(uint256(uint160(_message.sender))),
        //             value: keccak256(_message.data)
        //         });
        // }
        // console.log('kl todo proof', proof);
        // let proof1 = ethers.ZeroHash + ethers.AbiCoder.defaultAbiCoder().encode(['bytes[]'], [proof]).slice(2);
        // function _proveL2LogInclusion(
        //     uint256 _batchNumber,
        //     uint256 _index,
        //     L2Log memory _log,
        //     bytes32[] calldata _proof
        // ) internal view returns (bool) {
        //     bytes32 hashedLog = keccak256(
        //     // solhint-disable-next-line func-named-parameters
        //     abi.encodePacked(_log.l2ShardId, _log.isService, _log.txNumberInBatch, _log.sender, _log.key, _log.value)
        // );
        // let message = ethers.ZeroHash;
        // const l2TxNumberInBlock = 0;
        // const l1BatchNumber = 0;
        // const l2MessageIndex = 0;
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
        // const proof1 =  ethers.ZeroHash + ethers.AbiCoder.defaultAbiCoder().encode(['uint256'], [271]).slice(2);
        // const proof1 = ethers.ZeroHash + ethers.AbiCoder.defaultAbiCoder().encode(['bytes'], [ethers.ZeroHash]).slice(2);
        console.log('kl todo leafHash', leafHash);
        const proof1 =
            ethers.ZeroHash +
            ethers.AbiCoder.defaultAbiCoder()
                // .encode(['uint256', 'uint256', 'uint256', 'uint256', 'uint256'], [1, 2, 3, 4, 5])
                .encode(['uint256', 'uint256', 'uint256', 'bytes32'], [sendingChainId, l1BatchNumber, l2MessageIndex, leafHash])
                .slice(2);
        
        const result: string[] = [];

        // const proof1 =  ethers.ZeroHash + ethers.AbiCoder.defaultAbiCoder().encode(['uint256', 'uint256', 'uint256', 'bytes32', 'bytes32[]'], [271, l1BatchNumber, l2MessageIndex, leafHash, proof]).slice(2);
        for (let i = 2; i < proof1.length; i += 2) {
            result.push(proof1.slice(i, i + 2)); // Slice the string into pairs of 2 characters
        }
        // console.log(result.map(r => parseInt(r, 16)));
        process.stdout.write(JSON.stringify(result.map(r => parseInt(r, 16))) + '\n');
        const sender = L2_ASSET_ROUTER_ADDRESS;
        const nonce = await bobOtherChain.providerL1!.getTransactionCount(sender) +1;
        const interopTx = {
            chainId: secondChainId,
            // to: '0x0000000000000000000000000000000000020004',
            to: bobOtherChain.address,
            from: sender,
            nonce: nonce,
            calldata: message as ethers.BytesLike,
            customData: {
                paymaster_params: { paymaster: ethers.ZeroAddress, paymasterInput: '0x' },
                merkleProof: proof1,
                fullFee: '0xf000000000000000',
                toMint: '0xf000000000000000000000000000000000',
                refundRecipient: await alice.getAddress(),
                // customSignature: ethers.ZeroHash
            },
            maxFeePerGas: 276250000,
            maxPriorityFeePerGas: 140000000,
            gasLimit: '0x37E11D599',
            type: INTEROP_TX_TYPE,
            value: '0xf000000000000000'
        };

        const hexTx = zksync.utils.serializeEip712(interopTx);
        console.log('kl todo serialized tx', hexTx);
        // alice.provider.getRpcTransaction(interopTx);


        const tx = await l2ProviderOtherChain.broadcastTransaction(hexTx);
        console.log('kl todo tx', tx);

        // console.log('kl todo tx', tx);
        const balanceAfter = await bobOtherChain.getBalance();
        console.log('Balance before: ', balanceBefore.toString());
        console.log('Balance after: ', balanceAfter.toString());
    });
});
