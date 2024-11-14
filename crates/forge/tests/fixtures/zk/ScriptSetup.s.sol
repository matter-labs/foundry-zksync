// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {Greeter} from "../src/Greeter.sol";
import {console} from "forge-std/console.sol";
    
contract ScriptSetupNonce is Script {

    function setUp() public {
        // Perform transactions and deploy contracts in setup to increment nonce and verify broadcast nonce matches onchain
        new Greeter();
        new Greeter();
        new Greeter();
        new Greeter();
    }

    function run() public {
        NonceChecker checker1 = new NonceChecker();
        checker1.checkNonce();
        checker1.checkNonce();
        vm.getNonce(address(tx.origin));
        checkNonce(tx.origin);
        vm.getNonce(address(tx.origin));
        vm.startBroadcast();
        Greeter greeter = new Greeter();
        greeter.greeting("john");
        NonceChecker checker = new NonceChecker();
        NonceChecker checker2 = new NonceChecker();
        checker.assertNonce(vm.getNonce(address(tx.origin)) + 1);
        vm.stopBroadcast();
    }

    function checkNonce(address addr) public returns (uint256) {
        vm.prank(address(this), address(this));
        (bool success, bytes memory data) = address(0x000000000000000000000000000000000000008003).call(abi.encodeWithSignature("getMinNonce(address)", addr));
        require(success, "Failed to get nonce");
        return abi.decode(data, (uint256));
    }
}

contract NonceChecker {
    function checkNonce() public returns (uint256) {
        (bool success, bytes memory data) = address(0x000000000000000000000000000000000000008003).call(abi.encodeWithSignature("getMinNonce(address)", address(tx.origin)));
        require(success, "Failed to get nonce");
        return abi.decode(data, (uint256));
    }

    function checkDeployNonce() public returns (uint256) {
        (bool success, bytes memory data) = address(0x000000000000000000000000000000000000008003).call(abi.encodeWithSignature("getDeploymentNonce(address)", address(tx.origin)));
        require(success, "Failed to get deploy nonce");
        return abi.decode(data, (uint256));
    }

    function assertNonce(uint256 expected) public {
        uint256 real_nonce = checkNonce();
        console.log("real_nonce", real_nonce);
        require(real_nonce == expected, "Nonce mismatch");
    }
}