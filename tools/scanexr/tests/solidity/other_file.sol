// SPDX-License-Identifier: MIT
pragma solidity ^0.8.17;

function other(address a, uint b) pure returns (uint, address) {
    return (b, a);
}
