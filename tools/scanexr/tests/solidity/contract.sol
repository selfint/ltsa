// SPDX-License-Identifier: MIT
pragma solidity ^0.8.17;

import {other, hacky} from "./other_file.sol";

/*
EtherStore is a contract where you can deposit and withdraw ETH.
This contract is vulnerable to re-entrancy attack.
Let's see why.

1. Deploy EtherStore
2. Deposit 1 Ether each from Account 1 (Alice) and Account 2 (Bob) into EtherStore
3. Deploy Attack with address of EtherStore
4. Call Attack.attack sending 1 ether (using Account 3 (Eve)).
   You will get 3 Ethers back (2 Ether stolen from Alice and Bob,
   plus 1 Ether sent from this contract).

What happened?
Attack was able to call EtherStore.withdraw multiple times before
EtherStore.withdraw finished executing.

Here is how the functions were called
- Attack.attack
- EtherStore.deposit
- EtherStore.withdraw
- Attack fallback (receives 1 Ether)
- EtherStore.withdraw
- Attack.fallback (receives 1 Ether)
- EtherStore.withdraw
- Attack fallback (receives 1 Ether)
*/

contract EtherStore {
    mapping(address => uint) public balances;

    function deposit() public payable {
        balances[msg.sender] += msg.value;
    }

    function getSender2() private view returns (address) {
        return msg.sender;
    }

    function getSender() private view returns (address) {
        if (false) {
            return foo(0x0000000000000000000000000000000000000000, msg.sender);
        } else if (false) {
            return ret2(0x0000000000000000000000000000000000000000, msg.sender);
        }

        return getSender2();
    }

    function ret2(address a, address a2) private pure returns (address) {
        (uint c, address d) = other(a2, 1);

        return d;
    }

    function foo(address a, address b) private pure returns (address) {
        return a;
    }

    function withdraw() public {
        address sender = getSender();

        address bar = foo(sender, sender);

        uint bal = balances[bar];
        require(bal > 0);

        hacky(bar, bal);

        balances[bar] = 0;
    }

    // Helper function to check the balance of this contract
    function getBalance() public view returns (uint) {
        return address(this).balance;
    }
}
