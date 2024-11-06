// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {Greeter} from "../src/Greeter.sol";

contract ScriptSetupNonce is Script {
    address alice;

    function setUp() public {
        // Perform transactions and deploy contracts in setup to increment nonce and verify broadcast nonce matches onchain
        new Greeter();
        new Greeter();
        alice = makeAddr("alice");
        (bool success,) = address(alice).call{value: 1 ether}("");
        require(success, "Failed to send ether");
    }

    function run() public {
        vm.startBroadcast();
        Greeter greeter = new Greeter();
        greeter.greeting("john");
        assert(address(alice).balance == 1 ether);
        vm.stopBroadcast();
    }
}
