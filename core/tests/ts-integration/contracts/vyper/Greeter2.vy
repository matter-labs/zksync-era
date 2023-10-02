# @version ^0.3.3
# vim: ft=python

import Greeter as Greeter

greeter_contract: Greeter

@external
def __init__(greeter_address: address):
    self.greeter_contract = Greeter(greeter_address)

@external
@view
def test() -> String[100]:
    return self.greeter_contract.greet()
