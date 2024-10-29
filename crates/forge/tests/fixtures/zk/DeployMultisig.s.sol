// SPDX-License-Identifier: MIT
pragma solidity ^0.8.17;

import "forge-std/Script.sol";
import "zksync-contracts/zksync-contracts/l2/system-contracts/libraries/SystemContractsCaller.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "../src/AAFactory.sol";
import "../src/TwoUserMultisig.sol";

contract DeployMultisig is Script {
    function run() external {
        // Owners for the multisig account
        // Can be random
        address owner1 = makeAddr("OWNER_1");
        address owner2 = makeAddr("OWNER_2");

        // Read artifact file and get the bytecode hash
        string memory artifact = vm.readFile("zkout/TwoUserMultisig.sol/TwoUserMultisig.json");
        bytes32 multisigBytecodeHash = vm.parseJsonBytes32(artifact, ".hash");
        console.log("Bytecode hash: %s", multisigBytecodeHash);
        bytes32 salt = "JUAN";

        vm.startBroadcast();
        AAFactory factory = new AAFactory(multisigBytecodeHash);
        console.log("Factory deployed at: ", address(factory));
        (bool _success,) = address(vm).call(abi.encodeWithSignature("zkUseFactoryDep(string)", "TwoUserMultisig"));
        require(_success, "Cheatcode failed");
        factory.deployAccount(salt, owner1, owner2);
        vm.stopBroadcast();
    }
}
