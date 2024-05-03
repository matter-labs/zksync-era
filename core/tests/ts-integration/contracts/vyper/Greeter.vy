# @version ^0.3.3
# vim: ft=python

owner: public(address)
greeting: public(String[100])

# __init__ is not invoked when deployed from create_forwarder_to
@external
def __init__(greeting: String[64]):
  self.owner = msg.sender
  self.greeting = greeting

# Invoke once after create_forwarder_to
@external
def setup(_greeting: String[100]):
  assert self.owner == ZERO_ADDRESS, "owner != zero address"
  self.owner = msg.sender
  self.greeting = _greeting

@external
@view
def greet() -> String[100]:
    return self.greeting
