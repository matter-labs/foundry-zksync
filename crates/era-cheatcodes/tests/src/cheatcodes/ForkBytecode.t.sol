// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "forge-std/Test.sol";
import {Constants} from "./Constants.sol";

contract Target {
    function output() public pure returns (uint256) {
        return 255;
    }
}

contract ForkStorageTest is Test {
    uint256 constant FORK_BLOCK = 19579636;

    function testCreateSelectForkHasNonDeployedBytecodes() public {
        vm.createSelectFork("mainnet", FORK_BLOCK);

        // target should be able to deploy
        Target target = new Target();

        // Contract is still deployed
        require(255 == target.output(), "incorrect output after fork");

        // After fork, bytecode is remembered and contract is deployed
        Target newTarget = new Target();
        require(255 == newTarget.output(), "incorrect new target output");
    }

    function testSelectForkForkHasNonDeployedBytecodes() public {
        uint256 forkId = vm.createFork("mainnet", FORK_BLOCK + 100);
        vm.selectFork(forkId);

        // target should be able to deploy
        Target target = new Target();

        // Contract is still deployed
        require(255 == target.output(), "incorrect output after fork");

        // After fork, bytecode is remembered and contract is deployed
        Target newTarget = new Target();
        require(255 == newTarget.output(), "incorrect new target output");
    }
}