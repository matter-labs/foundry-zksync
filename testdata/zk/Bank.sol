// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.28;

contract Bank {
    function balance() public view returns (uint256) {
        return address(this).balance;
    }

    constructor() payable {}

    receive() external payable {}
}
