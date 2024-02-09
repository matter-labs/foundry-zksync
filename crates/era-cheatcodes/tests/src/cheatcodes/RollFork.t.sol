// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract RollForkTest is Test {
    uint256 mainnetFork;

    function setUp() public {
        mainnetFork = vm.createSelectFork("mainnet");
    }

    // test that we can switch between forks, and "roll" blocks
    function testCanRollFork() public {
        vm.selectFork(mainnetFork);

        uint256 mainBlock = block.number;

        vm.rollFork(block.number - 1);

        assertEq(block.number, mainBlock - 1);

        // can also roll by id
        uint256 otherMain = vm.createSelectFork("mainnet", block.number - 1);
        vm.selectFork(mainnetFork);
        vm.rollFork(otherMain, mainBlock - 10);

        assertEq(block.number, mainBlock - 1); // should not have rolled

        vm.selectFork(otherMain);

        assertEq(block.number, mainBlock - 10);
    }
}
