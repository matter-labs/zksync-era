import { Signer } from "ethers";
import { Provider } from "@ethersproject/providers";
import type { IL2Bridge } from "./IL2Bridge";
export declare class IL2BridgeFactory {
    static connect(address: string, signerOrProvider: Signer | Provider): IL2Bridge;
}
