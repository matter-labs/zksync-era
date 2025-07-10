import { MatcherModifier } from "../src/matchers/transaction-modifiers";

export declare global {
  function rawWriteToConsole(message: string, ...args: any[]);

  namespace jest {
    interface Matchers<R> {
      // Generic matchers

      /**
       * Fails the test with the provided message
       *
       * Note: `expect` expects a parameter to be provided, so you can pass anything there,
       * argument won't be used.
       * For example, `expect(null).fail("This shouldn't have happened!");`.
       *
       * @param message Message to be displayed in Jest output.
       */
      fail(message: string): R;

      // Ethereum primitives matchers

      /**
       * Checks if value represents a valid Ethereum address
       *
       * @param additionalInfo Optional message to be included if test fails.
       */
      toBeAddress(additionalInfo?: string): R;
      /**
       * Checks if value represents a valid Ethereum address
       *
       * @param additionalInfo Optional message to be included if test fails.
       */
      toBeHexString(additionalInfo?: string): R;

      // Transaction matchers

      /**
       * Checks that transaction is successfully executed by the server.
       * Does NOT support `.not` modifier. Use `toBeRejected` or `toBeReverted` instead.
       *
       * @param modifiers Optional list of transaction matcher modifiers to be applied to a receipt.
       * @param additionalInfo Optional message to be included if test fails.
       */
      toBeAccepted(
        modifiers?: MatcherModifier[],
        additionalInfo?: string,
      ): Promise<R>;
      /**
       * Checks that transaction is executed by server, but execution results in a revert.
       * Does NOT support `.not` modifier. Use `toBeAccepted` instead.
       *
       * @param modifiers Optional list of transaction matcher modifiers to be applied to a receipt.
       * @param additionalInfo Optional message to be included if test fails.
       */
      toBeReverted(
        modifiers?: MatcherModifier[],
        additionalInfo?: string,
      ): Promise<R>;
      /**
       * Checks that transaction is rejected by the API server.
       * Does NOT support `.not` modifier. Use `toBeAccepted` instead.
       *
       * @param errorSubstring Optional part of the error message that should be present in the API response.
       * @param additionalInfo Optional message to be included if test fails.
       */
      toBeRejected(
        errorSubstring?: string,
        additionalInfo?: string,
      ): Promise<R>;
      /**
       * Checks that eth_call is rejected by the API server.
       * Does NOT support `.not` modifier. Use `toBeAccepted` instead.
       *
       * @param revertReason Optional revert reason of eth_call.
       * @param encodedRevertReason Optional RLP encoded revert reason.
       * @param additionalInfo Optional message to be included if test fails.
       */
      toBeRevertedEthCall(
        revertReason?: string,
        encodedRevertReason?: string,
        additionalInfo?: string,
      ): Promise<R>;
      /**
       * Checks that eth_estimateGas is rejected by the API server.
       * Does NOT support `.not` modifier. Use `toBeAccepted` instead.
       *
       * @param revertReason Optional revert reason of eth_call.
       * @param encodedRevertReason Optional RLP encoded revert reason.
       * @param additionalInfo Optional message to be included if test fails.
       */
      toBeRevertedEstimateGas(
        revertReason?: string,
        encodedRevertReason?: string,
        additionalInfo?: string,
      ): Promise<R>;
    }
  }
}
