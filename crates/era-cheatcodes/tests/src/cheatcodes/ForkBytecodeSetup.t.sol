// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "forge-std/Test.sol";
import {Constants} from "./Constants.sol";

contract Target {
    function output() public pure returns (uint256) {
        return 255;
    }
}

contract ForkBytecodeSetupTest is Test {
    uint256 constant FORK_BLOCK = 19579636;

    function setUp() public {
        vm.createSelectFork("mainnet", FORK_BLOCK);
        Target target = new Target();
        require(255 == target.output(), "incorrect output after fork");
    }

    function testForkBytecodeSetupSuccess() public {}
}
