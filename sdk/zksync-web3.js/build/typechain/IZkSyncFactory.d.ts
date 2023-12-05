import { Signer } from "ethers";
import { Provider } from "@ethersproject/providers";
import type { IZkSync } from "./IZkSync";
export declare class IZkSyncFactory {
    static connect(address: string, signerOrProvider: Signer | Provider): IZkSync;
}
