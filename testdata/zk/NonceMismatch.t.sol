// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.18;

import "ds-test/test.sol";
import {Greeter} from "./Greeter.sol";
import "../cheats/Vm.sol";
// import "../default/logs/console.sol";

contract NonceMismatchTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);
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
