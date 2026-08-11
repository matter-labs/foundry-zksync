// SPDX-License-Identifier: BUSL-1.1
pragma solidity ^0.8.0;

contract Deposit {
    address public seller = msg.sender;
    mapping(address => uint256) public balance;

    function deposit() external payable {
        balance[msg.sender] += msg.value;
    }

    function withdraw() external {
        uint256 amount = balance[msg.sender];
        balance[msg.sender] = 0;
        (bool s,) = msg.sender.call{value: amount}("");
        require(s, "failed to send");
    }
}
