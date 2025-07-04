// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract CallAnyMethod {
    function callAnyMethod(address target, bytes memory data) public returns (bytes memory) {
        (bool success, bytes memory result) = target.call(data);
        require(success, "Call failed");
        return result;
    }
}
