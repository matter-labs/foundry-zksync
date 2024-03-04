// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, Vm, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract CheatcodeReadCallers is Test {
    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;
    address constant TEST_ORIGIN = 0xdEBe90b7BFD87Af696B1966082F6515a6E72F3d8;

    function testNormalReadCallers() public {
        (Vm.CallerMode mode, address sender, address origin) = vm.readCallers();
        require(uint8(mode) == 0, "normal call mode");
        require(sender == msg.sender, "sender not overridden");
        require(origin == tx.origin, "origin not overridden");
    }

    function testPrankedReadCallers() public {
        vm.startPrank(TEST_ADDRESS);

        (Vm.CallerMode mode, address sender, address origin) = vm.readCallers();

        require(uint8(mode) == 4, "recurrent prank call mode");
        require(sender == TEST_ADDRESS, "sender overridden");
        require(origin == tx.origin, "origin not overridden");
    }

    function testFullyPrankedReadCallers() public {
        vm.startPrank(TEST_ADDRESS, TEST_ORIGIN);

        (Vm.CallerMode mode, address sender, address origin) = vm.readCallers();

        require(uint8(mode) == 4, "recurrent prank call mode");
        require(sender == TEST_ADDRESS, "sender overridden");
        require(origin == TEST_ORIGIN, "origin overridden");
    }
}
