// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract RollForkTest is Test {
    uint256 mainnetFork;

    function setUp() public {
        mainnetFork = vm.createFork("mainnet");
    }

    // test that we can switch between forks, and "roll" blocks
    function testCanRollFork() public {
        vm.selectFork(mainnetFork);

        uint256 mainBlock = block.number;

        console.log("target block_number: ", block.number - 1);
        console.log("before block_number: ", block.number);

        vm.rollFork(block.number - 1);

        console.log("after block_number: ", block.number);

        assertEq(block.number, mainBlock - 1);

        // can also roll by id
        uint256 otherMain = vm.createFork("mainnet", block.number - 1);
        vm.rollFork(otherMain, mainBlock - 10);

        console.log("same block_number: ", block.number);
        assertEq(block.number, mainBlock - 1); // should not have rolled

        vm.selectFork(otherMain);

        console.log("target block_number: ", mainBlock - 10);
        console.log("actual block_number: ", block.number);

        assertEq(block.number, mainBlock - 10);
    }
}
