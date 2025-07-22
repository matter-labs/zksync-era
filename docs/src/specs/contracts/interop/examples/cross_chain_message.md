# Interop Message Simple Use Case

For this example, imagine you want to allow users to signup on multiple chains - but you want to coordinate when the signup starts.

With the help of interop, you only have to open the signup once, on one chain, no need to do that as many times as there are chains you want to open the signup on!

An example of this being done:

```solidity
// Contract deployed on chain A.
contract SignupManager {
  public bytes32 sigup_open_msg_hash;
  function signup_open() onlyOwner {
    // We are open for business
    signup_open_msg_hash = L1Messenger(L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR).sendToL1("We are open");
  }
}

// Contract deployed on all other chains.
contract SignupContract {
  public bool signupIsOpen;
  // Anyone can call it.
  function openSignup(uint256 _chainId, uint256 _batchNumber, uint256 _index, L2Message calldata _message, bytes32[] calldata _proof) {
    IZKChain(zkChain).proveL2MessageInclusionShared(_chainId, _batchNumber, _index, _message, proof);
    require(_chainId_ == CHAIN_A_ID);
    require(_message.sender == SIGNUP_MANAGER_ON_CHAIN_A);
    require(_message.data == "We are open");
    signupIsOpen = true;
  }

  function signup() {
     require(signupIsOpen);
     signedUpUser[msg.sender] = true;
  }
}
```

In the example above, the `signupManager` on chain `A` calls the `signup_open` method. After that, any user on other
chains can retrieve the `signup_open_msg_hash`, obtain the necessary proof from the Gateway (or another source), and
call the `openSignup` function on any destination chain. 

More details on the overall process can be read [here](../interop_messages.md).

You can also see another example in [Advanced guides](../../../../guides/advanced/19_interop_basics.md).
