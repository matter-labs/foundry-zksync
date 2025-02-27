pragma solidity ^0.8.20;

import {Script} from "forge-std/Script.sol";
import {AAFactory} from "../src/AAFactory.sol";
import {console} from "forge-std/console.sol";
import "zksync-contracts/zksync-contracts/l2/system-contracts/Constants.sol";

contract DeployAA is Script {
    function run() external {
        vm.startBroadcast();
        // deploy resolver
        (bool _success,) = address(vm).call(abi.encodeWithSignature("zkUseFactoryDep(string)", "AAAccount"));
        string memory artifact = vm.readFile("./zkout/AAAccount.sol/AAAccount.json");
        bytes32 bytecodeHash = vm.parseJsonBytes32(artifact, ".hash");

        AAFactory factory = new AAFactory(bytecodeHash);
        bytes32 salt = bytes32("salt");
        address account = factory.deployAccount(salt);
        console.log(account);
        vm.stopBroadcast();
    }
}
