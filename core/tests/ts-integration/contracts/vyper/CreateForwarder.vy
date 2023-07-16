# @version ^0.3
# vim: ft=python

interface DeployMe:
    def setup(name: String[101]): nonpayable

@external
def deploy(_masterCopy: address, _name: String[101]) -> address:
    addr: address = create_forwarder_to(_masterCopy)
    # DeployMe.__init__ was not called, else this would fail
    DeployMe(addr).setup(_name)
    return addr
