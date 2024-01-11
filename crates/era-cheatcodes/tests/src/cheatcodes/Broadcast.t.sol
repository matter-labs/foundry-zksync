// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract ATest is Test {
    uint256 public changed = 0;

    function t(uint256 a) public returns (uint256) {
        uint256 b = 0;
        for (uint256 i; i < a; i++) {
            b += 1;
        }
        emit log_string("here");
        return b;
    }

    function pt(uint256 a) public payable returns (uint256) {
        return t(a);
    }

    function inc() public returns (uint256) {
        changed += 1;
        return changed;
    }

    function multiple_arguments(uint256 a, address b, uint256[] memory c) public returns (uint256) {}

    function echoSender() public view returns (address) {
        return msg.sender;
    }
}

contract BroadcastTest is Test {
    // 1st anvil account
    address public ACCOUNT_A = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;
    // 2nd anvil account
    address public ACCOUNT_B = 0x70997970C51812dc3A010C7d01b50e0d17dc79C8;

    function test_SimpleBroadcastDeploy() public {
        vm.startBroadcast(ACCOUNT_A);

        ATest test = new ATest();

        vm.stopBroadcast(); 

        // this wont generate tx to sign
        test.t(4);

        // this will
        vm.startBroadcast(ACCOUNT_B);

        test.t(2);

        vm.stopBroadcast();
    }

    function test_BroadcastValue() public {
        vm.startBroadcast(ACCOUNT_A);

        ATest test = new ATest();
        test.pt{ value: 42 }(16);

        vm.stopBroadcast();
    }

    function test_BroadcastGasLimit() public {
        vm.startBroadcast();

        ATest test = new ATest();
        test.t{gas: 12345678}(12345678);

        vm.stopBroadcast();
    }

    function test_BroadcastNonces() public {
        vm.startBroadcast(ACCOUNT_B);

        ATest test = new ATest();
        test.t(1);
        test.t(2);
        test.t(3);
        test.t(4);

       vm.stopBroadcast(); 
    }
}
