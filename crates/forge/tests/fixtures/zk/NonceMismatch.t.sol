// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.18;

import "forge-std/Test.sol";
import {Greeter} from "../src/Greeter.sol";

contract NonceMismatchTest is Test {
    function setUp() public {
        // Deploy contracts in setup to increment nonce
        new Greeter();
        new Greeter();
        new Greeter();
        new Greeter();
    }

    function testNonceMismatch() public {
        uint256 nonce = vm.getNonce(address(tx.origin));
        assertEq(nonce, 2);

        // Deploy another contract
        new Greeter();

        uint256 newNonce = vm.getNonce(address(tx.origin));
        assertEq(newNonce, 2);
    }
}
