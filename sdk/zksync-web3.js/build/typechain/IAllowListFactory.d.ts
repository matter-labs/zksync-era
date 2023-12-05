import { Signer } from "ethers";
import { Provider } from "@ethersproject/providers";
import type { IAllowList } from "./IAllowList";
export declare class IAllowListFactory {
    static connect(address: string, signerOrProvider: Signer | Provider): IAllowList;
}
