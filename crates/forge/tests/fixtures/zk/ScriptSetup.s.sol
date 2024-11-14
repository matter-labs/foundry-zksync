// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {Greeter} from "../src/Greeter.sol";

contract ScriptSetupNonce is Script {
    function setUp() public {
        uint256 initial_nonce = checkNonce(address(tx.origin));
        // Perform transactions and deploy contracts in setup to increment nonce and verify broadcast nonce matches onchain
        new Greeter();
        new Greeter();
        new Greeter();
        new Greeter();
        assert(checkNonce(address(tx.origin)) == initial_nonce);
    }

    function run() public {
        uint256 initial_nonce = checkNonce(address(tx.origin));
        assert(initial_nonce == vm.getNonce(address(tx.origin)));
        Greeter not_broadcasted_greeter = new Greeter();
        not_broadcasted_greeter.greeting("john");
        assert(checkNonce(address(tx.origin)) == initial_nonce);
        vm.startBroadcast();
        Greeter greeter = new Greeter();
        greeter.greeting("john");
        NonceChecker checker = new NonceChecker();
        checker.assertNonce(vm.getNonce(address(tx.origin)) + 1);
        vm.stopBroadcast();
    }

    function checkNonce(address addr) public returns (uint256) {
        vm.prank(address(this), address(this));
        (bool success, bytes memory data) = address(0x000000000000000000000000000000000000008003).call(
            abi.encodeWithSignature("getMinNonce(address)", addr)
        );
        require(success, "Failed to get nonce");
        return abi.decode(data, (uint256));
    }
}

contract NonceChecker {
    function checkNonce() public returns (uint256) {
        (bool success, bytes memory data) = address(0x000000000000000000000000000000000000008003).call(
            abi.encodeWithSignature("getMinNonce(address)", address(tx.origin))
        );
        require(success, "Failed to get nonce");
        return abi.decode(data, (uint256));
    }

    function assertNonce(uint256 expected) public {
        uint256 real_nonce = checkNonce();
        require(real_nonce == expected, "Nonce mismatch");
    }
}
