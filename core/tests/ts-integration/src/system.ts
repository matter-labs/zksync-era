import { BigNumber, BytesLike, ethers } from 'ethers';
import { Provider, utils } from 'zksync-ethers';

const L1_CONTRACTS_FOLDER = `${process.env.ZKSYNC_HOME}/contracts/l1-contracts/artifacts`;
const GOVERNANCE_ABI = new ethers.utils.Interface(
    require(`${L1_CONTRACTS_FOLDER}/governance/Governance.sol/Governance.json`).abi
);
const ADMIN_FACET_ABI = new ethers.utils.Interface(
    require(`${L1_CONTRACTS_FOLDER}/state-transition/chain-deps/facets/Admin.sol/AdminFacet.json`).abi
);

export interface ForceDeployment {
    // The bytecode hash to put on an address
    bytecodeHash: BytesLike;
    // The address on which to deploy the bytecodehash to
    newAddress: string;
    // Whether to call the constructor
    callConstructor: boolean;
    // The value with which to initialize a contract
    value: BigNumber;
    // The constructor calldata
    input: BytesLike;
}

// A minimized copy of the `diamondCut` function used in L1 contracts
function diamondCut(facetCuts: any[], initAddress: string, initCalldata: string): any {
    return {
        facetCuts,
        initAddress,
        initCalldata
    };
}
