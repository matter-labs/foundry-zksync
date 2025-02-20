// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {Greeter} from "../src/Greeter.sol";

contract ScriptBroadcastInSetup is Script {
    function setUp() public {
        // Broadcasting in setup would lead in the past to a nonce mismatch when broadcasting again
        vm.startBroadcast();
        new Greeter();
        new Greeter();
        vm.stopBroadcast();
    }

    function run() public {
        // Create and interact with non-broadcasted contract to verify nonce is not incremented
        Greeter notBroadcastGreeter = new Greeter();
        notBroadcastGreeter.greeting("john");

        // Start broadcasting transactions
        vm.startBroadcast();
        // Deploy and interact with broadcasted contracts
        Greeter greeter = new Greeter();
        greeter.greeting("juan");

        vm.stopBroadcast();
    }
}