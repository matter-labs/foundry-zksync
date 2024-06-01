// SPDX-License-Identifier: MIT 
pragma solidity ^0.8.18;

import "forge-std/Test.sol";
import {LargeContract} from "./LargeContracts.sol";

contract ZkLargeFactoryDependenciesTest is Test {
    function testLargeFactoryDependenciesAreDeployedInBatches() public {
        new LargeContract();
    }
}