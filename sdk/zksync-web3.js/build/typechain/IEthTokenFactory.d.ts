import { Signer } from "ethers";
import { Provider } from "@ethersproject/providers";
import type { IEthToken } from "./IEthToken";
export declare class IEthTokenFactory {
    static connect(address: string, signerOrProvider: Signer | Provider): IEthToken;
}
