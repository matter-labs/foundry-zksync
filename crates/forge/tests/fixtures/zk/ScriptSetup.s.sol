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
        // Get initial nonce
        uint256 initial_nonce = checkNonce(address(tx.origin));
        assert(initial_nonce == vm.getNonce(address(tx.origin)));

        // Create and interact with non-broadcasted contract to verify nonce is not incremented
        Greeter notBroadcastGreeter = new Greeter();
        notBroadcastGreeter.greeting("john");
        assert(checkNonce(address(tx.origin)) == initial_nonce);

        // Start broadcasting transactions
        vm.startBroadcast();
        // Deploy and interact with broadcasted contracts
        Greeter greeter = new Greeter();
        greeter.greeting("john");

        // Deploy checker and verify nonce
        NonceChecker checker = new NonceChecker();
        // We expect the nonce to be incremented by 1 because the check is done in an external
        // call
        checker.assertNonce(vm.getNonce(address(tx.origin)) + 1);
        vm.stopBroadcast();
    }

    function checkNonce(address addr) public returns (uint256) {
        // We prank here to avoid accidentally "polluting" the nonce of `addr` during the call
        // for example when `addr` is `tx.origin`
        vm.prank(address(this), address(this));
        return NonceLib.getNonce(addr);
    }
}

contract NonceChecker {
    function checkNonce() public returns (uint256) {
        return NonceLib.getNonce(address(tx.origin));
    }

    function assertNonce(uint256 expected) public {
        uint256 real_nonce = checkNonce();
        require(real_nonce == expected, "Nonce mismatch");
    }
}

library NonceLib {
    address constant NONCE_HOLDER = address(0x8003);

    /// Retrieve tx nonce for `addr` from the NONCE_HOLDER system contract
    function getNonce(address addr) internal returns (uint256) {
        (bool success, bytes memory data) = NONCE_HOLDER.call(abi.encodeWithSignature("getMinNonce(address)", addr));
        require(success, "Failed to get nonce");
        return abi.decode(data, (uint256));
    }
}
