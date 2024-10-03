/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import * as fs from 'fs';

import { TestMaster } from '../src/index';
// import { Token } from '../src/types';
// import { shouldChangeTokenBalances, shouldOnlyTakeFee } from '../src/modifiers/balance-checker';

// import * as zksync from 'zksync-ethers';
import * as zksync from 'zksync-ethers-interop-support';
import { Provider, Wallet, Signer } from 'ethers';
// import { BigNumber, utils as etherUtils } from 'ethers';
import * as ethers from 'ethers';
// import { scaledGasPrice, waitUntilBlockFinalized } from '../src/helpers';
// import { L2_DEFAULT_ETH_PER_ACCOUNT } from '../src/context-owner';
import { BridgehubFactory } from '../../../../contracts/l1-contracts/typechain/BridgehubFactory';
import { Bridgehub } from '../../../../contracts/l1-contracts/typechain/Bridgehub';
import { IL1NativeTokenVaultFactory } from '../../../../contracts/l1-contracts/typechain/IL1NativeTokenVaultFactory';
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
import { cwd } from 'process';

export function readContract(path: string, fileName: string) {
    return JSON.parse(fs.readFileSync(`${path}/${fileName}.sol/${fileName}.json`, { encoding: 'utf-8' }));
}

const bridgehubInterface = readContract(
    '../../../contracts/l1-contracts/artifacts/contracts/bridgehub',
    'Bridgehub'
).abi;
const assetRouterInterface = readContract(
    '../../../contracts/l1-contracts/artifacts/contracts/bridge/asset-router',
    'L2AssetRouter'
).abi;

const ntvInterface = readContract(
    '../../../contracts/l1-contracts/artifacts/contracts/bridge/ntv',
    'L2NativeTokenVault'
).abi;

const INTEROP_TX_TYPE = 253;

describe('Interop checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let bob: zksync.Wallet;
    let bobOtherChain: zksync.Wallet;
    let l2Provider: RetryProvider;
    let l2Wallet: Wallet;
    let l2Provider2: Provider;
    // let tokenDetails: Token;
    // let baseTokenDetails: Token;
    // let aliceErc20: zksync.Contract;
    // let l2NativeTokenVault: ethers.Contract;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        bob = testMaster.newEmptyAccount();

        l2Provider = new RetryProvider(
            {
                url: 'http://localhost:3050',
                timeout: 1200 * 1000
            },
            undefined,
            testMaster.reporter
        );
        bobOtherChain = new zksync.Wallet(bob.privateKey, l2Provider, bob.providerL1!);

        // l2Provider2 = new Provider('http://localhost:3050');
        // l2Provider2 = new ethers.JsonRpcProvider('http://localhost:3050');
        l2Provider2 = new ethers.JsonRpcProvider('http://localhost:3050');

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
        const l2NativeTokenVault = new ethers.Contract(L2_NATIVE_TOKEN_VAULT_ADDRESS, ntvInterface, l2Wallet);
        const assetId = await l2NativeTokenVault.getAssetId(
            (
                await alice.providerL1!.getNetwork()
            ).chainId,
            ETH_ADDRESS_IN_CONTRACTS
        );
        const balanceBefore = await bobOtherChain.getBalance();
        const input = {
            chainId: '271',
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

        let promise = new Promise((resolve) => setTimeout(resolve, 25000));
        await promise;
        const receipt1 = await tx1.wait();

        let { l1BatchNumber, l2MessageIndex, l2TxNumberInBlock, message, proof } = await alice.finalizeWithdrawalParams(
            tx1.hash,
            0
        );
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
                .encode(['uint256', 'uint256', 'uint256', 'bytes32'], [271, l1BatchNumber, l2MessageIndex, leafHash])
                .slice(2);

        // const proof1 =  ethers.ZeroHash + ethers.AbiCoder.defaultAbiCoder().encode(['uint256', 'uint256', 'uint256', 'bytes32', 'bytes32[]'], [271, l1BatchNumber, l2MessageIndex, leafHash, proof]).slice(2);
        console.log(proof1);

        const interopTx = {
            chainId: 271,
            to: '0x0000000000000000000000000000000000020004',
            from: L2_ASSET_ROUTER_ADDRESS,
            nonce: 0x654322,
            calldata: message as ethers.BytesLike,
            customData: {
                paymaster_params: { paymaster: ethers.ZeroAddress, paymasterInput: '0x' },
                merkleProof: proof1,
                fullFee: '0xf000000000000000',
                toMint: '0xf000000000000000000000000000000000',
                refundRecipient: await alice.getAddress()
            },
            maxFeePerGas: 276250000,
            maxPriorityFeePerGas: 140000000,
            gasLimit: '0x37E11D599',
            type: INTEROP_TX_TYPE,
            value: '0xf000000000000000'
        };

        console.log('kl todo serialized tx', zksync.utils.serializeEip712(interopTx));
        const hexTx = alice.provider.getRpcTransaction(interopTx);

        const tx = await alice.sendTransaction({
            to: L2_ASSET_ROUTER_ADDRESS,
            data: zksync.utils.serializeEip712(interopTx),
            gasLimit: 100000000,
            gasPrice: 100000000
        });

        console.log('kl todo tx', tx);
        const balanceAfter = await bobOtherChain.getBalance();
        console.log('Balance before: ', balanceBefore.toString());
        console.log('Balance after: ', balanceAfter.toString());
    });
});
