// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {Greeter} from "../src/Greeter.sol";
import {VmExt} from "./ScriptSetup.s.sol";

contract ScriptBroadcastInSetup is Script {
     VmExt internal constant vmExt = VmExt(VM_ADDRESS);
    function setUp() public {
        // Store initial nonces
        uint64 initialTxNonce = vmExt.zkGetTransactionNonce(tx.origin);
        uint64 initialDeployNonce = vmExt.zkGetDeploymentNonce(tx.origin);

        // Broadcasting in setup
        vm.startBroadcast();
        Greeter greeter = new Greeter();  // Test deployment nonce and transaction nonce
        greeter.greeting("test");  // Test transaction nonce
        vm.stopBroadcast();

        // Nonces should have been incremented by 2 and 1 respectively to match the provider
        assert(vmExt.zkGetTransactionNonce(tx.origin) == initialTxNonce + 2);
        assert(vmExt.zkGetDeploymentNonce(tx.origin) == initialDeployNonce + 1);
    }

    function run() public {
        uint64 initialTxNonce = vmExt.zkGetTransactionNonce(tx.origin);
        uint64 initialDeployNonce = vmExt.zkGetDeploymentNonce(tx.origin);

        vm.startBroadcast();
        Greeter greeter = new Greeter();  // Should increment deployment nonce and transaction nonce
        greeter.greeting("juan");  // Should increment transaction nonce
        vm.stopBroadcast();

        // Verify both nonces were incremented
        assert(vmExt.zkGetTransactionNonce(tx.origin) == initialTxNonce + 2);
        assert(vmExt.zkGetDeploymentNonce(tx.origin) == initialDeployNonce + 1);
    }
}