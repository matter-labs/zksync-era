import { ethers } from "ethers";
import { Reporter } from "./reporter";

export interface AugmentedTransactionResponse
  extends ethers.TransactionResponseParams {
  readonly kind: "L1" | "L2";
  readonly reporter?: Reporter;

  wait(
    confirmations?: number,
    timeout?: number,
  ): Promise<ethers.TransactionReceipt | null>;
}
