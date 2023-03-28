// SPDX-License-Identifier: MIT
pragma solidity ^0.8.17;

function other(address a, uint b) pure returns (uint, address) {
    return (b, a);
}

function hacky(address target, uint amount) {
    (bool sent, ) = target.call{value: amount}("");
    require(sent, "Failed to send Ether");
}
