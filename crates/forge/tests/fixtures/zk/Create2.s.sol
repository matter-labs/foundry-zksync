// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import {Script} from "forge-std/Script.sol";
import {Greeter} from "../src/Greeter.sol";
import {Create2Utils} from "../src/Create2Utils.sol";

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

        // Verify the deployed address matches the expected address
        bytes32 bytecodeHash = getBytecodeHash("zkout/Greeter.sol/Greeter.json");
        address expectedAddress = Create2Utils.computeCreate2Address(
            address(0x0000000000000000000000000000000000010000), // DEFAULT_CREATE2_DEPLOYER_ZKSYNC
            greeterSalt,
            bytecodeHash,
            keccak256(abi.encode())
        );

        require(address(greeter) == expectedAddress, "Deployed address doesn't match expected address");

        // Test Greeter functionality
        string memory greeting = greeter.greeting("Alice");
        require(bytes(greeting).length > 0, "Greeter greeting failed");

        vm.stopBroadcast();
    }

    function getBytecodeHash(string memory path) internal returns (bytes32 bytecodeHash) {
        string memory artifact = vm.readFile(path);
        bytecodeHash = vm.parseJsonBytes32(artifact, ".hash");
    }
}
