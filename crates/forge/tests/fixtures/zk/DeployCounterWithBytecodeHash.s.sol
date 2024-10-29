// SPDX-License-Identifier: MIT
pragma solidity ^0.8.17;

import "forge-std/Script.sol";
import "zksync-contracts/zksync-contracts/l2/system-contracts/libraries/SystemContractsCaller.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "../src/Factory.sol";

contract DeployCounterWithBytecodeHash is Script {
    function run() external {
        // Read artifact file and get the bytecode hash
        string memory artifact = vm.readFile("zkout/Counter.sol/Counter.json");
        bytes32 counterBytecodeHash = vm.parseJsonBytes32(artifact, ".hash");
        bytes32 salt = "JUAN";

        vm.startBroadcast();
        Factory factory = new Factory(counterBytecodeHash);
        (bool _success,) = address(vm).call(abi.encodeWithSignature("zkUseFactoryDep(string)", "Counter"));
        require(_success, "Cheatcode failed");
        address counter = factory.deployAccount(salt);
        require(counter != address(0), "Counter deployment failed");
        vm.stopBroadcast();
    }
}
