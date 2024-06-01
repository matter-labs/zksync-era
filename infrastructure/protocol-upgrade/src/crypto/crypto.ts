import { getCryptoFileName, getUpgradePath, VerifierParams } from '../utils';
import fs from 'fs';
import { BytesLike, ethers } from 'ethers';
import { Command } from 'commander';
import { deployVerifier } from './deployer';

function saveVerificationKeys(
    recursionNodeLevelVkHash: BytesLike,
    recursionLeafLevelVkHash: BytesLike,
    recursionCircuitsSetVksHash: BytesLike,
    environment: string
) {
    recursionNodeLevelVkHash = recursionNodeLevelVkHash ?? process.env.CONTRACTS_FRI_RECURSION_NODE_LEVEL_VK_HASH;
    recursionLeafLevelVkHash = recursionLeafLevelVkHash ?? process.env.CONTRACTS_FRI_RECURSION_LEAF_LEVEL_VK_HASH;
    recursionCircuitsSetVksHash = recursionCircuitsSetVksHash ?? ethers.constants.HashZero;
    const verificationParams: VerifierParams = {
        recursionNodeLevelVkHash,
        recursionLeafLevelVkHash,
        recursionCircuitsSetVksHash
    };
    updateCryptoFile('keys', verificationParams, environment);
    console.log(`Verification keys ${JSON.stringify(verificationParams)} saved`);
}

function updateCryptoFile(name: string, values: any, environment: string) {
    const cryptoFile = getCryptoFileName(environment);

    if (!fs.existsSync(cryptoFile)) {
        let params = {};
        params[name] = values;
        fs.writeFileSync(cryptoFile, JSON.stringify(params, null, 2));
    } else {
        const cryptoData = JSON.parse(fs.readFileSync(cryptoFile, 'utf8'));
        cryptoData[name] = values;
        console.log(JSON.stringify(cryptoData, null, 2));
        fs.writeFileSync(cryptoFile, JSON.stringify(cryptoData, null, 2));
    }
}

export const command = new Command('crypto').description('Prepare crypto params');

command
    .command('save-verification-params')
    .description('Save verification params, if not provided, will be taken from env variables')
    .option('--recursion-node-level-vk <recursionNodeLevelVk>')
    .option('--recursion-leaf-level-vk <recursionLeafLevelVk>')
    .option('--recursion-circuits-set-vks <recursionCircuitsSetVks>')
    .option('--environment <environment>')
    .action(async (cmd) => {
        await saveVerificationKeys(
            cmd.recursionNodeLevelVk,
            cmd.recursionLeafLevelVk,
            cmd.recursionCircuitsSetVks,
            cmd.environment
        );
    });

command
    .command('deploy-verifier')
    .option('--l1rpc <l1Rpc>')
    .option('--private-key <privateKey>')
    .option('--create2-address <create2Address>')
    .option('--nonce <nonce>')
    .option('--gas-price <gasPrice>')
    .option('--environment <environment>')
    .option('--testnet-verifier')
    .description('Deploy verifier contract')
    .action(async (cmd) => {
        console.log('Deploying verifier contract');
        const path = getUpgradePath(cmd.environment);
        const tmpFile = `${path}/cryptoTmp.json`;
        await deployVerifier(
            cmd.testnetVerifier,
            cmd.l1Rpc,
            cmd.privateKey,
            cmd.create2Address,
            tmpFile,
            cmd.nonce,
            cmd.gasPrice
        );
        let tmpData = JSON.parse(fs.readFileSync(tmpFile, 'utf8'));
        console.log(`Verifier contract deployed at ${tmpData.address}`);
        updateCryptoFile('verifier', tmpData, cmd.environment);
        fs.unlinkSync(tmpFile);
    });
