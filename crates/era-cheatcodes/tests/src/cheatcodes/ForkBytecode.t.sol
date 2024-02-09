// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "forge-std/Test.sol";
import {Constants} from "./Constants.sol";

contract Target {
    function output() public pure returns (uint256) {
        return 255;
    }
}

contract ForkBytecodeTest is Test {
    uint256 constant FORK_BLOCK = 19579636;

    function testCreateSelectForkHasNonDeployedBytecodes() public {
        console.log("run");
        vm.createSelectFork("mainnet", FORK_BLOCK);

        // target should be able to deploy
        Target target = new Target();
        require(255 == target.output(), "incorrect output after fork");
    }

    function testSelectForkForkHasNonDeployedBytecodes() public {
        vm.createSelectFork("mainnet", FORK_BLOCK);

        // target should be able to deploy
        Target target = new Target();
        require(255 == target.output(), "incorrect output after fork");
    }
}
