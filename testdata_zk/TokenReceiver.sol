// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0;

interface IMyToken {
    function transfer(address to, uint256 amount) external returns (bool);
}

contract TokenReceiver {
    function receiveAndHoldToken(address token, uint256 amount) external {
        IMyToken(token).transfer(msg.sender, amount);
    }
}
