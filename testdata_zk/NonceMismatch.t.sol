// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.18;

import "utils/Test.sol";
import {Greeter} from "./Greeter.sol";

contract NonceMismatchTest is Test {
    uint256 initialNonce;

    function setUp() public {
        initialNonce = vm.getNonce(address(tx.origin));
        // Deploy contracts in setup to increment nonce
        new Greeter();
        new Greeter();
        new Greeter();
        new Greeter();
    }

    function testTxOriginNonceDoesNotUpdate() public {
        uint256 nonce = vm.getNonce(address(tx.origin));
        assertEq(nonce, 2);

        // Deploy another contract
        new Greeter();

        nonce = vm.getNonce(address(tx.origin));
        assertEq(nonce, 2);
    }

    function testTxOriginNonceDoesNotUpdateOnSetup() public {
        assertEq(vm.getNonce(address(tx.origin)), initialNonce);
    }
}
