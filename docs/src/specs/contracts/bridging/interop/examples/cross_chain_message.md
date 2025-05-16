# Interop Message Simple Use Case

Before we dive into the details of how the system works, let’s look at a simple use case for a DApp that decides to use
InteropMessage.

For this example, imagine a basic cross-chain contract where the `signup()` method can be called on chains B, C, and D
only if someone has first called `signup_open()` on chain A.

```solidity
// Contract deployed on chain A.
contract SignupManager {
  public bytes32 sigup_open_msg_hash;
  function signup_open() onlyOwner {
    // We are open for business
    signup_open_msg_hash = InteropCenter(INTEROP_CENTER_ADDRESS).sendInteropMessage("We are open");
  }
}

// Contract deployed on all other chains.
contract SignupContract {
  public bool signupIsOpen;
  // Anyone can call it.
  function openSignup(InteropMessage message, InteropProof proof) {
    InteropCenter(INTEROP_CENTER_ADDRESS).verifyInteropMessage(keccak(message), proof);
    require(message.sourceChainId == CHAIN_A_ID);
    require(message.sender == SIGNUP_MANAGER_ON_CHAIN_A);
    require(message.data == "We are open");
   signupIsOpen = true;
  }

  function signup() {
     require(signupIsOpen);
     signedUpUser[msg.sender] = true;
  }
}
```

In the example above, the `signupManager` on chain A calls the `signup_open` method. After that, any user on other
chains can retrieve the `signup_open_msg_hash`, obtain the necessary proof from the Gateway (or another source), and
call the `openSignup` function on any destination chain.