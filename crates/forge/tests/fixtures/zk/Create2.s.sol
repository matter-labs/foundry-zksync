// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import {Script} from "forge-std/Script.sol";
import {Greeter} from "../src/Greeter.sol";

contract Create2Script is Script {
    function run() external {
        (bool success,) = address(vm).call(abi.encodeWithSignature("zkVm(bool)", true));
        require(success, "zkVm() call failed");
        
        vm.startBroadcast();

        // Deploy Greeter using create2 with a salt
        bytes32 greeterSalt = bytes32("12345");
        Greeter greeter = new Greeter{salt: greeterSalt}();

        // Verify Greeter deployment
        require(address(greeter) != address(0), "Greeter deployment failed");

        // Test Greeter functionality
        string memory greeting = greeter.greeting("Alice");
        require(bytes(greeting).length > 0, "Greeter greeting failed");

        vm.stopBroadcast();
    }
}
