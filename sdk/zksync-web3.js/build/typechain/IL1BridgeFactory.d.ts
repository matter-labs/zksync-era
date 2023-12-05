import { Signer } from "ethers";
import { Provider } from "@ethersproject/providers";
import type { IL1Bridge } from "./IL1Bridge";
export declare class IL1BridgeFactory {
    static connect(address: string, signerOrProvider: Signer | Provider): IL1Bridge;
}
